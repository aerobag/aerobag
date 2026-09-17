// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

/// Subject identity is distinct from the facility which filed the notice.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotamSubjectKey {
    Navaid(String),
    Airway(String),
}

impl NotamSubjectKey {
    pub fn ident(&self) -> &str {
        match self {
            Self::Navaid(id) | Self::Airway(id) => id,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Navaid(_) => "navaid",
            Self::Airway(_) => "airway",
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let id = self.ident();
        let valid = match self {
            Self::Navaid(_) => {
                (1..=5).contains(&id.len())
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            }
            Self::Airway(_) => {
                (2..=5).contains(&id.len())
                    && matches!(
                        id.as_bytes()[0],
                        b'V' | b'J' | b'T' | b'Q' | b'A' | b'B' | b'G' | b'R'
                    )
                    && id.as_bytes()[1..].iter().all(u8::is_ascii_digit)
                    && id.as_bytes()[1] != b'0'
            }
        };
        if valid {
            Ok(())
        } else {
            Err(format!("invalid NOTAM {} subject {id:?}", self.kind()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_kinds_are_distinct_and_identifiers_are_canonical() {
        assert_ne!(
            NotamSubjectKey::Navaid("V23".into()),
            NotamSubjectKey::Airway("V23".into())
        );
        for id in ["V23", "J70", "T268", "Q1", "G13"] {
            let subject = NotamSubjectKey::Airway(id.into());
            subject.validate().unwrap();
            assert_eq!(
                serde_json::from_value::<NotamSubjectKey>(serde_json::to_value(&subject).unwrap())
                    .unwrap(),
                subject
            );
        }
        for id in ["", "v23", "V023", "ZSE", "R-003", "V23 TO V4", "V4 "] {
            assert!(
                NotamSubjectKey::Airway(id.into()).validate().is_err(),
                "{id}"
            );
        }
        for id in ["", "hpb", "HPB-VOR", "HPB/SEA", " HPB"] {
            assert!(
                NotamSubjectKey::Navaid(id.into()).validate().is_err(),
                "{id}"
            );
        }
    }
}
