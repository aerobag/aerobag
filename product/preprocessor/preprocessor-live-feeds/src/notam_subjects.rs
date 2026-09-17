// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::StructuredNotamRecord;
use product_contracts::NotamSubjectKey;
use std::collections::BTreeSet;

pub(super) fn subjects(record: &StructuredNotamRecord) -> BTreeSet<NotamSubjectKey> {
    let mut subjects = BTreeSet::new();
    let texts = [&record.text, &record.local_text, &record.icao_text];
    let nav_notice = record.notam_keyword.as_deref() == Some("NAV")
        || texts
            .iter()
            .filter_map(|text| text.as_deref())
            .any(|text| text.trim_start().starts_with("NAV "));
    if nav_notice {
        if let Some((id, kind)) = record
            .airport_name
            .as_deref()
            .and_then(|name| name.split_once('-'))
        {
            let subject = NotamSubjectKey::Navaid(id.to_ascii_uppercase());
            let location_matches = [&record.location_designator, &record.location]
                .into_iter()
                .flatten()
                .any(|location| location.eq_ignore_ascii_case(id));
            if location_matches
                && subject.validate().is_ok()
                && matches!(
                    kind.trim(),
                    "VOR" | "VOR/DME" | "VORTAC" | "TACAN" | "DME" | "NDB" | "NDB/DME"
                )
            {
                subjects.insert(subject);
            }
        }
    }
    for text in texts.into_iter().flatten() {
        subjects.extend(airway_subjects(text));
    }
    subjects
}

fn is_center(token: &str) -> bool {
    token.len() == 3 && token.starts_with('Z') && token.bytes().all(|b| b.is_ascii_uppercase())
}

fn airway_subjects(text: &str) -> BTreeSet<NotamSubjectKey> {
    // A route notice starts with a ROUTE heading or explicit ARTCC heading,
    // followed by its affected airway list. Do not scan arbitrary prose for IDs.
    let text = text.to_ascii_uppercase();
    let tokens = text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let Some(mut start) = tokens
        .iter()
        .take(4)
        .position(|token| *token == "ROUTE")
        .map(|i| i + 1)
        .or_else(|| tokens.first().filter(|token| is_center(token)).map(|_| 0))
    else {
        return BTreeSet::new();
    };
    let first_center = start;
    while tokens.get(start).is_some_and(|token| is_center(token)) {
        start += 1;
    }
    if start == first_center {
        return BTreeSet::new();
    }
    tokens[start..]
        .iter()
        .map(|id| NotamSubjectKey::Airway((*id).to_string()))
        .take_while(|subject| subject.validate().is_ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_subject_is_not_the_filing_center_or_an_incidental_radial() {
        for text in [
            "WA..ROUTE ZSE. V23 MALAY, WA TO MCKEN, WA MEA 5400 NORTHBOUND.",
            "ZSE. V23 MALAY TO MCKEN MEA 5400",
        ] {
            assert_eq!(
                airway_subjects(text),
                BTreeSet::from([NotamSubjectKey::Airway("V23".into())])
            );
        }
        assert_eq!(
            airway_subjects("ZNY ZBW. V139, V268 HAMPTON (HTO) VORTAC R-236 TO MANTA NA"),
            BTreeSet::from([
                NotamSubjectKey::Airway("V139".into()),
                NotamSubjectKey::Airway("V268".into())
            ])
        );
        for text in [
            "AIRSPACE UAS NEAR V23",
            "NAV VOR R-003 U/S",
            "ZSE SVC WX RADAR U/S",
            "ROUTE DESCRIPTION V23",
        ] {
            assert!(airway_subjects(text).is_empty(), "{text}");
        }
    }
}
