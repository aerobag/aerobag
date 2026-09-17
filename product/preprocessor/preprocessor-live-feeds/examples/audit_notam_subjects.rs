// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Reproject a private source/client audit capture, without modifying its store.
//! Input: JSON array of {"source": "<canonical source JSON>", "client": null|string}.
use preprocessor_live_feeds::{published_notam_record, StructuredNotamRecord};
use std::collections::{BTreeMap, BTreeSet};

fn main() -> anyhow::Result<()> {
    let rows: Vec<serde_json::Value> = serde_json::from_reader(std::io::stdin())?;
    let mut counts = BTreeMap::<String, usize>::new();
    let mut newly_visible = Vec::new();
    for row in rows {
        let source: StructuredNotamRecord =
            serde_json::from_str(row["source"].as_str().expect("source JSON"))?;
        if let Some(record) = published_notam_record(&source) {
            let kinds = record
                .subjects
                .iter()
                .map(|subject| subject.kind())
                .collect::<BTreeSet<_>>();
            for kind in kinds {
                *counts.entry(kind.into()).or_default() += 1;
            }
            if row["client"].is_null() && !record.subjects.is_empty() {
                newly_visible.push(
                    serde_json::json!({"id": record.id, "subjects": record.subjects,
                    "text": record.display_text(), "keyword": source.notam_keyword}),
                );
            }
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "counts": counts, "newly_visible_count": newly_visible.len(), "newly_visible": newly_visible
        }))?
    );
    Ok(())
}
