// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

fn generated_contract() -> serde_json::Value {
    serde_json::json!({
        "account_version": account_format::CURRENT.version,
        "page_version": CLOUD_PAGE_VERSION,
        "envelope_version": CLOUD_ENVELOPE_VERSION,
        "root": schemars::schema_for!(CloudNode),
        "page": schemars::schema_for!(CloudPage),
        "envelope": schemars::schema_for!(CloudEnvelope),
        "records": records::contract_inventory(),
    })
}

fn snapshot(version: u32) -> serde_json::Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("src/cloud/contracts/account-v{version}.json"));
    serde_json::from_slice(&std::fs::read(&path).unwrap_or_else(|error| {
        panic!("Missing account contract {}: {error}. Add a new version, never overwrite a published snapshot.", path.display())
    })).unwrap()
}

#[test]
fn every_persisted_type_matches_the_frozen_account_contract() {
    assert_eq!(generated_contract(), snapshot(account_format::CURRENT.version),
        "Cloud wire contract changed without a new account format and explicit migration. Runtime refactors must convert to unchanged wire types.");
}

#[test]
fn every_changed_family_has_an_explicit_migration() {
    let mut format = &account_format::CURRENT;
    while let Some(prior) = format.predecessor {
        let current = if format.version == account_format::CURRENT.version {
            generated_contract()
        } else {
            snapshot(format.version)
        };
        check_migration_contract(format, &current, &snapshot(prior.version));
        format = prior;
    }
}

fn check_migration_contract(
    format: &AccountFormat,
    current: &serde_json::Value,
    old: &serde_json::Value,
) {
    let current_records = current["records"].as_array().unwrap();
    let old_records = old["records"].as_array().unwrap();
    for record in old_records.iter().chain(current_records) {
        let previous = old_records
            .iter()
            .find(|entry| entry["key"] == record["key"]);
        let next = current_records
            .iter()
            .find(|entry| entry["key"] == record["key"]);
        // Format 1 predates schema snapshots and was already published with
        // several FP versions. Its explicit historical version set is frozen;
        // from format 2 onward compare the complete recursively derived shape.
        let changed = match (previous, next) {
            (Some(previous), Some(next)) if previous.get("legacy_schema_versions").is_some() => {
                previous["legacy_schema_versions"]
                    != serde_json::json!([next["schema_version"].clone()])
            }
            (Some(previous), Some(next)) => previous != next,
            _ => true,
        };
        if changed {
            let rules = format
                .migrations
                .iter()
                .filter(|rule| rule.key.description() == record["key"])
                .collect::<Vec<_>>();
            assert_eq!(
                rules.len(),
                1,
                "Expected one explicit migration for {}",
                record["key"]
            );
            let rule = rules[0];
            if let Some(previous) = previous {
                let versions = previous
                    .get("legacy_schema_versions")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!([previous["schema_version"].clone()]));
                assert!(
                    versions.as_array().unwrap().iter().all(|version| rule
                        .source_versions
                        .contains(&(version.as_u64().unwrap() as u32))),
                    "Migration must cover every declared historical version"
                );
            }
            if let Some(target) = rule.target_version {
                assert_eq!(
                    Some(serde_json::json!(target)),
                    next.map(|record| record["schema_version"].clone()),
                    "Migration output must have the newly declared schema"
                );
            }
        }
    }
}

#[test]
fn migration_is_atomic_and_cannot_change_unrelated_records_or_mutation_times() {
    let original = CloudRecord::fixture(1, Some(42), serde_json::json!("old"));
    let unrelated = CloudRecord::fixture(99, Some(17), serde_json::json!({"untouched": true}));
    let initial = BTreeMap::from([
        ("test/first".into(), original.clone()),
        ("test/value".into(), original),
        ("unknown/family".into(), unrelated),
    ]);
    let bad = Box::leak(Box::new(AccountFormat {
        version: account_format::CURRENT.version + 1,
        predecessor: Some(&account_format::CURRENT),
        decode_node: account_format::CURRENT.decode_node,
        decode_page: account_format::CURRENT.decode_page,
        encode_page: account_format::CURRENT.encode_page,
        migrations: &[
            account_format::RecordMigration {
                key: KeyPattern::Exact("test/first"),
                source_versions: &[1],
                target_version: Some(2),
                convert: |record| {
                    Ok(Some(CloudRecord::fixture(
                        2,
                        record.modified_at_epoch_ms(),
                        serde_json::json!("success before failure"),
                    )))
                },
                explanation: "test only",
            },
            account_format::RecordMigration {
                key: KeyPattern::Exact("test/value"),
                source_versions: &[1],
                target_version: Some(2),
                convert: |_| {
                    Ok(Some(CloudRecord::fixture(
                        2,
                        Some(999),
                        serde_json::json!("new"),
                    )))
                },
                explanation: "test only",
            },
        ],
    }));
    let mut records = initial.clone();
    assert!(bad
        .migrate(account_format::CURRENT.version, &mut records)
        .is_err());
    assert_eq!(records, initial);
}

// Deliberately no "update snapshots" mode. This produces a candidate for a
// newly numbered file; CI independently forbids editing historical files.
#[test]
#[ignore = "Print a candidate contract for a NEW account version"]
fn print_new_account_contract() {
    println!(
        "BEGIN_ACCOUNT_CONTRACT\n{}\nEND_ACCOUNT_CONTRACT",
        serde_json::to_string_pretty(&generated_contract()).unwrap()
    );
}
