// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tiny, clock-controlled counterpart to the external NOTAM publication replay.

use std::path::Path;

use chrono::{DateTime, Utc};
use notam_state::{NotamApplyWork, NotamMutation, NotamState};
use preprocessor_live_feeds::{
    engine::collapse_notam_transitions,
    nms_initial_load::{parse_nms_api_update, NmsNotamClassification},
    notam_store::{CanonicalNotamSourceCursor, NotamPersistentStore, NotamPublicationCursor},
};
use tempfile::tempdir;

use crate::collector::NmsApiCollectorStore;

fn timestamp(minute: u32) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&format!("2026-07-23T14:{minute:02}:00Z"))
        .unwrap()
        .with_timezone(&Utc)
}

fn notice(number: u32, facility: &str, text: &str, expires: &str) -> String {
    format!(
        r#"<AIXMBasicMessage xmlns:gml="http://www.opengis.net/gml/3.2" gml:id="NMS_ID_{number:016}">
          <classification>DOM</classification>
          <lastUpdated>2026-07-23T14:00:00Z</lastUpdated>
          <icaoLocation>K{facility}</icaoLocation>
          <NOTAM>
            <location>{facility}</location><number>07/{number:03}</number>
            <year>2026</year><type>N</type>
            <issued>2026-07-23T14:00:00Z</issued>
            <effectiveStart>202607231400</effectiveStart>
            <effectiveEnd>{expires}</effectiveEnd><text>{text}</text>
          </NOTAM>
        </AIXMBasicMessage>"#
    )
}

#[test]
fn airport_airspace_projection_survives_expiry_duplicates_and_catchup() -> anyhow::Result<()> {
    check_projection_trace("AIRSPACE UAS WI 1NM OF AP")
}

#[test]
fn airport_navigation_projection_survives_expiry_duplicates_and_catchup() -> anyhow::Result<()> {
    check_projection_trace("NAV ILS RWY 03 U/S")
}

#[test]
fn airport_obstruction_projection_survives_expiry_duplicates_and_catchup() -> anyhow::Result<()> {
    check_projection_trace("OBST TOWER LGT U/S")
}

#[test]
fn airport_service_projection_survives_expiry_duplicates_and_catchup() -> anyhow::Result<()> {
    check_projection_trace("SVC ATIS U/S")
}

