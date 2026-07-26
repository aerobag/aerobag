// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

#[derive(Debug, Clone, Copy)]
pub struct RunwayHeadingInput<'a> {
    pub published_heading_deg: Option<f64>,
    pub start: Option<(f64, f64)>,
    pub end: Option<(f64, f64)>,
    pub runway_ident: &'a str,
    pub magnetic_variation_deg: Option<f64>,
}

pub fn resolve_true_heading(input: RunwayHeadingInput<'_>) -> Option<f64> {
    if let Some(heading) = input
        .published_heading_deg
        .filter(|value| value.is_finite())
    {
        return Some(normalize_heading(heading));
    }
    if let (Some((start_lat, start_lon)), Some((end_lat, end_lon))) = (input.start, input.end) {
        let distinct = (start_lat - end_lat).abs() > f64::EPSILON
            || (start_lon - end_lon).abs() > f64::EPSILON;
        if distinct {
            return Some(bearing_true_deg(start_lat, start_lon, end_lat, end_lon));
        }
    }
    let magnetic_heading = magnetic_heading_from_ident(input.runway_ident)?;
    Some(normalize_heading(
        magnetic_heading + input.magnetic_variation_deg.unwrap_or_default(),
    ))
}

pub fn parse_optional_number(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

pub fn parse_optional_position(latitude: &str, longitude: &str) -> Option<(f64, f64)> {
    let latitude = parse_optional_number(latitude)?;
    let longitude = parse_optional_number(longitude)?;
    (valid_lat_lon(latitude, longitude)
        && (latitude.abs() > f64::EPSILON || longitude.abs() > f64::EPSILON))
        .then_some((latitude, longitude))
}

pub fn parse_airport_magnetic_variation(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (magnitude_text, suffix) = trimmed.split_at(trimmed.len().saturating_sub(1));
    match suffix {
        "E" => parse_optional_number(magnitude_text),
        "W" => parse_optional_number(magnitude_text).map(|degrees| -degrees),
        _ => parse_optional_number(trimmed),
    }
}

pub fn magnetic_heading_from_ident(runway_ident: &str) -> Option<f64> {
    let normalized = runway_ident.trim().to_ascii_uppercase();
    let compass_heading = match normalized.as_str() {
        "N" => Some(0.0),
        "NNE" => Some(22.5),
        "NE" => Some(45.0),
        "ENE" => Some(67.5),
        "E" => Some(90.0),
        "ESE" => Some(112.5),
        "SE" => Some(135.0),
        "SSE" => Some(157.5),
        "S" => Some(180.0),
        "SSW" => Some(202.5),
        "SW" => Some(225.0),
        "WSW" => Some(247.5),
        "W" => Some(270.0),
        "WNW" => Some(292.5),
        "NW" => Some(315.0),
        "NNW" => Some(337.5),
        _ => None,
    };
    if compass_heading.is_some() {
        return compass_heading;
    }
    let number_text = normalized
        .strip_suffix(['L', 'C', 'R', 'W', 'U'])
        .unwrap_or(&normalized);
    let number = number_text.parse::<u16>().ok()?;
    match number {
        1..=36 => Some(f64::from(number) * 10.0),
        37..=360 if number_text.len() == 3 => Some(f64::from(number)),
        _ => None,
    }
}

fn valid_lat_lon(latitude: f64, longitude: f64) -> bool {
    latitude.is_finite()
        && longitude.is_finite()
        && (-90.0..=90.0).contains(&latitude)
        && (-180.0..=180.0).contains(&longitude)
}

fn bearing_true_deg(start_lat: f64, start_lon: f64, end_lat: f64, end_lon: f64) -> f64 {
    let start_lat_rad = start_lat.to_radians();
    let end_lat_rad = end_lat.to_radians();
    let delta_lon_rad = (end_lon - start_lon).to_radians();
    let y = delta_lon_rad.sin() * end_lat_rad.cos();
    let x = start_lat_rad.cos() * end_lat_rad.sin()
        - start_lat_rad.sin() * end_lat_rad.cos() * delta_lon_rad.cos();
    normalize_heading(y.atan2(x).to_degrees())
}

fn normalize_heading(heading: f64) -> f64 {
    let normalized = heading.rem_euclid(360.0);
    if normalized == 0.0 {
        360.0
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_coordinate_cannot_become_null_island() {
        assert_eq!(parse_optional_position("", "-85.8931722"), None);
        assert_eq!(parse_optional_position("35.2071111", ""), None);
        assert_eq!(parse_optional_position("", ""), None);
        assert_eq!(parse_optional_position("0", "0"), None);
    }

    #[test]
    fn kuos_one_end_row_uses_identifier_and_variation() {
        let heading = resolve_true_heading(RunwayHeadingInput {
            published_heading_deg: None,
            start: None,
            end: Some((35.2071111, -85.8931722)),
            runway_ident: "07",
            magnetic_variation_deg: parse_airport_magnetic_variation("01W"),
        });

        assert_eq!(heading, Some(69.0));
    }

    #[test]
    fn sparse_runway_identifiers_resolve_magnetic_headings() {
        assert_eq!(magnetic_heading_from_ident("14W"), Some(140.0));
        assert_eq!(magnetic_heading_from_ident("04U"), Some(40.0));
        assert_eq!(magnetic_heading_from_ident("072"), Some(72.0));
        assert_eq!(magnetic_heading_from_ident("ESE"), Some(112.5));
        assert_eq!(magnetic_heading_from_ident("ALL"), None);
        assert_eq!(magnetic_heading_from_ident("00X"), None);
    }
}
