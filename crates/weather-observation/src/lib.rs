// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Observed METAR cloud cover and flight category, shared by ingest and clients.
//! This is not a full METAR decoder. Uninterpretable visibility/ceiling remains
//! unknown; the original report is always available to the reader.
//! Categories: https://aviationweather.gov/gfa/help/#fltcats
//! Visibility: https://www.weather.gov/asos/Visibility.html

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FlightCategory {
    Vfr,
    Mvfr,
    Ifr,
    Lifr,
}

impl FlightCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vfr => "VFR",
            Self::Mvfr => "MVFR",
            Self::Ifr => "IFR",
            Self::Lifr => "LIFR",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observation {
    pub cloud_symbol: Option<&'static str>,
    pub flight_category: Option<FlightCategory>,
}

pub fn metar_cloud_symbol(text: &str) -> Option<&'static str> {
    metar(text).cloud_symbol
}

pub fn metar(text: &str) -> Observation {
    let words: Vec<_> = text
        .split_whitespace()
        .map(|word| word.trim_end_matches('='))
        .take_while(|word| {
            ![
                "RMK", "BECMG", "TEMPO", "INTER", "NOSIG", "PROB30", "PROB40",
            ]
            .contains(word)
        })
        .collect();
    let mut layers = Vec::new();
    let mut clear = None;
    let mut vertical = false;
    let mut invalid_cloud = false;
    let mut visibility = None;
    let mut visibility_count = 0;
    let mut past_wind = false;
    let mut cavok = false;
    for (index, &word) in words.iter().enumerate() {
        if word == "NIL" {
            return Observation {
                cloud_symbol: None,
                flight_category: None,
            };
        }
        if word == "CAVOK" {
            cavok = true;
        }
        if ["CAVOK", "SKC", "CLR", "NCD", "NSC"].contains(&word) {
            clear = Some(if word == "NSC" { "NSC" } else { "SKC" });
        }
        for amount in ["FEW", "SCT", "BKN", "OVC", "VV"] {
            if let Some(rest) = word.strip_prefix(amount) {
                match cloud_height(rest) {
                    Some(height) => {
                        vertical |= amount == "VV";
                        layers.push((amount, height));
                    }
                    None => invalid_cloud = true,
                }
            }
        }
        // Four-digit metric visibility is positional: never interpret station
        // identifiers, runway visual range, temperature, or remarks as visibility.
        if past_wind && word.len() == 4 && word.bytes().all(|c| c.is_ascii_digit()) {
            visibility_count += 1;
            visibility = word
                .parse::<f64>()
                .ok()
                .map(|metres| visibility_category(metres / 1609.344));
        }
        if let Some(sm) = word.strip_suffix("SM") {
            visibility_count += 1;
            let whole = index
                .checked_sub(1)
                .and_then(|i| words[i].parse::<u32>().ok());
            visibility = statute_visibility(sm, whole);
        }
        if ["KT", "MPS", "KMH"]
            .iter()
            .any(|suffix| word.ends_with(suffix))
        {
            past_wind = true;
        }
        // A cloud or temperature group terminates the metric-visibility slot.
        if !layers.is_empty() || clear.is_some() || word.contains('/') {
            past_wind = false;
        }
    }
    if visibility_count != 1 {
        visibility = None;
    }
    if cavok && visibility_count == 0 {
        visibility = Some(FlightCategory::Vfr);
    }
    let ceiling_layer = layers
        .iter()
        .filter(|(amount, _)| ["BKN", "OVC", "VV"].contains(amount))
        .min_by_key(|(_, height)| height.unwrap_or(u32::MAX));
    let cloud_symbol = if vertical {
        Some("VV")
    } else {
        ceiling_layer
            .or_else(|| {
                layers
                    .iter()
                    .min_by_key(|(_, height)| height.unwrap_or(u32::MAX))
            })
            .map(|(amount, _)| *amount)
            .or(clear)
    };
    let unknown_ceiling = layers
        .iter()
        .any(|(amount, height)| ["BKN", "OVC", "VV"].contains(amount) && height.is_none());
    let ceiling = if invalid_cloud || unknown_ceiling || (clear.is_some() && !layers.is_empty()) {
        None
    } else if let Some((_, Some(feet))) = ceiling_layer {
        Some(match feet {
            0..=499 => FlightCategory::Lifr,
            500..=999 => FlightCategory::Ifr,
            1000..=3000 => FlightCategory::Mvfr,
            _ => FlightCategory::Vfr,
        })
    } else if clear.is_some() || !layers.is_empty() {
        Some(FlightCategory::Vfr)
    } else {
        None
    };
    Observation {
        cloud_symbol,
        // LIFR is conclusive even if the other component is unknown. Otherwise
        // do not invent a complete classification from incomplete observations.
        flight_category: if ceiling == Some(FlightCategory::Lifr)
            || visibility == Some(FlightCategory::Lifr)
        {
            Some(FlightCategory::Lifr)
        } else {
            ceiling.zip(visibility).map(|(c, v)| c.max(v))
        },
    }
}

