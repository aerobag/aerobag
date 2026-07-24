// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub fn component(value: &str) -> String {
    percent_encode_component(value.trim().as_bytes())
}

pub fn upper_component(value: &str) -> String {
    component(&value.to_ascii_uppercase())
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
}
