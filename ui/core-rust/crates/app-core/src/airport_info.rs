// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use chrono::{DateTime, Days, Duration, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use sunrise::{Coordinates, SolarDay, SolarEvent};

use crate::{had_ops::HadReadError, NavKvLookup, NavKvStore};

// AIM 4-3-3 uses 1,000 AGL unless an altitude is established; the 1,500 AGL
// large/turbine recommendation needs aircraft context this airport-only view lacks.
const DERIVED_TRAFFIC_PATTERN_ALTITUDE_AGL_FT: f64 = 1_000.0;
const MIN_RUNWAY_DIAGRAM_SCALE_FT: f64 = 5_000.0;
const FEET_PER_NAUTICAL_MILE: f64 = 6_076.115_49;
const MAX_RUNWAY_ENDPOINT_DISTANCE_FROM_AIRPORT_FT: f64 = 20.0 * FEET_PER_NAUTICAL_MILE;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirportInfoUiView {
    pub airport_id: String,
    pub name: String,
    pub elevation_label: String,
    pub traffic_pattern_altitude_label: String,
    pub traffic_pattern_altitude_source: String,
    pub local_time_label: String,
    pub utc_time_label: String,
    pub time_zone_label: String,
    pub sunrise: Option<AirportSolarEventUiView>,
    pub sunset: Option<AirportSolarEventUiView>,
    pub communications: Vec<AirportCommunicationUiView>,
    pub runway_diagram_complex: bool,
    pub runways: Vec<AirportRunwayUiView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AirportSolarEventUiView {
    pub local_time_label: String,
    pub utc_time_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_in_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AirportCommunicationUiView {
    pub label: String,
    pub value: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirportRunwayUiView {
    pub end_a_label: String,
    pub end_b_label: String,
    pub dimensions_label: String,
    pub surface_label: String,
    pub surface_color_key: String,
    pub diagram_end_a_x: f64,
    pub diagram_end_a_y: f64,
    pub diagram_end_b_x: f64,
    pub diagram_end_b_y: f64,
    pub diagram_width_ratio: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct AirportInfoRecord {
    schema_version: u32,
    airport_id: String,
    name: String,
    latitude: f64,
    longitude: f64,
    time_zone: String,
    elevation_msl_ft: Option<f64>,
    traffic_pattern_altitude_msl_ft: Option<f64>,
    #[serde(default)]
    communications: Vec<AirportCommunicationRecord>,
    #[serde(default)]
    contacts: Vec<AirportContactRecord>,
    #[serde(default)]
    runways: Vec<AirportRunwayRecord>,
}

#[derive(Debug, Clone, Deserialize)]
struct AirportCommunicationRecord {
    label: String,
    #[serde(default)]
    frequency: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AirportContactRecord {
    label: String,
    phone: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AirportRunwayRecord {
    length_ft: Option<f64>,
    width_ft: Option<f64>,
    surface: String,
    end_a: AirportRunwayEndRecord,
    end_b: AirportRunwayEndRecord,
}

#[derive(Debug, Clone, Deserialize)]
struct AirportRunwayEndRecord {
    ident: String,
    heading_true_deg: Option<f64>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    #[serde(default)]
    right_pattern: bool,
}

#[derive(Debug, Clone, Copy)]
struct RunwayDiagramGeometry {
    end_a_x: f64,
    end_a_y: f64,
    end_b_x: f64,
    end_b_y: f64,
    width_ratio: f64,
}

pub(crate) fn airport_info(
    store: &NavKvStore,
    airport_id: &str,
    now: DateTime<Utc>,
) -> Result<AirportInfoUiView, HadReadError> {
    let key = format!("airport/info/{}", airport_id.trim().to_ascii_uppercase());
    let record = match store.get_bytes(&key).map_err(HadReadError::Fatal)? {
        NavKvLookup::Hit(bytes) => serde_json::from_slice::<AirportInfoRecord>(&bytes)
            .map_err(|error| HadReadError::Fatal(format!("invalid {key}: {error}")))?,
        NavKvLookup::MissingKey => {
            return Err(HadReadError::Fatal(format!(
                "airport info is unavailable for {}",
                airport_id.trim().to_ascii_uppercase()
            )))
        }
        NavKvLookup::MissingPages(pages) => return Err(HadReadError::NeedPages(pages)),
    };
    project_airport_info(record, now).map_err(HadReadError::Fatal)
}

fn project_airport_info(
    record: AirportInfoRecord,
    now: DateTime<Utc>,
) -> Result<AirportInfoUiView, String> {
    if record.schema_version != 1 {
        return Err(format!(
            "airport info {} has schema_version {}; expected 1",
            record.airport_id, record.schema_version
        ));
    }
    let time_zone = record.time_zone.parse::<Tz>().map_err(|error| {
        format!(
            "airport info {} has invalid time zone {}: {error}",
            record.airport_id, record.time_zone
        )
    })?;
    let local_now = now.with_timezone(&time_zone);
    let elevation_label = record
        .elevation_msl_ft
        .map(|value| format!("{} ft MSL", rounded_feet(value)))
        .unwrap_or_else(|| "Not published".to_string());
    let (traffic_pattern_altitude_label, traffic_pattern_altitude_source) =
        traffic_pattern_altitude(
            record.elevation_msl_ft,
            record.traffic_pattern_altitude_msl_ft,
        );
    let (sunrise, sunset) = solar_event_views(
        record.latitude,
        record.longitude,
        record.elevation_msl_ft,
        time_zone,
        now,
    );
    let standalone_runway_scale_ft = record
        .runways
        .iter()
        .filter_map(|runway| runway.length_ft)
        .fold(MIN_RUNWAY_DIAGRAM_SCALE_FT, f64::max);
    let complex_runway_geometry =
        runway_complex_geometry(&record.runways, record.latitude, record.longitude);
    let runway_diagram_complex = complex_runway_geometry.is_some();
    let communications = record
        .communications
        .into_iter()
        .filter_map(|entry| {
            entry
                .frequency
                .filter(|value| !value.trim().is_empty())
                .map(|value| AirportCommunicationUiView {
                    label: entry.label,
                    value,
                    kind: "frequency".to_string(),
                })
        })
        .chain(record.contacts.into_iter().filter_map(|entry| {
            (!entry.phone.trim().is_empty()).then_some(AirportCommunicationUiView {
                label: format!("{} phone", entry.label),
                value: entry.phone,
                kind: "phone".to_string(),
            })
        }))
        .collect();
    let runways = record
        .runways
        .iter()
        .enumerate()
        .map(|(index, runway)| {
            let geometry = complex_runway_geometry
                .as_ref()
                .and_then(|geometries| geometries.get(index))
                .copied()
                .unwrap_or_else(|| standalone_runway_geometry(runway, standalone_runway_scale_ft));
            runway_ui_view(runway, geometry)
        })
        .collect();

    Ok(AirportInfoUiView {
        airport_id: record.airport_id,
        name: record.name,
        elevation_label,
        traffic_pattern_altitude_label,
        traffic_pattern_altitude_source,
        local_time_label: local_now.format("%H:%M %Z").to_string(),
        utc_time_label: now.format("%H%MZ").to_string(),
        time_zone_label: format!(
            "{} (UTC {})",
            local_now.format("%Z"),
            local_now.format("%z")
        ),
        sunrise,
        sunset,
        communications,
        runway_diagram_complex,
        runways,
    })
}

fn traffic_pattern_altitude(
    elevation_msl_ft: Option<f64>,
    published_msl_ft: Option<f64>,
) -> (String, String) {
    if let Some(published_msl_ft) = published_msl_ft {
        let agl = elevation_msl_ft.map(|elevation| published_msl_ft - elevation);
        return (
            match agl {
                Some(agl) => format!(
                    "{} ft MSL ({} ft AGL)",
                    rounded_feet(published_msl_ft),
                    rounded_feet(agl)
                ),
                None => format!("{} ft MSL", rounded_feet(published_msl_ft)),
            },
            "published".to_string(),
        );
    }
    let Some(elevation_msl_ft) = elevation_msl_ft else {
        return ("Not available".to_string(), "derived".to_string());
    };
    let derived_msl_ft =
        rounded_hundreds(elevation_msl_ft + DERIVED_TRAFFIC_PATTERN_ALTITUDE_AGL_FT);
    (
        format!(
            "{} ft MSL ({} ft AGL)",
            derived_msl_ft,
            rounded_feet(derived_msl_ft as f64 - elevation_msl_ft)
        ),
        "derived".to_string(),
    )
}

fn rounded_feet(value: f64) -> i64 {
    value.round() as i64
}

fn rounded_hundreds(value: f64) -> i64 {
    (value / 100.0).round() as i64 * 100
}

fn solar_event_views(
    latitude: f64,
    longitude: f64,
    elevation_msl_ft: Option<f64>,
    time_zone: Tz,
    now: DateTime<Utc>,
) -> (
    Option<AirportSolarEventUiView>,
    Option<AirportSolarEventUiView>,
) {
    let Some(coordinates) = Coordinates::new(latitude, longitude) else {
        return (None, None);
    };
    let local_date = now.with_timezone(&time_zone).date_naive();
    let altitude_m = elevation_msl_ft.unwrap_or_default().max(0.0) * 0.3048;
    let next_event = |event| {
        [local_date, local_date.checked_add_days(Days::new(1))?]
            .into_iter()
            .filter_map(|date| {
                SolarDay::new(coordinates, date)
                    .with_altitude(altitude_m)
                    .event_time(event)
            })
            .find(|event_at| *event_at >= now)
    };
    let sunrise_at = next_event(SolarEvent::Sunrise);
    let sunset_at = next_event(SolarEvent::Sunset);
    let next_at = [sunrise_at, sunset_at].into_iter().flatten().min();
    (
        sunrise_at.map(|at| solar_event_ui_view(at, time_zone, now, next_at == Some(at))),
        sunset_at.map(|at| solar_event_ui_view(at, time_zone, now, next_at == Some(at))),
    )
}

fn solar_event_ui_view(
    at: DateTime<Utc>,
    time_zone: Tz,
    now: DateTime<Utc>,
    is_next: bool,
) -> AirportSolarEventUiView {
    AirportSolarEventUiView {
        local_time_label: at.with_timezone(&time_zone).format("%H:%M %Z").to_string(),
        utc_time_label: at.format("%H%MZ").to_string(),
        next_in_label: is_next.then(|| format_duration_until(at - now)),
    }
}

fn format_duration_until(duration: Duration) -> String {
    let total_minutes = duration.num_minutes().max(0);
    format!("+{}:{:02}", total_minutes / 60, total_minutes % 60)
}

fn standalone_runway_geometry(
    runway: &AirportRunwayRecord,
    scale_ft: f64,
) -> RunwayDiagramGeometry {
    let heading = runway
        .end_a
        .heading_true_deg
        .or_else(|| runway.end_b.heading_true_deg.map(|value| value + 180.0))
        .unwrap_or_default()
        .to_radians();
    let half_length = runway.length_ft.unwrap_or_default() / scale_ft / 2.0;
    let dx = heading.sin() * half_length;
    let dy = -heading.cos() * half_length;
    RunwayDiagramGeometry {
        end_a_x: -dx,
        end_a_y: -dy,
        end_b_x: dx,
        end_b_y: dy,
        width_ratio: runway.width_ft.unwrap_or_default() / scale_ft,
    }
}

fn runway_complex_geometry(
    runways: &[AirportRunwayRecord],
    airport_latitude: f64,
    airport_longitude: f64,
) -> Option<Vec<RunwayDiagramGeometry>> {
    if runways.is_empty()
        || !airport_latitude.is_finite()
        || !airport_longitude.is_finite()
        || !(-90.0..=90.0).contains(&airport_latitude)
        || !(-180.0..=180.0).contains(&airport_longitude)
    {
        return None;
    }
    let longitude_scale_ft = 60.0 * FEET_PER_NAUTICAL_MILE * airport_latitude.to_radians().cos();
    let latitude_scale_ft = 60.0 * FEET_PER_NAUTICAL_MILE;
    let local_point = |end: &AirportRunwayEndRecord| {
        let latitude = end.latitude?;
        let longitude = end.longitude?;
        (latitude.is_finite()
            && longitude.is_finite()
            && (-90.0..=90.0).contains(&latitude)
            && (-180.0..=180.0).contains(&longitude))
        .then(|| {
            let longitude_delta = (longitude - airport_longitude + 180.0).rem_euclid(360.0) - 180.0;
            (
                longitude_delta * longitude_scale_ft,
                -(latitude - airport_latitude) * latitude_scale_ft,
            )
        })
        .filter(|(x, y)| x.hypot(*y) <= MAX_RUNWAY_ENDPOINT_DISTANCE_FROM_AIRPORT_FT)
    };
    let mut endpoints = Vec::with_capacity(runways.len());
    for runway in runways {
        let end_a = local_point(&runway.end_a)?;
        let end_b = local_point(&runway.end_b)?;
        let length_ft = (end_b.0 - end_a.0).hypot(end_b.1 - end_a.1);
        if length_ft < 1.0 {
            return None;
        }
        endpoints.push((end_a, end_b));
    }
    let (min_x, max_x, min_y, max_y) = endpoints.iter().flat_map(|(a, b)| [a, b]).fold(
        (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ),
        |(min_x, max_x, min_y, max_y), &(x, y)| {
            (min_x.min(x), max_x.max(x), min_y.min(y), max_y.max(y))
        },
    );
    let center_x = (min_x + max_x) / 2.0;
    let center_y = (min_y + max_y) / 2.0;
    let scale_ft = MIN_RUNWAY_DIAGRAM_SCALE_FT
        .max(max_x - min_x)
        .max(max_y - min_y);
    Some(
        runways
            .iter()
            .zip(endpoints)
            .map(|(runway, (end_a, end_b))| RunwayDiagramGeometry {
                end_a_x: (end_a.0 - center_x) / scale_ft,
                end_a_y: (end_a.1 - center_y) / scale_ft,
                end_b_x: (end_b.0 - center_x) / scale_ft,
                end_b_y: (end_b.1 - center_y) / scale_ft,
                width_ratio: runway.width_ft.unwrap_or_default() / scale_ft,
            })
            .collect(),
    )
}

fn runway_ui_view(
    runway: &AirportRunwayRecord,
    geometry: RunwayDiagramGeometry,
) -> AirportRunwayUiView {
    let (surface_label, surface_color_key) = runway_surface(&runway.surface);
    AirportRunwayUiView {
        end_a_label: format!(
            "RWY {} ({} pattern)",
            runway.end_a.ident,
            if runway.end_a.right_pattern {
                "right"
            } else {
                "left"
            }
        ),
        end_b_label: format!(
            "RWY {} ({} pattern)",
            runway.end_b.ident,
            if runway.end_b.right_pattern {
                "right"
            } else {
                "left"
            }
        ),
        dimensions_label: match (runway.length_ft, runway.width_ft) {
            (Some(length), Some(width)) => {
                format!("{}' x {}'", rounded_feet(length), rounded_feet(width))
            }
            (Some(length), None) => format!("{}' long", rounded_feet(length)),
            _ => "Dimensions not published".to_string(),
        },
        surface_label,
        surface_color_key,
        diagram_end_a_x: geometry.end_a_x,
        diagram_end_a_y: geometry.end_a_y,
        diagram_end_b_x: geometry.end_b_x,
        diagram_end_b_y: geometry.end_b_y,
        diagram_width_ratio: geometry.width_ratio,
    }
}

fn runway_surface(surface: &str) -> (String, String) {
    let mut parts = surface
        .split('-')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let condition = parts.last().and_then(|part| match *part {
        "E" => Some("excellent condition"),
        "G" => Some("good condition"),
        "F" => Some("fair condition"),
        "P" => Some("poor condition"),
        "L" => Some("failed condition"),
        _ => None,
    });
    if condition.is_some() {
        parts.pop();
    }
    let color_key = if parts.iter().any(|part| *part == "WATER") {
        "airport_runway_water"
    } else if parts
        .iter()
        .any(|part| matches!(*part, "ASPH" | "CONC" | "BIT" | "PEM" | "MATS" | "TREATED"))
    {
        "airport_runway_paved"
    } else if parts.iter().any(|part| *part == "TURF") {
        "airport_runway_turf"
    } else {
        "airport_runway_unpaved"
    };
    let surface_name = parts
        .into_iter()
        .map(|part| match part {
            "ASPH" => "Asphalt",
            "CONC" => "Concrete",
            "BIT" => "Bituminous",
            "PEM" => "Paved",
            "TURF" => "Turf",
            "GRVL" | "GRAVEL" => "Gravel",
            "DIRT" => "Dirt",
            "WATER" => "Water",
            "SNOW" => "Snow",
            "ICE" => "Ice",
            "MATS" => "Mats",
            "TREATED" => "Treated",
            _ => part,
        })
        .collect::<Vec<_>>()
        .join(" / ");
    let label = match (surface_name.is_empty(), condition) {
        (false, Some(condition)) => format!("{surface_name}, {condition}"),
        (false, None) => surface_name,
        (true, Some(condition)) => condition.to_string(),
        (true, None) => "Surface not published".to_string(),
    };
    (label, color_key.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_record() -> AirportInfoRecord {
        AirportInfoRecord {
            schema_version: 1,
            airport_id: "KRNT".to_string(),
            name: "Renton Municipal".to_string(),
            latitude: 47.493,
            longitude: -122.216,
            time_zone: "America/Los_Angeles".to_string(),
            elevation_msl_ft: Some(32.0),
            traffic_pattern_altitude_msl_ft: None,
            communications: vec![AirportCommunicationRecord {
                label: "Tower".to_string(),
                frequency: Some("124.7".to_string()),
            }],
            contacts: Vec::new(),
            runways: vec![AirportRunwayRecord {
                length_ft: Some(5_382.0),
                width_ft: Some(200.0),
                surface: "ASPH-CONC-G".to_string(),
                end_a: AirportRunwayEndRecord {
                    ident: "16".to_string(),
                    heading_true_deg: Some(174.0),
                    latitude: None,
                    longitude: None,
                    right_pattern: false,
                },
                end_b: AirportRunwayEndRecord {
                    ident: "34".to_string(),
                    heading_true_deg: Some(354.0),
                    latitude: None,
                    longitude: None,
                    right_pattern: true,
                },
            }],
        }
    }

    #[test]
    fn airport_info_projects_dst_solar_runway_and_derived_tpa_in_core() {
        let now = Utc.with_ymd_and_hms(2026, 7, 26, 2, 34, 0).unwrap();
        let view = project_airport_info(sample_record(), now).expect("view");

        assert_eq!(view.local_time_label, "19:34 PDT");
        assert_eq!(view.utc_time_label, "0234Z");
        assert_eq!(view.time_zone_label, "PDT (UTC -0700)");
        assert_eq!(
            view.traffic_pattern_altitude_label,
            "1000 ft MSL (968 ft AGL)"
        );
        assert_eq!(view.traffic_pattern_altitude_source, "derived");
        assert_eq!(view.communications[0].value, "124.7");
        assert_eq!(view.runways[0].end_b_label, "RWY 34 (right pattern)");
        assert_eq!(
            view.runways[0].surface_label,
            "Asphalt / Concrete, good condition"
        );
        assert_eq!(view.runways[0].surface_color_key, "airport_runway_paved");
        assert!(!view.runway_diagram_complex);
        assert!(
            view.sunrise
                .iter()
                .chain(view.sunset.iter())
                .filter(|event| event.next_in_label.is_some())
                .count()
                == 1
        );
    }

    #[test]
    fn runway_complex_requires_and_preserves_every_runway_position() {
        let now = Utc.with_ymd_and_hms(2026, 7, 26, 2, 34, 0).unwrap();
        let mut record = sample_record();
        record.runways[0].end_a.latitude = Some(47.4638);
        record.runways[0].end_a.longitude = Some(-122.3110);
        record.runways[0].end_b.latitude = Some(47.4380);
        record.runways[0].end_b.longitude = Some(-122.3112);
        let mut west_runway = record.runways[0].clone();
        west_runway.end_a.ident = "16R".to_string();
        west_runway.end_b.ident = "34L".to_string();
        west_runway.end_a.longitude = Some(-122.3179);
        west_runway.end_b.longitude = Some(-122.3181);
        record.runways.push(west_runway);

        let complex = project_airport_info(record.clone(), now).expect("complex view");
        assert!(complex.runway_diagram_complex);
        let east_center_x =
            (complex.runways[0].diagram_end_a_x + complex.runways[0].diagram_end_b_x) / 2.0;
        let west_center_x =
            (complex.runways[1].diagram_end_a_x + complex.runways[1].diagram_end_b_x) / 2.0;
        assert!(east_center_x > west_center_x);

        record.runways[1].end_b.longitude = None;
        let fallback = project_airport_info(record, now).expect("fallback view");
        assert!(!fallback.runway_diagram_complex);
        for runway in fallback.runways {
            assert!((runway.diagram_end_a_x + runway.diagram_end_b_x).abs() < 1e-12);
            assert!((runway.diagram_end_a_y + runway.diagram_end_b_y).abs() < 1e-12);
        }
    }

    #[test]
    fn published_tpa_uses_published_msl_and_reports_agl() {
        assert_eq!(
            traffic_pattern_altitude(Some(632.0), Some(1_600.0)),
            (
                "1600 ft MSL (968 ft AGL)".to_string(),
                "published".to_string()
            )
        );
    }

    #[test]
    fn airport_time_zone_follows_winter_standard_time() {
        let now = Utc.with_ymd_and_hms(2026, 1, 26, 2, 34, 0).unwrap();
        let view = project_airport_info(sample_record(), now).expect("view");

        assert_eq!(view.local_time_label, "18:34 PST");
        assert_eq!(view.time_zone_label, "PST (UTC -0800)");
    }
}
