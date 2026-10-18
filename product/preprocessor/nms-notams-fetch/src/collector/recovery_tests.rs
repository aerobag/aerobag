// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::tests::{timestamp, update_xml, UpdateIdentity};
use super::*;
use tempfile::tempdir;

const BASELINE_AT: &str = "2026-07-23T14:00:00Z";
const REJECTED_AT: &str = "2026-07-23T14:05:00Z";
const LATER_AT: &str = "2026-07-23T14:10:00Z";

fn notice(id: &str, number: &str, text: &str, at: &str) -> String {
    update_xml(
        UpdateIdentity {
            nms_id: id,
            classification: "FDC",
            location: "MHK",
            number,
        },
        Some("N"),
        &format!("!FDC 6/{number} MHK TEST"),
        text,
        at,
        None,
        "209907241405",
    )
}

fn setup(root: &Path) -> anyhow::Result<(NmsApiCollectorStore, Vec<StructuredNotamRecord>)> {
    let store = NmsApiCollectorStore::new(root);
    store.initialize()?;
    let baseline = vec![
        parse_nms_api_update(
            &notice("1000000000000001", "1111", "BASELINE", BASELINE_AT),
            NmsNotamClassification::Fdc,
        )?
        .record,
    ];
    store.install_baseline(
        "fixture",
        None,
        timestamp(BASELINE_AT),
        Path::new("/fixture/baseline"),
        &baseline,
    )?;
    Ok((store, baseline))
}

fn quarantine(
    store: &NmsApiCollectorStore,
    xml: &str,
    at: &str,
    classification: NmsNotamClassification,
) -> anyhow::Result<()> {
    store.apply_poll_at(
        timestamp(at),
        timestamp(BASELINE_AT),
        timestamp(at),
        vec![],
        vec![],
    )?;
    let mut connection = store.open_connection()?;
    let tx = connection.transaction()?;
    let poll_id = tx.query_row("SELECT MAX(poll_id) FROM poll_runs", [], |row| row.get(0))?;
    retain_rejected_update(
        &tx,
        poll_id,
        timestamp(at),
        &RejectedRawUpdate {
            classification,
            payload_sha256: sha256_hex(xml.as_bytes()),
            raw_aixm: xml.to_string(),
            reason: "old normalizer: NOTAM year is missing".to_string(),
        },
    )?;
    tx.commit()?;
    Ok(())
}

fn poll(store: &NmsApiCollectorStore, xml: Vec<String>, at: &str) -> anyhow::Result<()> {
    store.apply_poll_at(
        timestamp(at),
        timestamp(BASELINE_AT),
        timestamp(at),
        vec![],
        xml,
    )?;
    Ok(())
}

#[test]
fn recovery_restores_captured_variants_with_contiguous_changes_and_is_idempotent(
) -> anyhow::Result<()> {
    let temp = tempdir()?;
    let (store, baseline) = setup(temp.path())?;
    for (xml, classification) in [
        (
            include_str!(
                "../../../preprocessor-live-feeds/tests/fixtures/nms_missing_year_domestic.xml"
            ),
            NmsNotamClassification::Domestic,
        ),
        (
            include_str!(
                "../../../preprocessor-live-feeds/tests/fixtures/nms_missing_year_fdc.xml"
            ),
            NmsNotamClassification::Fdc,
        ),
    ] {
        quarantine(&store, xml, "2026-10-17T10:12:00Z", classification)?;
    }
    let before = store.canonical_source_cursor()?;
    let poll_cursor = store.poll_cursor()?;
    assert_eq!(
        store.recover_from_baseline(
            &baseline,
            timestamp(BASELINE_AT),
            timestamp("2026-10-17T10:13:00Z")
        )?,
        2
    );
    let records = store.current_records()?;
    assert_eq!(records.len(), 3);
    assert!(records
        .iter()
        .any(|record| record.id == "NMS:7292862494853778"));
    assert!(records
        .iter()
        .any(|record| record.id == "NMS:6046133424189660"));
    let batch = store
        .canonical_changes_after(&before)?
        .expect("continuous repair delta");
    assert_eq!(batch.changes.len(), 2);
    assert_eq!(batch.through_sequence, before.through_sequence + 2);
    assert_eq!(store.poll_cursor()?, poll_cursor);
    let repaired_cursor = store.canonical_source_cursor()?;
    assert_eq!(
        store.recover_from_baseline(
            &baseline,
            timestamp(BASELINE_AT),
            timestamp("2026-10-17T10:13:00Z")
        )?,
        0
    );
    assert_eq!(store.canonical_source_cursor()?, repaired_cursor);
    let connection = store.open_connection()?;
    assert_eq!(
        connection.query_row("SELECT COUNT(*) FROM rejected_updates", [], |row| row
            .get::<_, i64>(0))?,
        2
    );
    assert_eq!(
        connection.query_row("SELECT COUNT(*) FROM raw_updates", [], |row| row
            .get::<_, i64>(0))?,
        2
    );
    Ok(())
}

