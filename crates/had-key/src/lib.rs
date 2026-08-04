// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub fn component(value: &str) -> String {
    percent_encode_component(value.trim().as_bytes())
}

pub fn upper_component(value: &str) -> String {
    component(&value.to_ascii_uppercase())
}

pub fn decode_component(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).copied().ok_or("truncated escape")?;
            let low = bytes.get(index + 2).copied().ok_or("truncated escape")?;
            decoded.push((hex_value(high)? << 4) | hex_value(low)?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|error| error.to_string())
}

pub fn search_terms(value: &str) -> Vec<String> {
    let mut normalized = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_uppercase());
        } else if matches!(ch, '\'' | '\u{2019}') {
            // Users type O'HARE and OHARE interchangeably.
        } else {
            normalized.push(' ');
        }
    }
    normalized
        .split_whitespace()
        .filter(|term| term.len() >= 2)
        .map(str::to_string)
        .collect()
}

fn percent_encode_component(bytes: &[u8]) -> String {
    let mut out = String::new();
    for byte in bytes {
        let ch = *byte as char;
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~') {
            out.push(ch);
        } else {
            out.push('%');
            out.push(hex_digit(byte >> 4));
            out.push(hex_digit(byte & 0x0f));
        }
    }
    out
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'A' + value - 10) as char,
        _ => unreachable!("hex nibble out of range"),
    }
}

fn hex_value(value: u8) -> Result<u8, String> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(format!("invalid hex digit {}", value as char)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_escapes_had_key_delimiters_and_url_subdelims() {
        assert_eq!(
            component("plate:KORS:IAP-WA-RNAV (GPS)-A.png"),
            "plate%3AKORS%3AIAP-WA-RNAV%20%28GPS%29-A.png"
        );
        assert_eq!(component("a/b?c#d"), "a%2Fb%3Fc%23d");
        assert_eq!(component("keep-_.~"), "keep-_.~");
    }

    #[test]
    fn component_trims_and_percent_encodes_utf8_bytes() {
        assert_eq!(component(" café "), "caf%C3%A9");
    }

    #[test]
    fn upper_component_trims_and_uppercases_before_escaping() {
        assert_eq!(upper_component(" kgrk/vor-a "), "KGRK%2FVOR-A");
    }

    #[test]
    fn decode_component_reverses_percent_escaping() {
        assert_eq!(decode_component("KGRK%2FVOR-A").unwrap(), "KGRK/VOR-A");
        assert_eq!(decode_component("caf%C3%A9").unwrap(), "café");
        assert!(decode_component("bad%2").is_err());
    }

    #[test]
    fn search_terms_normalize_airport_names_and_cities() {
        assert_eq!(
            search_terms("Chicago O'Hare Intl"),
            vec!["CHICAGO", "OHARE", "INTL"]
        );
        assert_eq!(
            search_terms("Seattle-Tacoma / Paine Fld"),
            vec!["SEATTLE", "TACOMA", "PAINE", "FLD"]
        );
        assert_eq!(search_terms("St. Mary's"), vec!["ST", "MARYS"]);
    }
}
