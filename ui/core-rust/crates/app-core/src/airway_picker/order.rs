// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AirwayAltitudeBand {
    Low,
    High,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct AirwayMenuOrder {
    preferred_band: AirwayAltitudeBand,
}

impl Default for AirwayMenuOrder {
    fn default() -> Self {
        Self::new(AirwayAltitudeBand::Low)
    }
}

impl AirwayMenuOrder {
    pub(super) fn new(preferred_band: AirwayAltitudeBand) -> Self {
        Self { preferred_band }
    }

    fn band_rank(&self, name: &str) -> u8 {
        let band = match name.as_bytes().first() {
            Some(b'T' | b'V') => AirwayAltitudeBand::Low,
            Some(b'J' | b'Q') => AirwayAltitudeBand::High,
            // Other published route families stay available after these two groups.
            _ => return 2,
        };
        u8::from(band != self.preferred_band)
    }

    fn compare(&self, a: &str, b: &str) -> Ordering {
        self.band_rank(a)
            .cmp(&self.band_rank(b))
            .then_with(|| route_number_key(a).cmp(&route_number_key(b)))
            .then_with(|| a.cmp(b))
    }

    pub(super) fn sort(&self, names: &mut [String]) {
        names.sort_by(|a, b| self.compare(a, b));
    }
}

fn route_number_key(name: &str) -> (&str, usize, &str, &str) {
    let prefix_len = name.bytes().take_while(|c| !c.is_ascii_digit()).count();
    let (prefix, remainder) = name.split_at(prefix_len);
    let number_len = remainder.bytes().take_while(u8::is_ascii_digit).count();
    let (number, suffix) = remainder.split_at(number_len);
    // Comparing significant digit count then digits gives numeric order without overflow.
    let number = number.trim_start_matches('0');
    (prefix, number.len(), number, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn either_altitude_band_can_lead_without_changing_order_within_a_band() {
        let names = ["V25", "Q20", "T10", "J12", "V2", "T2", "J2", "Q3", "Y1"];
        for (preferred, expected) in [
            (
                AirwayAltitudeBand::Low,
                ["T2", "T10", "V2", "V25", "J2", "J12", "Q3", "Q20", "Y1"],
            ),
            (
                AirwayAltitudeBand::High,
                ["J2", "J12", "Q3", "Q20", "T2", "T10", "V2", "V25", "Y1"],
            ),
        ] {
            let mut actual = names.map(String::from);
            AirwayMenuOrder::new(preferred).sort(&mut actual);
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn numeric_order_is_total_including_suffixes_leading_zeroes_and_large_numbers() {
        let order = AirwayMenuOrder::default();
        let mut names =
            ["V10", "V2B", "V9999999999999999999999", "V2", "V02", "V2A"].map(String::from);
        order.sort(&mut names);
        assert_eq!(
            names,
            ["V02", "V2", "V2A", "V2B", "V10", "V9999999999999999999999"]
        );
    }
}
