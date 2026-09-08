// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Calendar-independent logical inputs for the NAVDB transaction tests.

pub const SOURCE: &str = include_str!("../source.json");

use had_nav_kv::{build_nav_kv_sorted_with_extra_prefetch_keys, NavKvPair, NAVKV_STORAGE_FORMAT};
use nav_kv_package::NavKvPackageMembers;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const INITIAL_CYCLE: &str = "9901";
pub const CANDIDATE_CYCLE: &str = "9902";
pub const REJECTED_NAV_KEY: &str = "navref/position/navaid/SEA";
pub const CHANGED_NAV_KEY: &str = "airport/info/KRNT";
pub const INITIAL_AIRPORT_NAME: &str = "RENTON MUNI";
pub const CANDIDATE_AIRPORT_NAME: &str = "RENTON MUNI - ROLLOVER B";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Generation {
    Initial,
    Candidate,
    Rejected,
}

impl Generation {
    pub fn cycle(self) -> &'static str {
        match self {
            Self::Initial => INITIAL_CYCLE,
            Self::Candidate | Self::Rejected => CANDIDATE_CYCLE,
        }
    }

    pub fn package_id(self) -> String {
        format!(
            "NAV_DB_{}_{}_ROLLOVER_{}",
            product_contracts::NAV_DB_CONTRACT_ID,
            self.cycle(),
            if self == Self::Rejected {
                "REJECT"
            } else {
                "VALID"
            }
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    schema_version: u32,
    nav_db_contract: product_contracts::NavDbContractDescriptor,
    provenance: Value,
    records: BTreeMap<String, Value>,
}

fn source_records(source: &str) -> Result<BTreeMap<String, Value>, String> {
    let source: Source = serde_json::from_str(source).map_err(|error| error.to_string())?;
    if source.schema_version != 1 {
        return Err("unsupported NAVDB rollover source schema".into());
    }
    if source.nav_db_contract != product_contracts::nav_db_contract_descriptor() {
        return Err("NAVDB rollover logical source needs an explicit contract migration; update its records and descriptor, never relabel encoded pages".into());
    }
    if !source.provenance.is_object() {
        return Err("NAVDB rollover source must record its provenance".into());
    }
    for (key, schema) in &source.nav_db_contract.required_exact_keys {
        if source
            .records
            .get(key)
            .and_then(|value| value["schema_version"].as_u64())
            != Some(u64::from(*schema))
        {
            return Err(format!(
                "NAVDB rollover source lacks required {key} schema {schema}"
            ));
        }
    }
    for key in [
        REJECTED_NAV_KEY,
        CHANGED_NAV_KEY,
        "plate/airport/KPAE",
        "chart/catalog",
        "vector/manifest",
    ] {
        if !source.records.contains_key(key) {
            return Err(format!("NAVDB rollover source lacks scenario record {key}"));
        }
    }
    if source.records[CHANGED_NAV_KEY]["name"] != INITIAL_AIRPORT_NAME {
        return Err("NAVDB rollover source changed its baseline airport name".into());
    }
    Ok(source.records)
}

/// Build fresh roots and pages with the same encoder used by the producer.
/// The source stores logical values, never storage pages or a historic cycle.
pub fn build(generation: Generation) -> Result<NavKvPackageMembers, String> {
    let mut records = source_records(SOURCE)?;
    records.insert(
        "contract/nav-db".into(),
        json!({"contract_id": product_contracts::NAV_DB_CONTRACT_ID}),
    );
    // Captured ancillary IDs remain opaque references, but their original FAA
    // validity windows must not age this transaction fixture out. Freshness is
    // tested separately; the lab supplies controlled dates for the NAVDBs.
    for (key, record) in &mut records {
        if key.starts_with("package/by-id/") {
            record["effective_date"] = Value::Null;
            record["expiration_date"] = Value::Null;
        }
    }
    if generation != Generation::Initial {
        records
            .get_mut(CHANGED_NAV_KEY)
            .expect("validated airport record")["name"] = json!(CANDIDATE_AIRPORT_NAME);
    }
    if generation == Generation::Rejected {
        records.remove(REJECTED_NAV_KEY);
    }
    let aircraft_keys = records
        .keys()
        .filter(|key| key.starts_with("aircraft/definition/"))
        .cloned()
        .collect::<Vec<_>>();
    let pairs = records
        .into_iter()
        .map(|(key, value)| {
            serde_json::to_vec(&value)
                .map(|value| NavKvPair { key, value })
                .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let built = build_nav_kv_sorted_with_extra_prefetch_keys(pairs, 64 * 1024, &aircraft_keys)?;
    let manifest = serde_json::to_vec_pretty(&json!({
        "schema_version": 1,
        "product_id": "nav-db",
        "contract_id": product_contracts::NAV_DB_CONTRACT_ID,
        "encoding": format!("had-nav-kv-v{NAVKV_STORAGE_FORMAT}"),
        "root": "root",
        "page_path_template": "page_{page:04}",
        "page_count": built.pages.len(),
        "page_size": built.page_size,
        "logical_bytes_len": built.logical_bytes_len,
        "value_bytes_len": built.value_bytes_len,
    }))
    .map_err(|error| error.to_string())?;
    Ok(NavKvPackageMembers {
        manifest,
        root: built.root_bytes,
        pages: built.pages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use had_nav_kv::{NavKvLookup, NavKvRoot, NavKvStore};

    fn records(generation: Generation) -> BTreeMap<String, Vec<u8>> {
        let built = build(generation).unwrap();
        let zip = nav_kv_package::write_stored_xz_framed_package_bytes(
            &built.manifest,
            &built.root,
            &built.pages,
        )
        .unwrap();
        let read = nav_kv_package::read_package_bytes("roundtrip", &zip).unwrap();
        assert_eq!(read, built);
        let mut store = NavKvStore::new(NavKvRoot::parse(&read.root).unwrap());
        for (index, page) in read.pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }
        store
            .keys_with_prefix("")
            .into_iter()
            .map(|key| {
                let NavKvLookup::Hit(value) = store.get_bytes(&key).unwrap() else {
                    panic!("missing {key}");
                };
                (key, value)
            })
            .collect()
    }

    #[test]
    fn packages_are_rebuilt_deterministically_and_have_observable_differences() {
        assert_eq!(
            build(Generation::Initial).unwrap(),
            build(Generation::Initial).unwrap()
        );
        let initial = records(Generation::Initial);
        let candidate = records(Generation::Candidate);
        let rejected = records(Generation::Rejected);
        assert_eq!(initial.len(), candidate.len());
        let changed: Vec<_> = initial
            .iter()
            .filter(|(key, value)| candidate.get(*key) != Some(*value))
            .map(|(key, _)| key.as_str())
            .collect();
        assert_eq!(changed, [CHANGED_NAV_KEY]);
        assert_eq!(
            serde_json::from_slice::<Value>(&candidate[CHANGED_NAV_KEY]).unwrap()["name"],
            CANDIDATE_AIRPORT_NAME
        );
        assert!(!rejected.contains_key(REJECTED_NAV_KEY));
        assert_eq!(candidate.len(), rejected.len() + 1);
        for (key, value) in rejected {
            assert_eq!(candidate[&key], value);
        }
    }

    #[test]
    fn semantic_contract_changes_fail_closed_until_source_is_migrated() {
        let mut source: Value = serde_json::from_str(SOURCE).unwrap();
        source["nav_db_contract"]["contract_id"] = json!("NAV-obsolete");
        assert!(source_records(&source.to_string())
            .unwrap_err()
            .contains("explicit contract migration"));
    }

    #[test]
    fn ancillary_packages_do_not_expire_with_the_captured_faa_cycle() {
        let packages: Vec<_> = records(Generation::Initial)
            .into_iter()
            .filter(|(key, _)| key.starts_with("package/by-id/"))
            .collect();
        assert!(!packages.is_empty());
        for (key, bytes) in packages {
            let record: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(record["effective_date"], Value::Null, "{key}");
            assert_eq!(record["expiration_date"], Value::Null, "{key}");
        }
    }

    #[test]
    fn missing_required_source_records_are_not_silently_fabricated() {
        let mut source: Value = serde_json::from_str(SOURCE).unwrap();
        source["records"]
            .as_object_mut()
            .unwrap()
            .remove(product_contracts::AIRWAY_ROUTING_MANIFEST_KEY);
        assert!(source_records(&source.to_string())
            .unwrap_err()
            .contains("airway/routing/manifest"));
    }
}
