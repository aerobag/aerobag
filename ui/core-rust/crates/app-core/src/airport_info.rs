// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use chrono::{DateTime, Days, Duration, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use sunrise::{Coordinates, SolarDay, SolarEvent};

use crate::{had_ops::HadReadError, NavKvLookup, NavKvStore, TimeDisplayMode};

// AIM 4-3-3 uses 1,000 AGL unless an altitude is established; the 1,500 AGL
// large/turbine recommendation needs aircraft context this airport-only view lacks.
const DERIVED_TRAFFIC_PATTERN_ALTITUDE_AGL_FT: f64 = 1_000.0;
const MIN_RUNWAY_DIAGRAM_SCALE_FT: f64 = 5_000.0;
const FEET_PER_NAUTICAL_MILE: f64 = 6_076.115_49;
const MAX_RUNWAY_ENDPOINT_DISTANCE_FROM_AIRPORT_FT: f64 = 20.0 * FEET_PER_NAUTICAL_MILE;
const RUNWAY_DIAGRAM_RUNWAY_EXTENT_SCALE: f64 = 0.95;
const PATTERN_INDICATOR_LEG_LENGTH: f64 = 0.065;
const PATTERN_INDICATOR_THRESHOLD_GAP: f64 = 0.025;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirportInfoUiView {
    pub airport_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_label: Option<String>,
    pub elevation_label: String,
    pub traffic_pattern_altitude_label: String,
    pub traffic_pattern_altitude_source: String,
    pub time_label: String,
    pub time_display_action_id: String,
    pub time_zone_label: String,
    pub sunrise: Option<AirportSolarEventUiView>,
    pub sunset: Option<AirportSolarEventUiView>,
    pub communications: Vec<AirportCommunicationUiView>,
    pub fact_sections: Vec<AirportInfoFactSectionUiView>,
    pub runways_section_title: String,
    pub runway_diagram_complex: bool,
    pub runways: Vec<AirportRunwayUiView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AirportInfoFactSectionUiView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub facts: Vec<AirportInfoFactUiView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AirportInfoFactUiView {
    pub label: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_in_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AirportSolarEventUiView {
    pub time_label: String,
    pub time_display_action_id: String,
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
    pub diagram_end_a_pattern: Option<AirportRunwayPatternUiView>,
    pub diagram_end_b_pattern: Option<AirportRunwayPatternUiView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirportRunwayPatternUiView {
    pub base_x: f64,
    pub base_y: f64,
    pub corner_x: f64,
    pub corner_y: f64,
    pub final_x: f64,
    pub final_y: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct AirportInfoRecord {
    schema_version: u32,
    airport_id: String,
    name: String,
    #[serde(default)]
    location_label: Option<String>,
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
    time_display_mode: TimeDisplayMode,
) -> Result<AirportInfoUiView, HadReadError> {
    let record = read_airport_info_record(store, airport_id)?.ok_or_else(|| {
        HadReadError::Fatal(format!(
            "airport info is unavailable for {}",
            airport_id.trim().to_ascii_uppercase()
        ))
    })?;
    project_airport_info(record, now, time_display_mode).map_err(HadReadError::Fatal)
}

pub(crate) fn airport_elevation_msl_ft(
    store: &NavKvStore,
    airport_id: &str,
) -> Result<Option<f64>, HadReadError> {
    Ok(read_airport_info_record(store, airport_id)?.and_then(|record| record.elevation_msl_ft))
}

fn read_airport_info_record(
    store: &NavKvStore,
    airport_id: &str,
) -> Result<Option<AirportInfoRecord>, HadReadError> {
    let key = format!("airport/info/{}", airport_id.trim().to_ascii_uppercase());
    match store.get_bytes(&key).map_err(HadReadError::Fatal)? {
        NavKvLookup::Hit(bytes) => serde_json::from_slice::<AirportInfoRecord>(&bytes)
            .map(Some)
            .map_err(|error| HadReadError::Fatal(format!("invalid {key}: {error}"))),
        NavKvLookup::MissingKey => Ok(None),
        NavKvLookup::MissingPages(pages) => Err(HadReadError::NeedPages(pages)),
    }
}

fn project_airport_info(
    record: AirportInfoRecord,
    now: DateTime<Utc>,
    time_display_mode: TimeDisplayMode,
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
        time_display_mode,
    );
    let standalone_runway_scale_ft = record
        .runways
        .iter()
        .filter_map(|runway| runway.length_ft)
        .fold(MIN_RUNWAY_DIAGRAM_SCALE_FT, f64::max);
    let complex_runway_geometry =
        runway_complex_geometry(&record.runways, record.latitude, record.longitude);
    let runway_diagram_complex = complex_runway_geometry.is_some();
    let communications: Vec<AirportCommunicationUiView> = record
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

    let mut primary_facts = vec![
        AirportInfoFactUiView {
            label: "Airport elevation".to_string(),
            value: elevation_label.clone(),
            next_in_label: None,
            action_id: None,
            link_url: None,
        },
        AirportInfoFactUiView {
            label: "Traffic pattern altitude".to_string(),
            value: format!("{traffic_pattern_altitude_label} {traffic_pattern_altitude_source}"),
            next_in_label: None,
            action_id: None,
            link_url: None,
        },
        AirportInfoFactUiView {
            label: "Time at airport".to_string(),
            value: crate::format_time_of_day(
                now.timestamp_millis(),
                time_display_mode,
                time_zone,
                airport_time_style(time_display_mode),
            )
            .with_basis(),
            next_in_label: None,
            action_id: Some(crate::TOGGLE_TIME_DISPLAY_MODE_ACTION_ID.to_string()),
            link_url: None,
        },
        AirportInfoFactUiView {
            label: "Time zone".to_string(),
            value: format!(
                "{} (UTC {})",
                local_now.format("%Z"),
                local_now.format("%z")
            ),
            next_in_label: None,
            action_id: None,
            link_url: None,
        },
    ];
    for (label, event) in [("Sunrise", sunrise.as_ref()), ("Sunset", sunset.as_ref())] {
        if let Some(event) = event {
            primary_facts.push(AirportInfoFactUiView {
                label: label.to_string(),
                value: event.time_label.clone(),
                next_in_label: event.next_in_label.clone(),
                action_id: Some(event.time_display_action_id.clone()),
                link_url: None,
            });
        }
    }
    let mut fact_sections = vec![AirportInfoFactSectionUiView {
        title: None,
        facts: primary_facts,
    }];
    if !communications.is_empty() {
        fact_sections.push(AirportInfoFactSectionUiView {
            title: Some("Communications".to_string()),
            facts: communications
                .iter()
                .map(|communication| AirportInfoFactUiView {
                    label: communication.label.clone(),
                    value: communication.value.clone(),
                    next_in_label: None,
                    action_id: None,
                    link_url: (communication.kind == "phone")
                        .then(|| format!("tel:{}", communication.value)),
                })
                .collect(),
        });
    }

    Ok(AirportInfoUiView {
        airport_id: record.airport_id,
        name: record.name,
        location_label: record.location_label,
        elevation_label,
        traffic_pattern_altitude_label,
        traffic_pattern_altitude_source,
        time_label: crate::format_time_of_day(
            now.timestamp_millis(),
            time_display_mode,
            time_zone,
            airport_time_style(time_display_mode),
        )
        .with_basis(),
        time_display_action_id: crate::TOGGLE_TIME_DISPLAY_MODE_ACTION_ID.to_string(),
        time_zone_label: format!(
            "{} (UTC {})",
            local_now.format("%Z"),
            local_now.format("%z")
        ),
        sunrise,
        sunset,
        communications,
        fact_sections,
        runways_section_title: "Runways".to_string(),
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
    time_display_mode: TimeDisplayMode,
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
        sunrise_at.map(|at| {
            solar_event_ui_view(at, time_zone, now, next_at == Some(at), time_display_mode)
        }),
        sunset_at.map(|at| {
            solar_event_ui_view(at, time_zone, now, next_at == Some(at), time_display_mode)
        }),
    )
}

fn solar_event_ui_view(
    at: DateTime<Utc>,
    time_zone: Tz,
    now: DateTime<Utc>,
    is_next: bool,
    time_display_mode: TimeDisplayMode,
) -> AirportSolarEventUiView {
    AirportSolarEventUiView {
        time_label: crate::format_time_of_day(
            at.timestamp_millis(),
            time_display_mode,
            time_zone,
            airport_time_style(time_display_mode),
        )
        .with_basis(),
        time_display_action_id: crate::TOGGLE_TIME_DISPLAY_MODE_ACTION_ID.to_string(),
        next_in_label: is_next.then(|| format_duration_until(at - now)),
    }
}

fn airport_time_style(time_display_mode: TimeDisplayMode) -> crate::TimeOfDayStyle {
    match time_display_mode {
        TimeDisplayMode::Local => crate::TimeOfDayStyle::Colon,
        TimeDisplayMode::Utc => crate::TimeOfDayStyle::Compact,
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
        endpoints.push(positioned_runway_endpoints(
            runway,
            local_point(&runway.end_a),
            local_point(&runway.end_b),
        ));
    }
    let missing = endpoints
        .iter()
        .enumerate()
        .filter_map(|(index, endpoints)| endpoints.is_none().then_some(index))
        .collect::<Vec<_>>();
    if let [missing_index] = missing.as_slice() {
        let missing_runway = &runways[*missing_index];
        if runway_is_heliport(missing_runway) || !runway_dimensions_are_positive(missing_runway) {
            return None;
        }
        let (half_dx, half_dy) = runway_half_vector(missing_runway)?;
        let missing_length_ft = missing_runway.length_ft?;
        let mut moment_x = 0.0;
        let mut moment_y = 0.0;
        let mut positioned_runway_count = 0;
        for (index, (runway, runway_endpoints)) in runways.iter().zip(endpoints.iter()).enumerate()
        {
            if index == *missing_index || runway_is_heliport(runway) {
                continue;
            }
            let length_ft = runway
                .length_ft
                .filter(|length| length.is_finite() && *length > 0.0)?;
            let (end_a, end_b) = runway_endpoints.as_ref()?;
            moment_x += length_ft * (end_a.0 + end_b.0) / 2.0;
            moment_y += length_ft * (end_a.1 + end_b.1) / 2.0;
            positioned_runway_count += 1;
        }
        if positioned_runway_count == 0 {
            return None;
        }
        let center = (-moment_x / missing_length_ft, -moment_y / missing_length_ft);
        let inferred = (
            (center.0 - half_dx, center.1 - half_dy),
            (center.0 + half_dx, center.1 + half_dy),
        );
        if !runway_endpoints_are_plausible(inferred.0, inferred.1) {
            return None;
        }
        endpoints[*missing_index] = Some(inferred);
    } else if !missing.is_empty() {
        return None;
    }
    let endpoints = endpoints.into_iter().collect::<Option<Vec<_>>>()?;
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

fn positioned_runway_endpoints(
    runway: &AirportRunwayRecord,
    end_a: Option<(f64, f64)>,
    end_b: Option<(f64, f64)>,
) -> Option<((f64, f64), (f64, f64))> {
    let endpoints = match (end_a, end_b) {
        (Some(end_a), Some(end_b)) => (end_a, end_b),
        (Some(center), None) if single_position_heliport(runway) => {
            let half_length_ft = runway.length_ft? / 2.0;
            (
                (center.0, center.1 - half_length_ft),
                (center.0, center.1 + half_length_ft),
            )
        }
        (Some(end_a), None)
            if !runway_is_heliport(runway) && runway_dimensions_are_positive(runway) =>
        {
            let (half_dx, half_dy) = runway_half_vector(runway)?;
            (end_a, (end_a.0 + 2.0 * half_dx, end_a.1 + 2.0 * half_dy))
        }
        (None, Some(end_b))
            if !runway_is_heliport(runway) && runway_dimensions_are_positive(runway) =>
        {
            let (half_dx, half_dy) = runway_half_vector(runway)?;
            ((end_b.0 - 2.0 * half_dx, end_b.1 - 2.0 * half_dy), end_b)
        }
        _ => return None,
    };
    runway_endpoints_are_plausible(endpoints.0, endpoints.1).then_some(endpoints)
}

fn runway_half_vector(runway: &AirportRunwayRecord) -> Option<(f64, f64)> {
    let length_ft = runway
        .length_ft
        .filter(|length| length.is_finite() && *length > 0.0)?;
    let heading = runway
        .end_a
        .heading_true_deg
        .or_else(|| runway.end_b.heading_true_deg.map(|value| value + 180.0))
        .filter(|heading| heading.is_finite())?
        .to_radians();
    Some((
        heading.sin() * length_ft / 2.0,
        -heading.cos() * length_ft / 2.0,
    ))
}

fn runway_endpoints_are_plausible(end_a: (f64, f64), end_b: (f64, f64)) -> bool {
    end_a.0.hypot(end_a.1) <= MAX_RUNWAY_ENDPOINT_DISTANCE_FROM_AIRPORT_FT
        && end_b.0.hypot(end_b.1) <= MAX_RUNWAY_ENDPOINT_DISTANCE_FROM_AIRPORT_FT
        && (end_b.0 - end_a.0).hypot(end_b.1 - end_a.1) >= 1.0
}

fn runway_dimensions_are_positive(runway: &AirportRunwayRecord) -> bool {
    runway
        .length_ft
        .is_some_and(|length| length.is_finite() && length > 0.0)
        && runway
            .width_ft
            .is_some_and(|width| width.is_finite() && width > 0.0)
}

fn runway_is_heliport(runway: &AirportRunwayRecord) -> bool {
    runway
        .end_a
        .ident
        .trim()
        .to_ascii_uppercase()
        .starts_with('H')
        && runway.end_b.ident.trim().is_empty()
}

fn single_position_heliport(runway: &AirportRunwayRecord) -> bool {
    runway_is_heliport(runway)
        && runway.end_a.latitude.is_some()
        && runway.end_a.longitude.is_some()
        && runway.end_b.latitude.is_none()
        && runway.end_b.longitude.is_none()
        && runway_dimensions_are_positive(runway)
}

fn runway_ui_view(
    runway: &AirportRunwayRecord,
    geometry: RunwayDiagramGeometry,
) -> AirportRunwayUiView {
    // Leave room beyond each threshold for the traffic-pattern turn indicator.
    let geometry = RunwayDiagramGeometry {
        end_a_x: geometry.end_a_x * RUNWAY_DIAGRAM_RUNWAY_EXTENT_SCALE,
        end_a_y: geometry.end_a_y * RUNWAY_DIAGRAM_RUNWAY_EXTENT_SCALE,
        end_b_x: geometry.end_b_x * RUNWAY_DIAGRAM_RUNWAY_EXTENT_SCALE,
        end_b_y: geometry.end_b_y * RUNWAY_DIAGRAM_RUNWAY_EXTENT_SCALE,
        width_ratio: geometry.width_ratio * RUNWAY_DIAGRAM_RUNWAY_EXTENT_SCALE,
    };
    let (surface_label, surface_color_key) = runway_surface(&runway.surface);
    let (diagram_end_a_pattern, diagram_end_b_pattern) = if runway_is_heliport(runway) {
        (None, None)
    } else {
        (
            runway_pattern_ui_view(
                (geometry.end_a_x, geometry.end_a_y),
                (geometry.end_b_x, geometry.end_b_y),
                runway.end_a.right_pattern,
            ),
            runway_pattern_ui_view(
                (geometry.end_b_x, geometry.end_b_y),
                (geometry.end_a_x, geometry.end_a_y),
                runway.end_b.right_pattern,
            ),
        )
    };
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
        diagram_end_a_pattern,
        diagram_end_b_pattern,
    }
}

fn runway_pattern_ui_view(
    runway_end: (f64, f64),
    opposite_end: (f64, f64),
    right_pattern: bool,
) -> Option<AirportRunwayPatternUiView> {
    let delta = (opposite_end.0 - runway_end.0, opposite_end.1 - runway_end.1);
    let length = delta.0.hypot(delta.1);
    if !length.is_finite() || length <= f64::EPSILON {
        return None;
    }
    let final_direction = (delta.0 / length, delta.1 / length);
    let final_point = (
        runway_end.0 - final_direction.0 * PATTERN_INDICATOR_THRESHOLD_GAP,
        runway_end.1 - final_direction.1 * PATTERN_INDICATOR_THRESHOLD_GAP,
    );
    let corner = (
        final_point.0 - final_direction.0 * PATTERN_INDICATOR_LEG_LENGTH,
        final_point.1 - final_direction.1 * PATTERN_INDICATOR_LEG_LENGTH,
    );
    let right_side = (-final_direction.1, final_direction.0);
    let side_sign = if right_pattern { 1.0 } else { -1.0 };
    Some(AirportRunwayPatternUiView {
        base_x: corner.0 + right_side.0 * PATTERN_INDICATOR_LEG_LENGTH * side_sign,
        base_y: corner.1 + right_side.1 * PATTERN_INDICATOR_LEG_LENGTH * side_sign,
        corner_x: corner.0,
        corner_y: corner.1,
        final_x: final_point.0,
        final_y: final_point.1,
    })
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
    let color_key = if parts.contains(&"WATER") {
        "airport_runway_water"
    } else if parts
        .iter()
        .any(|part| matches!(*part, "ASPH" | "CONC" | "BIT" | "PEM" | "MATS" | "TREATED"))
    {
        "airport_runway_paved"
    } else if parts.contains(&"TURF") {
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
            location_label: Some("Renton, WA".to_string()),
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
        let view =
            project_airport_info(sample_record(), now, TimeDisplayMode::Local).expect("view");

        assert_eq!(view.time_label, "19:34 PDT");
        assert_eq!(view.location_label.as_deref(), Some("Renton, WA"));
        assert_eq!(
            view.time_display_action_id,
            crate::TOGGLE_TIME_DISPLAY_MODE_ACTION_ID,
        );
        assert_eq!(view.time_zone_label, "PDT (UTC -0700)");
        assert_eq!(
            view.traffic_pattern_altitude_label,
            "1000 ft MSL (968 ft AGL)"
        );
        assert_eq!(view.traffic_pattern_altitude_source, "derived");
        assert_eq!(view.communications[0].value, "124.7");
        assert_eq!(view.runways[0].end_b_label, "RWY 34 (right pattern)");
        let end_a_pattern = view.runways[0]
            .diagram_end_a_pattern
            .as_ref()
            .expect("runway 16 left-pattern indicator");
        let end_b_pattern = view.runways[0]
            .diagram_end_b_pattern
            .as_ref()
            .expect("runway 34 right-pattern indicator");
        let turn_cross = |pattern: &AirportRunwayPatternUiView| {
            let base_direction = (
                pattern.corner_x - pattern.base_x,
                pattern.corner_y - pattern.base_y,
            );
            let final_direction = (
                pattern.final_x - pattern.corner_x,
                pattern.final_y - pattern.corner_y,
            );
            base_direction.0 * final_direction.1 - base_direction.1 * final_direction.0
        };
        assert!(
            turn_cross(end_a_pattern) < 0.0,
            "runway 16 must show left traffic"
        );
        assert!(
            turn_cross(end_b_pattern) > 0.0,
            "runway 34 must show right traffic"
        );
        let threshold_gap = |pattern: &AirportRunwayPatternUiView, end: (f64, f64)| {
            (pattern.final_x - end.0).hypot(pattern.final_y - end.1)
        };
        assert!(
            (threshold_gap(
                end_a_pattern,
                (
                    view.runways[0].diagram_end_a_x,
                    view.runways[0].diagram_end_a_y,
                ),
            ) - PATTERN_INDICATOR_THRESHOLD_GAP)
                .abs()
                < 1e-12
        );
        assert!(
            (threshold_gap(
                end_b_pattern,
                (
                    view.runways[0].diagram_end_b_x,
                    view.runways[0].diagram_end_b_y,
                ),
            ) - PATTERN_INDICATOR_THRESHOLD_GAP)
                .abs()
                < 1e-12
        );
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
    fn runway_diagram_reserves_room_for_pattern_indicators() {
        let record = sample_record();
        let runway = runway_ui_view(
            &record.runways[0],
            RunwayDiagramGeometry {
                end_a_x: 0.0,
                end_a_y: -0.5,
                end_b_x: 0.0,
                end_b_y: 0.5,
                width_ratio: 0.02,
            },
        );

        assert!((runway.diagram_end_a_y + 0.475).abs() < 1e-12);
        assert!((runway.diagram_end_b_y - 0.475).abs() < 1e-12);
        let maximum_pattern_extent = RUNWAY_DIAGRAM_RUNWAY_EXTENT_SCALE / 2.0
            + PATTERN_INDICATOR_THRESHOLD_GAP
            + PATTERN_INDICATOR_LEG_LENGTH;
        for pattern in [
            runway.diagram_end_a_pattern.expect("end A pattern"),
            runway.diagram_end_b_pattern.expect("end B pattern"),
        ] {
            for coordinate in [
                pattern.base_x,
                pattern.base_y,
                pattern.corner_x,
                pattern.corner_y,
                pattern.final_x,
                pattern.final_y,
            ] {
                assert!(coordinate.abs() <= maximum_pattern_extent + 1e-12);
            }
        }
        assert!(maximum_pattern_extent < 0.58);
    }

    #[test]
    fn runway_complex_completes_one_missing_runway_but_not_two() {
        let now = Utc.with_ymd_and_hms(2026, 7, 26, 2, 34, 0).unwrap();
        let mut record = sample_record();
        record.latitude = 47.4509;
        record.longitude = -122.31455;
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
        record.runways.push(AirportRunwayRecord {
            length_ft: Some(50.0),
            width_ft: Some(50.0),
            surface: "CONC-G".to_string(),
            end_a: AirportRunwayEndRecord {
                ident: "H1".to_string(),
                heading_true_deg: None,
                latitude: Some(47.4500),
                longitude: Some(-122.3200),
                right_pattern: false,
            },
            end_b: AirportRunwayEndRecord {
                ident: String::new(),
                heading_true_deg: None,
                latitude: None,
                longitude: None,
                right_pattern: false,
            },
        });

        let complex = project_airport_info(record.clone(), now, TimeDisplayMode::Local)
            .expect("complex view");
        assert!(complex.runway_diagram_complex);
        assert_eq!(complex.runways.len(), 3);
        assert!(
            (complex.runways[2].diagram_end_b_y - complex.runways[2].diagram_end_a_y).abs() > 0.0
        );
        assert!(complex.runways[2].diagram_end_a_pattern.is_none());
        assert!(complex.runways[2].diagram_end_b_pattern.is_none());
        let east_center_x =
            (complex.runways[0].diagram_end_a_x + complex.runways[0].diagram_end_b_x) / 2.0;
        let west_center_x =
            (complex.runways[1].diagram_end_a_x + complex.runways[1].diagram_end_b_x) / 2.0;
        assert!(east_center_x > west_center_x);

        record.runways[1].end_b.longitude = None;
        let one_end_completed = project_airport_info(record.clone(), now, TimeDisplayMode::Local)
            .expect("one-end-completed view");
        assert!(one_end_completed.runway_diagram_complex);

        record.runways[1].end_a.latitude = None;
        record.runways[1].end_a.longitude = None;
        record.runways[1].end_b.latitude = None;
        record.runways[1].end_b.longitude = None;
        let arp_inferred = project_airport_info(record.clone(), now, TimeDisplayMode::Local)
            .expect("ARP-inferred view");
        assert!(arp_inferred.runway_diagram_complex);
        let inferred_east_center_x = (arp_inferred.runways[0].diagram_end_a_x
            + arp_inferred.runways[0].diagram_end_b_x)
            / 2.0;
        let inferred_west_center_x = (arp_inferred.runways[1].diagram_end_a_x
            + arp_inferred.runways[1].diagram_end_b_x)
            / 2.0;
        assert!(inferred_east_center_x > inferred_west_center_x);

        record.runways[0].end_a.latitude = None;
        record.runways[0].end_a.longitude = None;
        record.runways[0].end_b.latitude = None;
        record.runways[0].end_b.longitude = None;
        let fallback =
            project_airport_info(record, now, TimeDisplayMode::Local).expect("fallback view");
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
        let view =
            project_airport_info(sample_record(), now, TimeDisplayMode::Local).expect("view");

        assert_eq!(view.time_label, "18:34 PST");
        assert_eq!(view.time_zone_label, "PST (UTC -0800)");
    }

    #[test]
    fn airport_time_display_mode_projects_all_clocks_in_zulu() {
        let now = Utc.with_ymd_and_hms(2026, 7, 26, 2, 34, 0).unwrap();
        let view = project_airport_info(sample_record(), now, TimeDisplayMode::Utc).expect("view");

        assert_eq!(view.time_label, "0234Z");
        for event in view.sunrise.iter().chain(view.sunset.iter()) {
            assert!(event.time_label.ends_with('Z'));
            assert_eq!(
                event.time_display_action_id,
                crate::TOGGLE_TIME_DISPLAY_MODE_ACTION_ID,
            );
        }
    }
}