fn check_projection_trace(expiring_notice: &str) -> anyhow::Result<()> {
    let temp = tempdir()?;
    let collector = NmsApiCollectorStore::new(temp.path().join("collector"));
    collector.initialize()?;
    let records = [
        notice(1, "GTF", "RWY 03 CLSD", "202607231500"),
        notice(2, "GTF", expiring_notice, "202607231401"),
        notice(3, "ZAB", "AIRSPACE R5107H ACT", "202607231500"),
    ]
    .iter()
    .map(|xml| Ok(parse_nms_api_update(xml, NmsNotamClassification::Domestic)?.record))
    .collect::<anyhow::Result<Vec<_>>>()?;
    collector.install_baseline("test", None, timestamp(0), Path::new("test"), &records)?;
    let reference = NotamPersistentStore::new(temp.path().join("reference"));
    let incremental = NotamPersistentStore::new(temp.path().join("incremental"));
    reference.initialize()?;
    incremental.initialize()?;
    let baseline = collector.canonical_source_snapshot()?;
    reference.synchronize_current_records(&baseline.records, &timestamp(0).to_rfc3339())?;
    incremental.synchronize_canonical_source_snapshot(
        &baseline.records,
        &timestamp(0).to_rfc3339(),
        &baseline.cursor,
    )?;
    let initial = reference.current_checkpoint()?;
    assert_eq!(
        initial.records.len(),
        2,
        "RWY and {expiring_notice} must both reach the client"
    );
    assert_eq!(initial, incremental.current_checkpoint()?);
    assert!(initial
        .records
        .iter()
        .all(|record| record.airport_id.as_deref() == Some("KGTF")));
    for store in [&reference, &incremental] {
        let snapshot = store.publication_snapshot()?;
        let baseline_transition = &snapshot.transitions[0];
        store.advance_publication_cursor(
            baseline_transition.journal_seq,
            None,
            &baseline_transition.to_state_id,
        )?;
    }

    let service = notice(4, "GTF", "SVC ATIS U/S", "202607231403");
    let airspace = notice(5, "ABQ", "AIRSPACE UAS WI 1NM OF AP", "202607231500");
    let airspace_id = parse_nms_api_update(&airspace, NmsNotamClassification::Domestic)?
        .record
        .id;
    let cancellation = airspace.replace(
        "<lastUpdated>2026-07-23T14:00:00Z</lastUpdated>",
        "<lastUpdated>2026-07-23T14:05:00Z</lastUpdated><canceled>2026-07-23T14:05:00Z</canceled>",
    );
    // Only expiry at t=1; a service notice at t=2; its duplicate plus expiry at
    // t=3; an airport AIRSPACE notice at t=4 and cancellation at t=5; a
    // non-airport facility at t=6. No wall-clock waits or downloaded data.
    for (minute, updates, expected_transitions, expected_ids) in [
        (1, vec![], 1, vec![records[0].id.clone()]),
        (
            2,
            vec![service.clone()],
            2,
            vec![
                records[0].id.clone(),
                parse_nms_api_update(&service, NmsNotamClassification::Domestic)?
                    .record
                    .id,
            ],
        ),
        (3, vec![service], 3, vec![records[0].id.clone()]),
        (
            4,
            vec![airspace],
            4,
            vec![records[0].id.clone(), airspace_id.clone()],
        ),
        (5, vec![cancellation], 5, vec![records[0].id.clone()]),
        (
            6,
            vec![notice(
                6,
                "ZAB",
                "AIRSPACE R5107H ACT SFC-9000FT",
                "202607231500",
            )],
            5,
            vec![records[0].id.clone()],
        ),
    ] {
        let summary = collector.apply_poll_at(
            timestamp(minute),
            timestamp(0),
            timestamp(minute),
            updates,
            vec![],
        )?;
        assert_eq!(summary.rejected_payloads, 0, "poll {minute}: {summary:?}");
        if minute == 3 {
            assert_eq!(summary.duplicate_payloads, 1);
            assert_eq!(summary.expired, 1);
        }
        reference.synchronize_current_records(
            &collector.current_records()?,
            &timestamp(minute).to_rfc3339(),
        )?;
        let cursor = incremental.canonical_source_cursor()?.unwrap();
        let batch = collector.canonical_changes_after(&cursor)?.unwrap();
        incremental.apply_canonical_source_batch(&batch, &timestamp(minute).to_rfc3339())?;
        let acknowledged = CanonicalNotamSourceCursor {
            epoch: batch.epoch,
            through_sequence: batch.through_sequence,
        };
        assert_eq!(
            incremental.canonical_source_cursor()?.as_ref(),
            Some(&acknowledged)
        );
        collector.prune_canonical_changes_through(&acknowledged)?;
        let checkpoint = reference.current_checkpoint()?;
        assert_eq!(checkpoint, incremental.current_checkpoint()?);
        assert_eq!(
            checkpoint
                .records
                .iter()
                .map(|record| &record.id)
                .collect::<Vec<_>>(),
            expected_ids.iter().collect::<Vec<_>>()
        );
        let reference_snapshot = reference.publication_snapshot()?;
        let actual = incremental.publication_snapshot()?;
        assert_eq!(
            actual.transitions.len(),
            expected_transitions,
            "poll {minute}"
        );
        assert_eq!(actual.current_state_id, reference_snapshot.current_state_id);
        assert_eq!(
            actual.transitions.len(),
            reference_snapshot.transitions.len()
        );
        for (a, b) in actual
            .transitions
            .iter()
            .zip(&reference_snapshot.transitions)
        {
            assert_eq!(a.mutations, b.mutations);
            assert_eq!(a.to_state_id, b.to_state_id);
        }
    }
    let snapshot = incremental.publication_snapshot()?;
    assert!(
        matches!(&snapshot.transitions[0].mutations[..], [NotamMutation::Remove { notam_id }] if notam_id == &records[1].id)
    );
    assert!(
        matches!(&snapshot.transitions[1].mutations[..], [NotamMutation::Upsert { record }] if record.notam_keyword.as_deref() == Some("SVC"))
    );
    assert!(
        matches!(&snapshot.transitions[4].mutations[..], [NotamMutation::Remove { notam_id }] if notam_id == &airspace_id)
    );
    let final_checkpoint = incremental.current_checkpoint()?;
    for span in [1, snapshot.transitions.len()] {
        let mut client =
            NotamState::from_checkpoint(initial.clone(), &mut NotamApplyWork::default())?;
        for chunk in snapshot.transitions.chunks(span) {
            let delta = collapse_notam_transitions(
                &NotamPublicationCursor {
                    published_through_journal_seq: chunk[0].journal_seq - 1,
                    published_head_state_id: Some(chunk[0].from_state_id.clone()),
                },
                chunk,
            )?;
            client.apply_delta(delta, &mut NotamApplyWork::default())?;
        }
        assert_eq!(client.state_id(), final_checkpoint.state_id);
        assert_eq!(
            client
                .canonical_records()
                .map(|(_, record)| record.clone())
                .collect::<Vec<_>>(),
            final_checkpoint.records
        );
    }
    assert_eq!(
        collector.current_records()?.len(),
        3,
        "ARTCC records remain in source state"
    );
    Ok(())
}