fn cloud_height(rest: &str) -> Option<Option<u32>> {
    let digit_len = rest.bytes().take_while(u8::is_ascii_digit).count();
    let (height, suffix) = if let Some(suffix) = rest.strip_prefix("///") {
        (None, suffix)
    } else if (3..=4).contains(&digit_len) {
        (
            Some(rest[..digit_len].parse::<u32>().ok()? * 100),
            &rest[digit_len..],
        )
    } else {
        return None;
    };
    ["", "CB", "TCU", "///"].contains(&suffix).then_some(height)
}

fn visibility_category(sm: f64) -> FlightCategory {
    if sm < 1.0 {
        FlightCategory::Lifr
    } else if sm < 3.0 {
        FlightCategory::Ifr
    } else if sm <= 5.0 {
        FlightCategory::Mvfr
    } else {
        FlightCategory::Vfr
    }
}

fn statute_visibility(text: &str, whole: Option<u32>) -> Option<FlightCategory> {
    let bound = text.as_bytes().first().copied()?;
    let number = if bound == b'M' || bound == b'P' {
        &text[1..]
    } else {
        text
    };
    let value = if let Some((n, d)) = number.split_once('/') {
        let n = n.parse::<u32>().ok()?;
        let d = d.parse::<u32>().ok()?;
        if n >= d || d == 0 {
            return None;
        }
        f64::from(n) / f64::from(d) + f64::from(whole.unwrap_or(0))
    } else {
        f64::from(number.parse::<u32>().ok()?)
    };
    if value > 100.0 {
        return None;
    }
    match bound {
        b'M' => (value > 0.0 && value <= 1.0).then_some(FlightCategory::Lifr),
        b'P' => (value >= 5.0).then_some(FlightCategory::Vfr),
        _ => Some(visibility_category(value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_thresholds_use_worst_of_observed_ceiling_and_prevailing_visibility() {
        for (visibility, vcat) in [
            ("M1/4SM", "LIFR"),
            ("3/4SM", "LIFR"),
            ("1SM", "IFR"),
            ("2 1/2SM", "IFR"),
            ("3SM", "MVFR"),
            ("5SM", "MVFR"),
            ("6SM", "VFR"),
            ("P6SM", "VFR"),
        ] {
            for (cloud, ccat) in [
                ("BKN004", "LIFR"),
                ("BKN005", "IFR"),
                ("OVC009", "IFR"),
                ("BKN010", "MVFR"),
                ("BKN030", "MVFR"),
                ("OVC031", "VFR"),
                ("CLR", "VFR"),
            ] {
                let rank = |value| {
                    ["VFR", "MVFR", "IFR", "LIFR"]
                        .iter()
                        .position(|c| *c == value)
                        .unwrap()
                };
                let expected = if rank(vcat) > rank(ccat) { vcat } else { ccat };
                let text =
                    format!("METAR KPAE 101253Z AUTO 18005KT {visibility} {cloud} 10/05 A3000");
                assert_eq!(
                    metar(&text).flight_category.map(FlightCategory::as_str),
                    Some(expected),
                    "{text}"
                );
            }
        }
    }

    #[test]
    fn metric_visibility_cavok_and_vertical_visibility() {
        for (body, expected, symbol) in [
            ("9999 SCT020", Some("VFR"), "SCT"),
            ("1500 OVC020", Some("LIFR"), "OVC"),
            ("4000 BKN020", Some("IFR"), "BKN"),
            ("8000 BKN040", Some("MVFR"), "BKN"),
            ("CAVOK", Some("VFR"), "SKC"),
            ("10SM VV002", Some("LIFR"), "VV"),
            ("10SM VV///", None, "VV"),
        ] {
            let result = metar(&format!("EGLL 101250Z 00000KT {body} 10/05 Q1010"));
            assert_eq!(
                result.flight_category.map(FlightCategory::as_str),
                expected,
                "{body}"
            );
            assert_eq!(result.cloud_symbol, Some(symbol));
        }
    }

    #[test]
    fn remarks_trends_rvr_and_partial_data_cannot_invent_observed_weather() {
        for body in [
            "R16L/0600FT CLR",
            "10SM BKN///",
            "10SM BKN020 OVC///",
            "10SM BKNbad",
            "10SM",
            "CLR",
            "P1SM CLR",
            "M3SM CLR",
            "10SM CLR BKN005",
            "10SM 1SM CLR",
            "NIL",
        ] {
            assert_eq!(
                metar(&format!("KPAE 101253Z 00000KT {body}")).flight_category,
                None,
                "{body}"
            );
        }
        for tail in [
            "RMK VIS 1/4SM BKN001",
            "TEMPO 1/4SM BKN001",
            "BECMG 1/4SM BKN001",
            "NOSIG",
        ] {
            let result = metar(&format!(
                "SPECI KPAE 101253Z 00000KT 10SM FEW010 BKN080 {tail}"
            ));
            assert_eq!(result.flight_category, Some(FlightCategory::Vfr));
            assert_eq!(result.cloud_symbol, Some("BKN"));
        }
    }
}