#[test]
fn recovery_honors_later_cancellations_replacements_newer_records_and_expiry() -> anyhow::Result<()>
{
    for case in [
        "cancel-self",
        "cancel-reference",
        "replacement",
        "newer",
        "expired",
    ] {
        let temp = tempdir()?;
        let (store, baseline) = setup(temp.path())?;
        let mut rejected = notice("1000000000000002", "2222", "OMITTED", REJECTED_AT)
            .replace("<event:year>2026</event:year>", "");
        if case == "expired" {
            rejected = rejected.replace("209907241405", "202607231409");
        }
        quarantine(&store, &rejected, REJECTED_AT, NmsNotamClassification::Fdc)?;
        let accepted = match case {
            "cancel-self" => Some(
                notice("1000000000000002", "2222", "CANCELED", LATER_AT).replace(
                    "<fnse:lastUpdated>",
                    "<fnse:canceled>2026-07-23T14:10:00Z</fnse:canceled><fnse:lastUpdated>",
                ),
            ),
            "cancel-reference" => Some(update_xml(
                UpdateIdentity {
                    nms_id: "1000000000000099",
                    classification: "FDC",
                    location: "FDC",
                    number: "2222",
                },
                Some("C"),
                "!FDC 6/2222 FDC CANCEL 6/2222 MHK",
                "FDC 6/2222 NOTAMC 6/2222 A) MHK",
                LATER_AT,
                None,
                "209907241405",
            )),
            "replacement" => Some(update_xml(
                UpdateIdentity {
                    nms_id: "1000000000000003",
                    classification: "FDC",
                    location: "MHK",
                    number: "3333",
                },
                Some("R"),
                "!FDC 6/3333 MHK NOTAMR 6/2222 TEST",
                "FDC 6/3333 NOTAMR 6/2222 A) MHK",
                LATER_AT,
                None,
                "209907241405",
            )),
            "newer" => Some(notice("1000000000000002", "2222", "NEWER", LATER_AT)),
            _ => None,
        };
        if case == "replacement" {
            assert!(
                parse_nms_api_update(accepted.as_ref().unwrap(), NmsNotamClassification::Fdc)?
                    .referenced_notam
                    .is_some()
            );
        }
        poll(&store, accepted.into_iter().collect(), LATER_AT)?;
        let before = store.canonical_source_snapshot()?;
        assert_eq!(
            store.recover_from_baseline(&baseline, timestamp(BASELINE_AT), timestamp(LATER_AT))?,
            1,
            "{case}"
        );
        assert_eq!(
            store.canonical_source_snapshot()?,
            before,
            "{case}: an obsolete rejected payload must not alter today's state"
        );
    }
    Ok(())
}

#[test]
fn rejected_cancellation_removes_stale_record_and_emits_removal() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let (store, baseline) = setup(temp.path())?;
    let rejected = notice("1000000000000001", "1111", "CANCELED", REJECTED_AT)
        .replace("<event:year>2026</event:year>", "")
        .replace(
            "<fnse:lastUpdated>",
            "<fnse:canceled>2026-07-23T14:05:00Z</fnse:canceled><fnse:lastUpdated>",
        );
    quarantine(&store, &rejected, REJECTED_AT, NmsNotamClassification::Fdc)?;
    let cursor = store.canonical_source_cursor()?;
    assert_eq!(
        store.recover_from_baseline(&baseline, timestamp(BASELINE_AT), timestamp(LATER_AT))?,
        1
    );
    assert!(store.current_records()?.is_empty());
    let batch = store.canonical_changes_after(&cursor)?.unwrap();
    assert_eq!(batch.changes.len(), 1);
    assert!(
        matches!(&batch.changes[0].mutation, CanonicalNotamSourceMutation::Remove { notam_id } if notam_id == &baseline[0].id)
    );
    Ok(())
}

