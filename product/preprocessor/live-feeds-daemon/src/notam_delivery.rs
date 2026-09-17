// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::BTreeSet, path::Path, sync::Arc};

use serde::{Deserialize, Serialize};

#[derive(Default)]
pub(super) struct DeliveryAudit {
    omitted: Option<Arc<BTreeSet<String>>>,
    tfr: Option<(String, Result<BTreeSet<String>, String>)>,
    summary: DeliverySummary,
}

#[derive(Clone, Default, Serialize)]
pub(super) struct DeliverySummary {
    tfr_version: Option<String>,
    tfr_overlap_count: Option<usize>,
    without_delivery_count: Option<usize>,
    error: Option<String>,
}

impl DeliveryAudit {
    pub fn needs_tfr_read(&self, version: &str) -> bool {
        !matches!(&self.tfr, Some((current, Ok(_))) if current == version)
    }

    pub fn set_notams(&mut self, omitted: Arc<BTreeSet<String>>) {
        self.omitted = Some(omitted);
        self.refresh();
    }

    pub fn set_tfr(&mut self, version: String, ids: Result<BTreeSet<String>, String>) {
        self.tfr = Some((version, ids));
        self.refresh();
    }

    fn refresh(&mut self) {
        self.summary = DeliverySummary {
            tfr_version: self.tfr.as_ref().map(|(version, _)| version.clone()),
            ..Default::default()
        };
        match (&self.omitted, &self.tfr) {
            (Some(omitted), Some((_, Ok(tfr)))) => {
                let overlap = omitted.intersection(tfr).count();
                self.summary.tfr_overlap_count = Some(overlap);
                self.summary.without_delivery_count = Some(omitted.len() - overlap);
            }
            (_, Some((_, Err(error)))) => self.summary.error = Some(error.clone()),
            _ => self.summary.error = Some("Waiting for NOTAM and TFR publication evidence".into()),
        }
    }

    pub fn summary(&self) -> &DeliverySummary {
        &self.summary
    }
}

#[derive(Deserialize)]
struct TfrState {
    schema_version: u32,
    areas: Vec<TfrArea>,
}

#[derive(Deserialize)]
struct TfrArea {
    notam: Option<TfrNotam>,
}

#[derive(Deserialize)]
struct TfrNotam {
    record_id: Option<String>,
    text: Option<String>,
    local_text: Option<String>,
    icao_text: Option<String>,
}

pub(super) fn read_tfr_ids(path: &Path) -> Result<BTreeSet<String>, String> {
    let encoded = std::fs::read(path).map_err(|e| format!("TFR delivery audit read: {e}"))?;
    let bytes = nav_kv_package::decode_xz_if_needed(&encoded)?;
    let state: TfrState =
        serde_json::from_slice(&bytes).map_err(|e| format!("TFR delivery audit decode: {e}"))?;
    if state.schema_version != product_contracts::TFR_PRODUCT_CONTRACT_VERSION {
        return Err(format!(
            "Unsupported TFR delivery audit schema {}",
            state.schema_version
        ));
    }
    Ok(state
        .areas
        .into_iter()
        .filter_map(|area| {
            let notam = area.notam?;
            let text = notam.text.or(notam.local_text).or(notam.icao_text)?;
            let id = notam.record_id?;
            (!text.trim().is_empty() && !id.trim().is_empty()).then_some(id)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(values: &[&str]) -> BTreeSet<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn publication_order_duplicates_removal_and_unknown_are_accounted_for() {
        for tfr_first in [false, true] {
            let mut audit = DeliveryAudit::default();
            if tfr_first {
                audit.set_tfr("a".into(), Ok(ids(&["A", "A", "OTHER"])));
            }
            audit.set_notams(Arc::new(ids(&["A", "B"])));
            if !tfr_first {
                assert_eq!(audit.summary().without_delivery_count, None);
                audit.set_tfr("a".into(), Ok(ids(&["A", "A", "OTHER"])));
            }
            assert_eq!(audit.summary().tfr_overlap_count, Some(1));
            assert_eq!(audit.summary().without_delivery_count, Some(1));
            assert!(!audit.needs_tfr_read("a"));
            audit.set_tfr("b".into(), Ok(BTreeSet::new()));
            assert_eq!(audit.summary().without_delivery_count, Some(2));
            audit.set_notams(Arc::new(ids(&["B"])));
            assert_eq!(audit.summary().without_delivery_count, Some(1));
            audit.set_tfr("c".into(), Err("corrupt TFR publication".into()));
            assert_eq!(audit.summary().without_delivery_count, None);
            assert!(audit.needs_tfr_read("c"));
            assert!(audit.summary().error.is_some());
        }
    }

    #[test]
    fn reading_tfr_counts_delivered_text_once_not_areas_or_missing_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.json");
        let state = serde_json::json!({
            "schema_version": product_contracts::TFR_PRODUCT_CONTRACT_VERSION,
            "areas": [
                {"notam": {"record_id": "A", "text": "Restriction"}},
                {"notam": {"record_id": "A", "text": "Restriction"}},
                {"notam": {"record_id": "B"}}, {"notam": null}
            ]
        });
        std::fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(read_tfr_ids(&path).unwrap(), ids(&["A"]));
        std::fs::write(
            &path,
            nav_kv_package::xz_frame_uncompressed_bytes(&serde_json::to_vec(&state).unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(read_tfr_ids(&path).unwrap(), ids(&["A"]));
        std::fs::write(&path, b"{}").unwrap();
        assert!(read_tfr_ids(&path).is_err());
    }
}
