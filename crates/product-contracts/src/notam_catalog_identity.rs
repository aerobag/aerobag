// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

use crate::NotamAirportCatalog;

/// Canonical representation v2: domain bytes (including NUL), catalog schema
/// as big-endian u32, unique airport count as big-endian u64, then each sorted
/// UTF-8 ID prefixed by its byte length as big-endian u64. No JSON or NAVDB
/// package bytes participate. The sorted alias count and length-prefixed alias /
/// target pairs follow the IDs. Changing this algorithm requires a new version.
const IDENTITY_DOMAIN: &[u8] = b"aerobag/notam-airport-catalog/v2\0";

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
        let catalogs: Vec<_> = catalogs.into_iter().collect();
        for catalog in &catalogs {
            catalog.identity()?;
        }
        let (catalog, errors) = Self::union_for_delivery(catalogs)?;
        if !errors.is_empty() {
            return Err(errors.join("; "));
        }
        Ok(catalog)
    }

    /// Keep unambiguous associations for live delivery, with explicit diagnostics
    /// for every omission. The resulting identity describes only retained data.
    /// Producers and qualification should continue to use the strict `union`.
    pub fn union_for_delivery<'a>(
        catalogs: impl IntoIterator<Item = &'a Self>,
    ) -> Result<(Self, Vec<String>), String> {
        let mut union = Self {
            schema_version: Self::SCHEMA_VERSION,
            airport_ids: Default::default(),
            aliases: Default::default(),
        };
        let valid_id = |id: &str| {
            !id.is_empty()
                && id
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        };
        let mut errors = BTreeSet::new();
        let mut rejected = BTreeSet::new();
        let mut targets: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for catalog in catalogs {
            if catalog.schema_version != Self::SCHEMA_VERSION {
                return Err(format!(
                    "unsupported NOTAM airport catalog schema {}",
                    catalog.schema_version
                ));
            }
            if catalog.airport_ids.is_empty() {
                errors.insert("NOTAM airport catalog is empty".into());
            }
            for id in &catalog.airport_ids {
                if valid_id(id) {
                    union.airport_ids.insert(id.clone());
                } else {
                    errors.insert(format!("NOTAM airport catalog contains invalid ID {id:?}"));
                }
            }
            for (alias, target) in &catalog.aliases {
                if !valid_id(alias)
                    || !valid_id(target)
                    || !catalog.airport_ids.contains(target)
                    || (catalog.airport_ids.contains(alias) && alias != target)
                {
                    errors.insert(format!(
                        "invalid NOTAM airport alias {alias:?} -> {target:?}"
                    ));
                    rejected.insert(alias.clone());
                }
                targets
                    .entry(alias.clone())
                    .or_default()
                    .insert(target.clone());
            }
        }
        for (alias, choices) in &targets {
            if choices.len() != 1 {
                errors.insert(format!(
                    "conflicting NOTAM airport alias {alias}: {choices:?}"
                ));
                rejected.insert(alias.clone());
            }
        }
        // An ID can be canonical in an older cycle and an alias in a newer
        // one. Follow only explicit FAA alias edges, never name heuristics.
        for alias in targets.keys() {
            if rejected.contains(alias) {
                continue;
            }
            let mut visited = BTreeSet::new();
            let mut target = alias;
            let result = loop {
                if rejected.contains(target) {
                    break Err(format!(
                        "NOTAM airport alias {alias} depends on rejected alias {target}"
                    ));
                }
                if !visited.insert(target) {
                    break Err(format!("cyclic NOTAM airport alias {alias}: {visited:?}"));
                }
                let Some(next) = targets.get(target).and_then(|choices| choices.first()) else {
                    break Ok(target);
                };
                if next == target {
                    break Ok(target);
                }
                target = next;
            };
            match result {
                Ok(target) => {
                    union.aliases.insert(alias.clone(), target.clone());
                }
                Err(error) => {
                    errors.insert(error);
                }
            }
        }
        // Do not resurrect a rejected alias as a canonical airport from another
        // cycle. That would attach notices to an ambiguous identity anyway.
        for alias in targets.keys() {
            if union.aliases.get(alias) != Some(alias) {
                union.airport_ids.remove(alias);
            }
        }
        union.identity()?;
        Ok((union, errors.into_iter().collect()))
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
    hash.update((catalog.aliases.len() as u64).to_be_bytes());
    for (alias, target) in &catalog.aliases {
        for id in [alias, target] {
            hash.update((id.len() as u64).to_be_bytes());
            hash.update(id.as_bytes());
        }
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
            aliases: Default::default(),
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
            "4c98f3bf197d671459fe5327fa89338a0f50ede43d42f9bf4fbf3dfa8edb8a43"
        );
        assert_eq!(
            expected.identity(),
            catalog(&["1S5", "KSFO", "KJFK", "KSFO"]).identity()
        );
    }

    #[test]
    fn cross_cycle_canonical_id_becomes_alias() {
        let old = catalog(&["FLT"]);
        let mut new = catalog(&["PAFT"]);
        new.aliases.insert("FLT".into(), "PAFT".into());
        for cycles in [[&old, &new], [&new, &old]] {
            assert_eq!(NotamAirportCatalog::union(cycles).unwrap(), new);
        }
    }

    #[test]
    fn cross_cycle_alias_chains_are_flattened_and_cycles_rejected() {
        let mut first = catalog(&["BBB"]);
        first.aliases.insert("AAA".into(), "BBB".into());
        let mut second = catalog(&["CCC"]);
        second.aliases.insert("BBB".into(), "CCC".into());
        let mut expected = catalog(&["CCC"]);
        expected.aliases = [("AAA".into(), "CCC".into()), ("BBB".into(), "CCC".into())].into();
        for cycles in [[&first, &second], [&second, &first]] {
            assert_eq!(NotamAirportCatalog::union(cycles).unwrap(), expected);
        }
        let mut reversed = catalog(&["AAA"]);
        reversed.aliases.insert("BBB".into(), "AAA".into());
        for cycles in [[&first, &reversed], [&reversed, &first]] {
            assert!(NotamAirportCatalog::union(cycles).is_err());
        }
    }

    #[test]
    fn delivery_union_omits_only_invalid_associations() {
        let mut source = catalog(&["KSEA", "PAFT", "bad id"]);
        source.aliases = [
            ("FLT".into(), "PAFT".into()),
            ("BAD".into(), "MISSING".into()),
            ("not upper".into(), "PAFT".into()),
        ]
        .into();
        let (usable, errors) = NotamAirportCatalog::union_for_delivery([&source]).unwrap();
        assert_eq!(usable.airport_ids, catalog(&["KSEA", "PAFT"]).airport_ids);
        assert_eq!(usable.aliases, [("FLT".into(), "PAFT".into())].into());
        assert_eq!(errors.len(), 3);
        usable.identity().unwrap();
        assert!(NotamAirportCatalog::union([&source]).is_err());
    }

    #[test]
    fn delivery_union_conflicts_and_dependents_are_not_order_dependent() {
        let mut first = catalog(&["PAFT", "PXYZ"]);
        first.aliases.insert("FLT".into(), "PAFT".into());
        let mut second = first.clone();
        second.aliases.insert("FLT".into(), "PXYZ".into());
        let mut old = catalog(&["FLT"]);
        old.aliases.insert("OLD".into(), "FLT".into());
        let expected = NotamAirportCatalog::union_for_delivery([&first, &second, &old]).unwrap();
        for cycles in [[&second, &first, &old], [&old, &second, &first]] {
            assert_eq!(
                NotamAirportCatalog::union_for_delivery(cycles).unwrap(),
                expected
            );
        }
        let (usable, errors) = expected;
        assert_eq!(usable, catalog(&["PAFT", "PXYZ"]));
        assert_eq!(errors.len(), 2);
        assert!(errors.iter().any(|e| e.contains("OLD")));
        assert!(errors
            .iter()
            .any(|e| e.contains("conflicting") && e.contains("FLT")));
    }

    #[test]
    fn delivery_union_quarantines_cycles_but_keeps_independent_airports() {
        let mut first = catalog(&["BBB", "KSEA"]);
        first.aliases.insert("AAA".into(), "BBB".into());
        let mut second = catalog(&["AAA"]);
        second.aliases.insert("BBB".into(), "AAA".into());
        for cycles in [[&first, &second], [&second, &first]] {
            let (usable, errors) = NotamAirportCatalog::union_for_delivery(cycles).unwrap();
            assert_eq!(usable, catalog(&["KSEA"]));
            assert_eq!(errors.len(), 2);
            assert!(errors.iter().all(|error| error.contains("cyclic")));
        }
    }

    #[test]
    fn aliases_affect_identity_and_conflicts_are_rejected() {
        let original = catalog(&["PAAB", "PXYZ"]);
        let mut first = original.clone();
        first.aliases.insert("4A2".into(), "PAAB".into());
        let mut second = first.clone();
        second.aliases.insert("4A2".into(), "PXYZ".into());
        assert_ne!(original.identity(), first.identity());
        assert_ne!(first.identity(), second.identity());
        assert_eq!(NotamAirportCatalog::union([&first, &first]).unwrap(), first);
        for pair in [[&first, &second], [&second, &first]] {
            assert!(NotamAirportCatalog::union(pair).is_err());
        }
        second.aliases.insert("4A2".into(), "MISSING".into());
        assert!(second.identity().is_err());
        second.aliases = std::collections::BTreeMap::from([("PAAB".into(), "PXYZ".into())]);
        assert!(second.identity().is_err());
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
