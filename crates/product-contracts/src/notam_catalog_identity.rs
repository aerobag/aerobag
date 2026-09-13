// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::NotamAirportCatalog;

/// Canonical representation v1: domain bytes (including NUL), catalog schema
/// as big-endian u32, unique airport count as big-endian u64, then each sorted
/// UTF-8 ID prefixed by its byte length as big-endian u64. No JSON or NAVDB
/// package bytes participate. Changing this algorithm requires descriptor v2.
const IDENTITY_DOMAIN: &[u8] = b"aerobag/notam-airport-catalog/v1\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotamCatalogIdentity {
    /// Schema of the logical catalog, not of a publication or NAVDB package.
    pub schema_version: u32,
    pub sha256: String,
    pub airport_count: u64,
}

impl NotamCatalogIdentity {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != NotamAirportCatalog::SCHEMA_VERSION {
            return Err("unsupported NOTAM catalog identity schema".into());
        }
        if self.airport_count == 0 {
            return Err("NOTAM catalog identity has no airports".into());
        }
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("NOTAM catalog identity requires lowercase SHA-256 hex".into());
        }
        Ok(())
    }
}

impl NotamAirportCatalog {
    /// Fingerprint the exact in-memory catalog supplied to NOTAM projection.
    /// Invalid IDs are rejected, never trimmed, case-folded, or filtered.
    pub fn identity(&self) -> Result<NotamCatalogIdentity, String> {
        self.validate()?;
        for id in &self.airport_ids {
            if !id
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            {
                return Err(format!("NOTAM airport catalog contains invalid ID {id:?}"));
            }
        }
        Ok(canonical_identity(self))
    }

    /// Union every supported cycle's validated catalog before fingerprinting.
    pub fn union<'a>(catalogs: impl IntoIterator<Item = &'a Self>) -> Result<Self, String> {
        let mut union = Self {
            schema_version: Self::SCHEMA_VERSION,
            airport_ids: Default::default(),
        };
        for catalog in catalogs {
            catalog.identity()?;
            union
                .airport_ids
                .extend(catalog.airport_ids.iter().cloned());
        }
        union.identity()?;
        Ok(union)
    }
}

fn canonical_identity(catalog: &NotamAirportCatalog) -> NotamCatalogIdentity {
    let airport_count = catalog.airport_ids.len() as u64;
    let mut hash = Sha256::new();
    hash.update(IDENTITY_DOMAIN);
    hash.update(catalog.schema_version.to_be_bytes());
    hash.update(airport_count.to_be_bytes());
    for id in &catalog.airport_ids {
        hash.update((id.len() as u64).to_be_bytes());
        hash.update(id.as_bytes());
    }
    NotamCatalogIdentity {
        schema_version: catalog.schema_version,
        airport_count,
        sha256: format!("{:x}", hash.finalize()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog(ids: &[&str]) -> NotamAirportCatalog {
        NotamAirportCatalog {
            schema_version: NotamAirportCatalog::SCHEMA_VERSION,
            airport_ids: ids.iter().map(|id| id.to_string()).collect(),
        }
    }

    #[test]
    fn ordering_and_cross_cycle_duplicates_do_not_affect_identity() {
        let first = catalog(&["KSFO", "1S5"]);
        let second = catalog(&["KJFK", "KSFO"]);
        let expected = catalog(&["KJFK", "KSFO", "1S5"]);
        for cycles in [[&first, &second], [&second, &first]] {
            assert_eq!(NotamAirportCatalog::union(cycles).unwrap(), expected);
        }
        assert_eq!(expected.identity().unwrap().airport_count, 3);
        assert_eq!(
            expected.identity().unwrap().sha256,
            "5dce8d7e0cf97d5de13dacc38f0d7c0ff3d66b54c01a881ad41d3dc31b934b08"
        );
        assert_eq!(
            expected.identity(),
            catalog(&["1S5", "KSFO", "KJFK", "KSFO"]).identity()
        );
    }

    #[test]
    fn additions_removals_and_equal_count_substitutions_change_hash() {
        let first = catalog(&["KSFO", "KJFK"]).identity().unwrap();
        for ids in [
            vec!["KSFO"],
            vec!["KSFO", "KJFK", "1S5"],
            vec!["KSFO", "KLAX"],
        ] {
            assert_ne!(first.sha256, catalog(&ids).identity().unwrap().sha256);
        }
    }

    #[test]
    fn schema_participates_in_hash_and_unknown_schema_is_rejected() {
        let first = catalog(&["KSFO"]);
        let mut changed = first.clone();
        changed.schema_version += 1;
        assert_ne!(
            canonical_identity(&first).sha256,
            canonical_identity(&changed).sha256
        );
        assert!(changed.identity().is_err());
        assert!(NotamAirportCatalog::union([&first, &changed]).is_err());
    }

    #[test]
    fn invalid_catalogs_are_not_normalized_into_compatibility() {
        for id in [
            "",
            " ",
            "ksfo",
            " KSFO",
            "KSFO\n",
            "K SFO",
            "K/SFO",
            "K-SFO",
            "K\u{00c9}",
            "K\0SFO",
        ] {
            assert!(catalog(&[id]).identity().is_err(), "{id:?}");
        }
        assert!(catalog(&[]).identity().is_err());
        assert!(NotamAirportCatalog::union([]).is_err());
    }

    #[test]
    fn identity_is_not_ambiguous_id_concatenation() {
        assert_ne!(
            catalog(&["AB", "C"]).identity(),
            catalog(&["A", "BC"]).identity()
        );
    }

    #[test]
    fn different_navdb_bytes_with_identical_logical_catalogs_match() {
        use had_nav_kv::{build_nav_kv_strict, NavKvPair, NavKvRoot};
        let catalog = catalog(&["KSFO", "KJFK"]);
        let compact = serde_json::to_vec(&catalog).unwrap();
        let pretty = serde_json::to_vec_pretty(&catalog).unwrap();
        assert_ne!(compact, pretty);
        let build = |value, size| {
            build_nav_kv_strict(
                vec![NavKvPair {
                    key: crate::NOTAM_AIRPORT_CATALOG_NAV_DB_KEY.into(),
                    value,
                }],
                size,
            )
            .unwrap()
        };
        let first = build(compact, 4096);
        let second = build(pretty, 8192);
        assert_ne!(first.root_bytes, second.root_bytes);
        assert_ne!(first.pages, second.pages);
        for package in [first, second] {
            let root = NavKvRoot::parse(&package.root_bytes).unwrap();
            let value = root
                .extract_value(crate::NOTAM_AIRPORT_CATALOG_NAV_DB_KEY, |page| {
                    package.pages.get(page as usize).cloned()
                })
                .unwrap();
            let decoded: NotamAirportCatalog = serde_json::from_slice(&value).unwrap();
            assert_eq!(decoded.identity(), catalog.identity());
        }
    }

    #[test]
    fn malformed_identity_is_rejected() {
        let valid = catalog(&["KSFO"]).identity().unwrap();
        valid.validate().unwrap();
        for hash in ["", "ABCDEF", &"A".repeat(64), &"g".repeat(64)] {
            let mut invalid = valid.clone();
            invalid.sha256 = hash.to_string();
            assert!(invalid.validate().is_err());
        }
        let mut empty = valid;
        empty.airport_count = 0;
        assert!(empty.validate().is_err());
    }
}
