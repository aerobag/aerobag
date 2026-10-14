// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Read-only audit of canonical NMS records (one JSON object per line) on stdin.
//! Exercises the production join, checks ownership, and reverses source order.
use preprocessor_live_feeds::{tfr_notam_metadata_by_fdc_id, StructuredNotamRecord};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn main() -> anyhow::Result<()> {
    let records = serde_json::Deserializer::from_reader(std::io::stdin())
        .into_iter::<StructuredNotamRecord>()
        .collect::<Result<Vec<_>, _>>()?;
    let source_by_id = records
        .iter()
        .map(|record| (record.id.as_str(), record))
        .collect::<BTreeMap<_, _>>();
    let forward = tfr_notam_metadata_by_fdc_id(&records);
    let reversed = tfr_notam_metadata_by_fdc_id(&records.iter().rev().cloned().collect::<Vec<_>>());
    let mut owners = BTreeMap::<String, BTreeSet<&str>>::new();
    for record in &records {
        if let Some(own) = source_identity(record) {
            owners.entry(own).or_default().insert(&record.id);
        }
    }
    let missing = owners
        .iter()
        .filter(|(id, records)| records.len() == 1 && !forward.contains_key(*id))
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    let mut wrong_owner = Vec::new();
    let mut order_dependent = Vec::new();
    for (key, metadata) in &forward {
        let metadata = serde_json::to_value(metadata)?;
        let source = source_by_id[metadata["record_id"].as_str().unwrap()];
        let own = source_identity(source);
        if own.as_ref() != Some(key) {
            wrong_owner.push(json!({"joined_as": key, "own_number": own, "record_id": source.id,
                "source_type": source.source_type, "text": source.local_text.as_ref().or(source.text.as_ref())}));
        }
        if reversed.get(key) != forward.get(key) {
            order_dependent.push(json!({"key": key, "forward": metadata["record_id"],
                "reverse": serde_json::to_value(reversed.get(key))?["record_id"]}));
        }
    }
    let report: Value = json!({"source_records": records.len(), "indexed_numbers": forward.len(),
        "wrong_owner_count": wrong_owner.len(), "order_dependent_count": order_dependent.len(),
        "missing_unambiguous_owner_count": missing.len(), "missing_unambiguous_owners": missing,
        "conflicting_own_numbers": owners.iter().filter(|(_, owners)| owners.len() > 1).collect::<BTreeMap<_,_>>(),
        "wrong_owners": wrong_owner, "order_dependent": order_dependent});
    println!("{}", serde_json::to_string_pretty(&report)?);
    anyhow::ensure!(
        report["wrong_owner_count"] == 0
            && report["order_dependent_count"] == 0
            && report["missing_unambiguous_owner_count"] == 0,
        "TFR enrichment ownership/order audit failed"
    );
    Ok(())
}

// Independent oracle: accept only the source's structured FDC designation, not
// anything parsed from display text or produced by the enrichment implementation.
fn source_identity(record: &StructuredNotamRecord) -> Option<String> {
    if record.source_type.as_deref()? != "F" {
        return None;
    }
    let year = record.notam_year.as_deref()?.parse::<u16>().ok()?;
    let number = record.notam_number.as_deref()?;
    let (prefix, sequence) = number
        .split_once('/')
        .map(|(a, b)| Some((a.parse::<u16>().ok()?, b.parse::<u16>().ok()?)))
        .unwrap_or_else(|| Some((year % 10, number.parse::<u16>().ok()?)))?;
    (year >= 1000 && prefix == year % 10 && sequence < 10000)
        .then(|| format!("{prefix}/{sequence:04}"))
}
