// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Explicit maintenance tool, never run by CI. Capture logical records from one
//! available production package, not its encoded root/pages or a cycle pair.

use had_nav_kv::{NavKvLookup, NavKvRoot, NavKvStore};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
};

const AIRPORTS: &[&str] = &["KRNT", "KPAE"];

fn selected(key: &str) -> bool {
    let parts: Vec<_> = key.split('/').collect();
    key.starts_with("aircraft/")
        || key == "magvar/source"
        || (parts.first() == Some(&"magvar")
            && parts
                .get(1)
                .and_then(|s| s.parse::<i32>().ok())
                .is_some_and(|v| (45..=50).contains(&v))
            && parts
                .get(2)
                .and_then(|s| s.parse::<i32>().ok())
                .is_some_and(|v| (-125..=-120).contains(&v)))
        || matches!(
            key,
            "chart/catalog"
                | "vector/manifest"
                | "airport/notam-catalog"
                | "airway/routing/manifest"
        )
        || key.starts_with("airport/info/") && AIRPORTS.contains(&parts[2])
        || key.starts_with("navref/")
            && ["KRNT", "KPAE", "SEA", "ECEPO", "PAE"]
                .iter()
                .any(|id| parts.contains(id))
        || key.starts_with("plate/")
            && key.contains("KPAE")
            && (key.contains("VOR-A") || key == "plate/airport/KPAE")
        || key.starts_with("procedure/geometry/KPAE/APPROACH/VOR-A/")
        || key.starts_with("waypoint/identifier/")
            && ["KRNT", "KPAE", "SEA", "ECEPO", "PAE"].contains(&parts[2])
        || key.starts_with("waypoint/search-prefix/") && ["KRN", "KPA", "SEA"].contains(&parts[2])
}

fn dependencies(value: &Value, keys: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            if let Some(id) = object.get("segment_ref").and_then(Value::as_str) {
                keys.insert(procedure_geometry_types::procedure_geometry_segment_navdb_key(id));
            }
            for field in ["package_name", "package_id"] {
                if let Some(id) = object.get(field).and_then(Value::as_str) {
                    keys.insert(format!("package/by-id/{}", had_key::component(id)));
                }
            }
            if let Some(ids) = object.get("package_ids").and_then(Value::as_array) {
                for id in ids.iter().filter_map(Value::as_str) {
                    keys.insert(format!("package/by-id/{}", had_key::component(id)));
                }
            }
            if let (Some(kind @ ("fix" | "navaid" | "airport")), Some(id)) = (
                object.get("kind").and_then(Value::as_str),
                object.get("value").and_then(Value::as_str),
            ) {
                keys.insert(format!(
                    "navref/position/{kind}/{}",
                    had_key::upper_component(id)
                ));
            }
            for item in object.values() {
                dependencies(item, keys);
            }
        }
        Value::Array(items) => {
            for item in items {
                dependencies(item, keys);
            }
        }
        _ => {}
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: capture NAVDB.zip OUTPUT.json (OUTPUT must not exist)".into());
    }
    if std::path::Path::new(&args[1]).exists() {
        return Err("capture output already exists".into());
    }
    let bytes = fs::read(&args[0])?;
    let package = nav_kv_package::read_package_bytes("capture", &bytes)?;
    let manifest: Value = serde_json::from_slice(&package.manifest)?;
    if manifest["contract_id"] != product_contracts::NAV_DB_CONTRACT_ID {
        return Err("capture requires a genuinely rebuilt current-contract NAVDB".into());
    }
    let root = NavKvRoot::parse(&package.root)?;
    let mut store = NavKvStore::new(root);
    for (index, page) in package.pages.into_iter().enumerate() {
        store.insert_page(u32::try_from(index)?, page);
    }
    let mut records = BTreeMap::<String, Value>::new();
    for key in store.keys_with_prefix("") {
        if selected(&key) {
            let NavKvLookup::Hit(value) = store.get_bytes(&key)? else {
                return Err(format!("cannot read {key}").into());
            };
            records.insert(key, serde_json::from_slice(&value)?);
        }
    }
    records
        .get_mut("chart/catalog")
        .ok_or("missing chart catalog")?
        .as_array_mut()
        .ok_or("chart catalog is not an array")?
        .retain(|record| matches!(record["id"].as_str(), Some("sec:nw" | "tac:nw")));
    records
        .get_mut("airport/notam-catalog")
        .ok_or("missing NOTAM catalog")?["airport_ids"] = json!(AIRPORTS);
    records.insert(
        "airway/routing/manifest".into(),
        serde_json::to_value(product_contracts::AirwayRoutingManifest {
            schema_version: product_contracts::AIRWAY_ROUTING_SCHEMA_VERSION,
            node_count: 0,
            edge_count: 0,
            chunk_count: 0,
        })?,
    );
    let airport = records
        .get_mut("plate/airport/KPAE")
        .ok_or("missing KPAE plates")?;
    airport["chart_ids"]
        .as_array_mut()
        .ok_or("missing chart ids")?
        .retain(|v| v.as_str().is_some_and(|id| id.contains("VOR-A")));
    airport["charted_procedures"]
        .as_array_mut()
        .ok_or("missing charted procedures")?
        .retain(|v| v["procedure_id"] == "VOR-A");
    for (key, value) in &mut records {
        if key.starts_with("waypoint/search-prefix/") {
            value
                .as_array_mut()
                .ok_or("search prefix is not an array")?
                .retain(|record| {
                    matches!(record["identifier"].as_str(), Some("KRNT" | "KPAE" | "SEA"))
                });
        }
    }
    loop {
        let mut needed = BTreeSet::new();
        for value in records.values() {
            dependencies(value, &mut needed);
        }
        needed.retain(|key| !records.contains_key(key));
        if needed.is_empty() {
            break;
        }
        for key in needed {
            let NavKvLookup::Hit(value) = store.get_bytes(&key)? else {
                return Err(format!("missing captured dependency {key}").into());
            };
            records.insert(key, serde_json::from_slice(&value)?);
        }
    }
    let document = json!({
        "schema_version": 1,
        "nav_db_contract": product_contracts::nav_db_contract_descriptor(),
        "provenance": {
            "source_package_sha256": format!("{:x}", Sha256::digest(&bytes)),
            "source_package_manifest": manifest,
            "capture_scope": "KRNT SEA KPAE, KPAE VOR-A and its shared geometry, local magnetic variation, two NW chart catalogs; no imagery",
        },
        "records": records,
    });
    fs::write(&args[1], serde_json::to_vec_pretty(&document)?)?;
    println!("{} logical records captured", records.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permanent_source_keeps_all_referenced_geometry_packages_and_positions() {
        let source: Value = serde_json::from_str(nav_db_fixture::SOURCE).unwrap();
        let records = source["records"].as_object().unwrap();
        let mut needed = BTreeSet::new();
        for value in records.values() {
            dependencies(value, &mut needed);
        }
        for prefix in [
            "procedure/geometry-segment/",
            "package/by-id/",
            "navref/position/",
        ] {
            assert!(
                needed.iter().any(|key| key.starts_with(prefix)),
                "missing {prefix} coverage"
            );
        }
        let missing: Vec<_> = needed
            .iter()
            .filter(|key| !records.contains_key(*key))
            .collect();
        assert!(
            missing.is_empty(),
            "source references uncaptured records: {missing:?}"
        );
    }
}