#[test]
fn corrupt_history_or_wrong_baseline_aborts_without_partial_repair() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let (store, baseline) = setup(temp.path())?;
    let rejected = notice("1000000000000002", "2222", "OMITTED", REJECTED_AT)
        .replace("<event:year>2026</event:year>", "");
    quarantine(&store, &rejected, REJECTED_AT, NmsNotamClassification::Fdc)?;
    poll(
        &store,
        vec![notice("1000000000000003", "3333", "ACCEPTED", LATER_AT)],
        LATER_AT,
    )?;
    let before = store.canonical_source_snapshot()?;
    assert!(store
        .recover_from_baseline(&[], timestamp(BASELINE_AT), timestamp(LATER_AT))
        .unwrap_err()
        .to_string()
        .contains("baseline"));
    store
        .open_connection()?
        .execute("UPDATE raw_updates SET raw_aixm='damaged'", [])?;
    assert!(store
        .recover_from_baseline(&baseline, timestamp(BASELINE_AT), timestamp(LATER_AT))
        .unwrap_err()
        .to_string()
        .contains("checksum"));
    assert_eq!(store.canonical_source_snapshot()?, before);
    assert_eq!(
        store
            .open_connection()?
            .query_row("SELECT COUNT(*) FROM raw_updates", [], |row| row
                .get::<_, i64>(0))?,
        1
    );
    Ok(())
}

#[test]
fn new_baseline_supersedes_old_quarantine_and_unknown_identity_remains_rejected(
) -> anyhow::Result<()> {
    let temp = tempdir()?;
    let (store, baseline) = setup(temp.path())?;
    let rejected = notice("1000000000000002", "2222", "OMITTED", REJECTED_AT)
        .replace("<event:year>2026</event:year>", "");
    quarantine(&store, &rejected, REJECTED_AT, NmsNotamClassification::Fdc)?;
    store.install_baseline(
        "fixture",
        None,
        timestamp(LATER_AT),
        Path::new("/fixture/new-baseline"),
        &baseline,
    )?;
    let invalid = rejected.replace("<event:issued>2026-07-23T14:00:00Z</event:issued>", "");
    quarantine(
        &store,
        &invalid,
        "2026-07-23T14:12:00Z",
        NmsNotamClassification::Fdc,
    )?;
    assert_eq!(
        store.recover_from_baseline(
            &baseline,
            timestamp(LATER_AT),
            timestamp("2026-07-23T14:15:00Z")
        )?,
        0
    );
    assert_eq!(store.current_records()?, baseline);
    Ok(())
}

#[test]
fn repair_refuses_live_collector_and_missing_state() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let (store, _) = setup(temp.path())?;
    let _lock = store.acquire_lock()?;
    assert!(store.repair_rejected_updates().is_err());
    assert!(NmsApiCollectorStore::new(temp.path().join("missing"))
        .repair_rejected_updates()
        .is_err());
    Ok(())
}

#[test]
fn failed_commit_rolls_back_current_records_journal_and_repair_markers() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let (store, baseline) = setup(temp.path())?;
    let rejected = notice("1000000000000002", "2222", "OMITTED", REJECTED_AT)
        .replace("<event:year>2026</event:year>", "");
    quarantine(&store, &rejected, REJECTED_AT, NmsNotamClassification::Fdc)?;
    store.open_connection()?.execute_batch(
        "CREATE TRIGGER fail_repair BEFORE INSERT ON raw_updates
         BEGIN SELECT RAISE(ABORT, 'injected repair failure'); END;",
    )?;
    let before = store.canonical_source_snapshot()?;
    assert!(store
        .recover_from_baseline(&baseline, timestamp(BASELINE_AT), timestamp(LATER_AT))
        .is_err());
    assert_eq!(store.canonical_source_snapshot()?, before);
    assert_eq!(
        store
            .open_connection()?
            .query_row("SELECT COUNT(*) FROM raw_updates", [], |row| row
                .get::<_, i64>(0))?,
        0
    );
    store
        .open_connection()?
        .execute_batch("DROP TRIGGER fail_repair;")?;
    assert_eq!(
        store.recover_from_baseline(&baseline, timestamp(BASELINE_AT), timestamp(LATER_AT))?,
        1
    );
    Ok(())
}

#[test]
fn missing_accepted_history_cannot_delete_unexplained_current_records() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let (store, baseline) = setup(temp.path())?;
    let rejected = notice("1000000000000002", "2222", "OMITTED", REJECTED_AT)
        .replace("<event:year>2026</event:year>", "");
    quarantine(&store, &rejected, REJECTED_AT, NmsNotamClassification::Fdc)?;
    poll(
        &store,
        vec![notice("1000000000000003", "3333", "ACCEPTED", LATER_AT)],
        LATER_AT,
    )?;
    store
        .open_connection()?
        .execute("DELETE FROM raw_updates", [])?;
    let before = store.canonical_source_snapshot()?;
    assert!(store
        .recover_from_baseline(&baseline, timestamp(BASELINE_AT), timestamp(LATER_AT))
        .unwrap_err()
        .to_string()
        .contains("does not reproduce current state"));
    assert_eq!(store.canonical_source_snapshot()?, before);
    Ok(())
}

#[test]
fn empty_poll_expiry_prevents_ambiguous_later_cancellation() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let (store, baseline) = setup(temp.path())?;
    // Two IDs with one human identity are briefly present. An empty poll
    // expires the first before the second appears and is then canceled.
    let rejected = notice("1000000000000002", "2222", "EXPIRES", REJECTED_AT)
        .replace("<event:year>2026</event:year>", "")
        .replace("209907241405", "202607231409");
    quarantine(&store, &rejected, REJECTED_AT, NmsNotamClassification::Fdc)?;
    poll(&store, vec![], LATER_AT)?;
    let replacement = notice("1000000000000003", "2222", "NEW ID", "2026-07-23T14:12:00Z");
    let cancellation = update_xml(
        UpdateIdentity {
            nms_id: "1000000000000099",
            classification: "FDC",
            location: "FDC",
            number: "2222",
        },
        Some("C"),
        "!FDC 6/2222 FDC CANCEL 6/2222 MHK",
        "FDC 6/2222 NOTAMC 6/2222 A) MHK",
        "2026-07-23T14:13:00Z",
        None,
        "209907241405",
    );
    poll(
        &store,
        vec![replacement, cancellation],
        "2026-07-23T14:15:00Z",
    )?;
    let before = store.canonical_source_snapshot()?;
    assert_eq!(
        store.recover_from_baseline(
            &baseline,
            timestamp(BASELINE_AT),
            timestamp("2026-07-23T14:16:00Z")
        )?,
        1
    );
    assert_eq!(store.canonical_source_snapshot()?, before);
    Ok(())
}

#[test]
fn still_invalid_quarantine_does_not_require_loading_baseline() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let (store, _) = setup(temp.path())?;
    quarantine(
        &store,
        "<not-valid-xml>",
        REJECTED_AT,
        NmsNotamClassification::Fdc,
    )?;
    assert_eq!(store.repair_rejected_updates()?, 0);
    Ok(())
}

#[test]
fn collector_repairs_retained_capture_before_announcing_state_ready() -> anyhow::Result<()> {
    struct NoNetwork;
    impl crate::InitialLoadSource for NoNetwork {
        fn capture_source(&self) -> InitialLoadCaptureSource {
            InitialLoadCaptureSource {
                environment: "fixture".into(),
                api_base_url: None,
            }
        }
        fn fetch_classification(
            &mut self,
            _: NmsNotamClassification,
            _: &Path,
        ) -> anyhow::Result<()> {
            bail!("unexpected network request during recovery")
        }
    }
    impl NmsApiSource for NoNetwork {
        fn fetch_updates(
            &mut self,
            _: NmsNotamClassification,
            _: DateTime<Utc>,
        ) -> anyhow::Result<Vec<String>> {
            bail!("unexpected network request during recovery")
        }
    }

    let temp = tempdir()?;
    let (store, baseline) = setup(temp.path())?;
    let capture = temp.path().join("captures/retained");
    fs::create_dir_all(&capture)?;
    let records = serde_json::to_vec(&baseline)?;
    fs::write(capture.join("normalized.json"), &records)?;
    let file = serde_json::json!({"path":"normalized.json", "bytes":records.len(), "sha256":sha256_hex(&records)});
    fs::write(
        capture.join("manifest.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema_version":1, "captured_at_utc":BASELINE_AT,
            "source":{"environment":"fixture", "api_base_url":null},
            "classifications":[{"classification":"FDC", "feature_collection_timestamp":null,
                "declared_record_count":1, "parsed_message_count":1, "canonical_record_count":1,
                "gzip":file, "xml":file, "normalized_records":file}]
        }))?,
    )?;
    store.install_baseline("fixture", None, timestamp(BASELINE_AT), &capture, &baseline)?;
    let rejected = notice("1000000000000002", "2222", "OMITTED", REJECTED_AT)
        .replace("<event:year>2026</event:year>", "");
    quarantine(&store, &rejected, REJECTED_AT, NmsNotamClassification::Fdc)?;
    let cursor = store.canonical_source_cursor()?;
    let mut saw_ready = false;
    run_collector_with_observer(
        &store,
        &mut NoNetwork,
        &CollectorOptions {
            max_polls: Some(0),
            ..CollectorOptions::default()
        },
        |event| {
            if let NmsCollectorEvent::StateReady {
                current_records, ..
            } = event
            {
                assert_eq!(*current_records, 2);
                saw_ready = true;
            }
            Ok(())
        },
    )?;
    assert!(saw_ready);
    assert_eq!(
        store
            .canonical_changes_after(&cursor)?
            .unwrap()
            .changes
            .len(),
        1
    );
    assert_eq!(store.repair_rejected_updates()?, 0);
    Ok(())
}
