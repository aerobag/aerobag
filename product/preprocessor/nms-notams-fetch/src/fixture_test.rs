// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{bail, Context};
use app_core::{
    decode_prepared_live_feed, prepare_notam_live_feed_delta_resource_with_work,
    prepare_notam_live_feed_state_resource_with_work, BackgroundNotamWork, NotamDisplayIndex,
    NotamProjectionPreparer, PreparedLiveFeedPayload, PreparedNotamPayload,
};
use chrono::{DateTime, Utc};
use flate2::read::GzDecoder;
use notam_state::{NotamApplyWork, NotamCheckpoint, NotamDelta, NotamMutation, NotamState};
use preprocessor_live_feeds::engine::{
    collapse_notam_transitions, write_immutable_xz_json_pretty_file,
};
use preprocessor_live_feeds::nms_initial_load::{parse_nms_initial_load, NmsNotamClassification};
use preprocessor_live_feeds::notam_store::{
    NotamPersistentStore, NotamPublicationCursor, NotamPublicationSnapshot,
    NotamPublicationTransition,
};
use serde::Deserialize;
use tempfile::tempdir;

use crate::collector::NmsApiCollectorStore;
use crate::fixture::{load_nms_fixture, LoadedNmsFixture};

struct PreparedNotamClient {
    preparer: NotamProjectionPreparer,
    index: NotamDisplayIndex,
}

#[derive(Debug, Deserialize)]
struct TraceExpectations {
    schema_version: u32,
    baseline_record_count: usize,
    poll_count: usize,
    update_count: usize,
    transition_count: usize,
    mutation_count: usize,
    removal_count: usize,
    repeated_mutation_id_count: usize,
    final_state_id: String,
}

#[derive(Clone)]
struct Boundary {
    state_id: String,
    counters: notam_state::NotamCounters,
}

#[test]
#[ignore = "requires the external NMS NOTAM trace"]
fn captured_nms_trace_converges_across_checkpoint_and_catchup_schedules() -> anyhow::Result<()> {
    let fixture = load_trace_fixture()?;
    let temp = tempdir()?;
    let collector = NmsApiCollectorStore::new(temp.path().join("collector"));
    collector.initialize()?;
    let mut baseline_records = Vec::new();
    for initial in &fixture.manifest.initial_load {
        let path = fixture.root.join(&initial.file.path);
        let reader = BufReader::new(GzDecoder::new(
            File::open(&path).with_context(|| format!("failed to open {}", path.display()))?,
        ));
        let parsed = parse_nms_initial_load(reader, initial.classification)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        parsed.validate_complete()?;
        assert_eq!(
            parsed.feature_collection_timestamp,
            initial.feature_collection_timestamp
        );
        assert_eq!(parsed.declared_record_count, initial.declared_record_count);
        assert_eq!(parsed.parsed_message_count, initial.parsed_message_count);
        assert_eq!(parsed.records.len(), initial.canonical_record_count);
        baseline_records.extend(parsed.records);
    }
    let initial_load_captured_at = parse_timestamp(
        &fixture.manifest.initial_load_captured_at_utc,
        "fixture Initial Load capture",
    )?;
    collector.install_baseline(
        &fixture.manifest.source_environment,
        None,
        initial_load_captured_at,
        Path::new("nms-fixture-initial-load"),
        &baseline_records,
    )?;

    let reference_store = NotamPersistentStore::new(temp.path().join("notam-store-reference"));
    reference_store.initialize()?;
    let incremental_store = NotamPersistentStore::new(temp.path().join("notam-store-incremental"));
    incremental_store.initialize()?;
    let source_snapshot = collector.canonical_source_snapshot()?;
    let reference_baseline = reference_store.synchronize_current_records(
        &source_snapshot.records,
        &fixture.manifest.initial_load_captured_at_utc,
    )?;
    let incremental_baseline = incremental_store.synchronize_canonical_source_snapshot(
        &source_snapshot.records,
        &fixture.manifest.initial_load_captured_at_utc,
        &source_snapshot.cursor,
    )?;
    assert_eq!(reference_baseline, incremental_baseline);
    let baseline_snapshot = reference_store.publication_snapshot()?;
    let incremental_baseline_snapshot = incremental_store.publication_snapshot()?;
    assert_equivalent_publication(&baseline_snapshot, &incremental_baseline_snapshot);
    if baseline_snapshot.transitions.len() != 1 {
        bail!(
            "NMS fixture baseline produced {} publication transitions; expected one",
            baseline_snapshot.transitions.len()
        );
    }
    let baseline_transition = &baseline_snapshot.transitions[0];
    reference_store.advance_publication_cursor(
        baseline_transition.journal_seq,
        None,
        &baseline_transition.to_state_id,
    )?;
    let incremental_baseline_transition = &incremental_baseline_snapshot.transitions[0];
    incremental_store.advance_publication_cursor(
        incremental_baseline_transition.journal_seq,
        None,
        &incremental_baseline_transition.to_state_id,
    )?;
    let initial_checkpoint = reference_store.current_checkpoint()?;
    assert_eq!(incremental_store.current_checkpoint()?, initial_checkpoint);
    let mut reference_state_id = reference_baseline.state_id;

    for poll in &fixture.polls {
        let mut domestic = Vec::new();
        let mut fdc = Vec::new();
        for update in &poll.updates {
            match update.classification {
                NmsNotamClassification::Domestic => domestic.push(update.raw_aixm.clone()),
                NmsNotamClassification::Fdc => fdc.push(update.raw_aixm.clone()),
            }
        }
        if domestic.len() > poll.source_domestic_received || fdc.len() > poll.source_fdc_received {
            bail!("NMS fixture poll has more unique updates than source messages");
        }
        let summary = collector.apply_poll_at(
            parse_timestamp(&poll.started_at_utc, "fixture poll start")?,
            parse_timestamp(&poll.query_since_utc, "fixture poll query cursor")?,
            parse_timestamp(&poll.completed_at_utc, "fixture poll completion")?,
            domestic,
            fdc,
        )?;
        if summary.new_payloads != poll.updates.len() || summary.duplicate_payloads != 0 {
            bail!("NMS fixture poll did not preserve unique payload identity");
        }
        let reference_summary = if summary.upserted + summary.removed + summary.expired > 0 {
            let synchronized = reference_store.synchronize_current_records(
                &collector.current_records()?,
                &poll.completed_at_utc,
            )?;
            reference_state_id.clone_from(&synchronized.state_id);
            Some(synchronized)
        } else {
            None
        };
        let incremental_cursor = incremental_store
            .canonical_source_cursor()?
            .context("incremental fixture store lost its NMS source cursor")?;
        let batch = collector
            .canonical_changes_after(&incremental_cursor)?
            .with_context(|| {
                format!(
                    "NMS fixture journal cannot continue from {}:{} after poll {}",
                    incremental_cursor.epoch,
                    incremental_cursor.through_sequence,
                    poll.started_at_utc
                )
            })?;
        let acknowledged_cursor =
            preprocessor_live_feeds::notam_store::CanonicalNotamSourceCursor {
                epoch: batch.epoch.clone(),
                through_sequence: batch.through_sequence,
            };
        let incremental_summary =
            incremental_store.apply_canonical_source_batch(&batch, &poll.completed_at_utc)?;
        if let Some(reference_summary) = reference_summary {
            assert_eq!(incremental_summary, reference_summary);
        } else {
            assert_eq!(incremental_summary.state_id, reference_state_id);
            assert_eq!(incremental_summary.changed_count, 0);
            assert_eq!(incremental_summary.removed_count, 0);
        }
        collector.prune_canonical_changes_through(&acknowledged_cursor)?;
    }

    let snapshot = reference_store.publication_snapshot()?;
    let incremental_snapshot = incremental_store.publication_snapshot()?;
    assert_equivalent_publication(&snapshot, &incremental_snapshot);
    let transitions = snapshot.transitions;
    let final_checkpoint = reference_store.current_checkpoint()?;
    assert_eq!(incremental_store.current_checkpoint()?, final_checkpoint);
    validate_trace_chain(&initial_checkpoint, &transitions, &final_checkpoint)?;
    let mutation_count = transitions
        .iter()
        .map(|transition| transition.mutations.len())
        .sum::<usize>();
    let removal_count = transitions
        .iter()
        .flat_map(|transition| &transition.mutations)
        .filter(|mutation| matches!(mutation, NotamMutation::Remove { .. }))
        .count();
    let repeated_id_count = repeated_mutation_id_count(&transitions);
    eprintln!(
        "NMS NOTAM fixture: baseline_records={} polls={} updates={} transitions={} mutations={} removals={} repeated_ids={} final_state={}",
        baseline_records.len(),
        fixture.polls.len(),
        fixture
            .polls
            .iter()
            .map(|poll| poll.updates.len())
            .sum::<usize>(),
        transitions.len(),
        mutation_count,
        removal_count,
        repeated_id_count,
        final_checkpoint.state_id
    );
    let expected = load_trace_expectations(&fixture.root)?;
    assert_eq!(baseline_records.len(), expected.baseline_record_count);
    assert_eq!(fixture.polls.len(), expected.poll_count);
    assert_eq!(
        fixture
            .polls
            .iter()
            .map(|poll| poll.updates.len())
            .sum::<usize>(),
        expected.update_count
    );
    assert_eq!(transitions.len(), expected.transition_count);
    assert_eq!(mutation_count, expected.mutation_count);
    assert_eq!(removal_count, expected.removal_count);
    assert_eq!(repeated_id_count, expected.repeated_mutation_id_count);
    assert_eq!(final_checkpoint.state_id, expected.final_state_id);
    assert!(
        transitions.len() >= 10,
        "fixture has only {} logical transitions",
        transitions.len()
    );
    assert!(
        mutation_count >= 100,
        "fixture has only {mutation_count} logical mutations"
    );
    assert!(removal_count > 0, "fixture contains no removals");
    assert!(
        repeated_id_count > 0,
        "fixture contains no repeatedly mutated NOTAM IDs"
    );

    let boundaries = trace_boundaries(&initial_checkpoint, &transitions);
    let checkpoint_boundaries = checkpoint_boundaries(&transitions);
    let checkpoint_artifacts = build_checkpoint_artifacts(
        temp.path(),
        &initial_checkpoint,
        &transitions,
        &checkpoint_boundaries,
    )?;
    let mut delta_artifacts = HashMap::new();
    let mut path_count = 0_usize;

    for &checkpoint_boundary in &checkpoint_boundaries {
        for max_span in [1_usize, 7, 31, 100, usize::MAX] {
            let mut client = install_checkpoint_artifact(
                checkpoint_boundary,
                &boundaries,
                &checkpoint_artifacts,
            )?;
            apply_transition_range(TransitionRangeInput {
                client: &mut client,
                transitions: &transitions,
                boundaries: &boundaries,
                start: checkpoint_boundary,
                end: transitions.len(),
                max_span,
                output_root: temp.path(),
                artifacts: &mut delta_artifacts,
            })?;
            assert_exact_final_state(&client, &final_checkpoint);
            path_count += 1;
        }
    }

    let usable_checkpoints = checkpoint_boundaries
        .iter()
        .copied()
        .filter(|boundary| *boundary < transitions.len())
        .collect::<Vec<_>>();
    let mut seed = 0xc0de_51a7_2026_0723_u64;
    for schedule in 0..64 {
        seed = next_seed(seed);
        let checkpoint_boundary = usable_checkpoints[(seed as usize) % usable_checkpoints.len()];
        seed = next_seed(seed);
        let remaining = transitions.len() - checkpoint_boundary;
        let disconnect_boundary =
            checkpoint_boundary + (seed as usize % remaining.saturating_add(1));
        let mut client =
            install_checkpoint_artifact(checkpoint_boundary, &boundaries, &checkpoint_artifacts)?;
        let online_span = [1_usize, 7, 31][schedule % 3];
        apply_transition_range(TransitionRangeInput {
            client: &mut client,
            transitions: &transitions,
            boundaries: &boundaries,
            start: checkpoint_boundary,
            end: disconnect_boundary,
            max_span: online_span,
            output_root: temp.path(),
            artifacts: &mut delta_artifacts,
        })?;

        let mut catchup_start = disconnect_boundary;
        if schedule % 3 == 0 {
            if let Some(new_checkpoint) = checkpoint_boundaries
                .iter()
                .copied()
                .rfind(|boundary| *boundary > disconnect_boundary && *boundary <= transitions.len())
            {
                client = install_checkpoint_artifact(
                    new_checkpoint,
                    &boundaries,
                    &checkpoint_artifacts,
                )?;
                catchup_start = new_checkpoint;
            }
        }
        let catchup_span = [1_usize, 7, 31, 100, usize::MAX][schedule % 5];
        apply_transition_range(TransitionRangeInput {
            client: &mut client,
            transitions: &transitions,
            boundaries: &boundaries,
            start: catchup_start,
            end: transitions.len(),
            max_span: catchup_span,
            output_root: temp.path(),
            artifacts: &mut delta_artifacts,
        })?;
        assert_exact_final_state(&client, &final_checkpoint);
        path_count += 1;
    }

    eprintln!(
        "NMS NOTAM recovery: transitions={} mutations={} removals={} repeated_ids={} checkpoints={} delta_spans={} client_paths={} final_state={}",
        transitions.len(),
        mutation_count,
        removal_count,
        repeated_id_count,
        checkpoint_artifacts.len(),
        delta_artifacts.len(),
        path_count,
        final_checkpoint.state_id
    );
    Ok(())
}

fn assert_equivalent_publication(
    reference: &NotamPublicationSnapshot,
    incremental: &NotamPublicationSnapshot,
) {
    // Full reconciliation numbers source rounds; the journal numbers individual
    // source mutations. Those ranges are producer diagnostics, not published
    // client state, so validate each independently and compare every other field.
    assert_eq!(incremental.current_state_id, reference.current_state_id);
    assert_eq!(incremental.counters, reference.counters);
    assert_eq!(incremental.cursor, reference.cursor);
    assert_eq!(incremental.transitions.len(), reference.transitions.len());
    for (index, (reference, incremental)) in reference
        .transitions
        .iter()
        .zip(&incremental.transitions)
        .enumerate()
    {
        assert_eq!(
            incremental.journal_seq, reference.journal_seq,
            "publication journal sequence diverged at transition {index}"
        );
        assert_eq!(
            incremental.observed_at_utc, reference.observed_at_utc,
            "observation time diverged at transition {index}"
        );
        assert_eq!(
            incremental.from_state_id, reference.from_state_id,
            "source state diverged at transition {index}"
        );
        assert_eq!(
            incremental.to_state_id, reference.to_state_id,
            "destination state diverged at transition {index}"
        );
        assert_eq!(
            incremental.counters, reference.counters,
            "counters diverged at transition {index}"
        );
        assert_eq!(
            incremental.mutations, reference.mutations,
            "mutations diverged at transition {index}"
        );
    }
    for (label, snapshot) in [("reference", reference), ("incremental", incremental)] {
        let mut previous_last = None;
        for transition in &snapshot.transitions {
            assert!(
                transition.source_first_ingest_seq <= transition.source_last_ingest_seq,
                "{label} publication transition {} has a reversed source range",
                transition.journal_seq
            );
            if let Some(previous_last) = previous_last {
                assert!(
                    transition.source_first_ingest_seq > previous_last,
                    "{label} publication transition {} overlaps its predecessor",
                    transition.journal_seq
                );
            }
            previous_last = Some(transition.source_last_ingest_seq);
        }
    }
}

fn load_trace_fixture() -> anyhow::Result<LoadedNmsFixture> {
    let test_artifacts_root = std::env::var_os("AEROBAG_TEST_ARTIFACTS_ROOT")
        .or_else(|| std::env::var_os("AEROBAG_TEST_ARTIFACTS"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("..")
                .join("..")
                .join("aerobag-test-artifacts")
        });
    let fixture_root = test_artifacts_root.join("notams").join("nms-api-trace");
    let manifest_path = fixture_root.join("capture_manifest.json");
    if !manifest_path.is_file() {
        bail!(
            "NMS NOTAM fixture is missing {}; set AEROBAG_TEST_ARTIFACTS_ROOT",
            manifest_path.display()
        );
    }
    load_nms_fixture(&fixture_root)
}

fn load_trace_expectations(fixture_root: &Path) -> anyhow::Result<TraceExpectations> {
    let expected_path = fixture_root.join("expected.json");
    let expected: TraceExpectations = serde_json::from_slice(
        &fs::read(&expected_path)
            .with_context(|| format!("failed to read {}", expected_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", expected_path.display()))?;
    if expected.schema_version != 1 {
        bail!(
            "unsupported NOTAM trace expectation schema {}; expected 1",
            expected.schema_version
        );
    }
    Ok(expected)
}

fn parse_timestamp(value: &str, label: &str) -> anyhow::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("{label} is not RFC3339: {value}"))
        .map(|value| value.with_timezone(&Utc))
}

fn validate_trace_chain(
    initial: &NotamCheckpoint,
    transitions: &[NotamPublicationTransition],
    final_checkpoint: &NotamCheckpoint,
) -> anyhow::Result<()> {
    let mut head = initial.state_id.as_str();
    for transition in transitions {
        if transition.from_state_id != head {
            bail!(
                "NOTAM trace transition {} starts at {}, expected {head}",
                transition.journal_seq,
                transition.from_state_id
            );
        }
        head = &transition.to_state_id;
    }
    if head != final_checkpoint.state_id {
        bail!(
            "NOTAM trace ends at {head}, projection ends at {}",
            final_checkpoint.state_id
        );
    }
    Ok(())
}

fn repeated_mutation_id_count(transitions: &[NotamPublicationTransition]) -> usize {
    let mut counts = BTreeMap::<&str, usize>::new();
    for mutation in transitions
        .iter()
        .flat_map(|transition| &transition.mutations)
    {
        *counts.entry(mutation.notam_id()).or_default() += 1;
    }
    counts.values().filter(|count| **count > 1).count()
}

fn trace_boundaries(
    initial: &NotamCheckpoint,
    transitions: &[NotamPublicationTransition],
) -> Vec<Boundary> {
    let mut boundaries = Vec::with_capacity(transitions.len() + 1);
    boundaries.push(Boundary {
        state_id: initial.state_id.clone(),
        counters: initial.counters,
    });
    boundaries.extend(transitions.iter().map(|transition| Boundary {
        state_id: transition.to_state_id.clone(),
        counters: transition.counters,
    }));
    boundaries
}

fn checkpoint_boundaries(transitions: &[NotamPublicationTransition]) -> Vec<usize> {
    let mut boundaries = BTreeSet::from([0, transitions.len()]);
    for numerator in 1..8 {
        boundaries.insert(transitions.len() * numerator / 8);
    }
    let total_mutations = transitions
        .iter()
        .map(|transition| transition.mutations.len())
        .sum::<usize>();
    let suffix_target = total_mutations.saturating_sub(80);
    let mut cumulative = 0_usize;
    for (index, transition) in transitions.iter().enumerate() {
        cumulative += transition.mutations.len();
        if cumulative >= suffix_target {
            boundaries.insert(index + 1);
            break;
        }
    }
    boundaries.into_iter().collect()
}

fn build_checkpoint_artifacts(
    output_root: &Path,
    initial: &NotamCheckpoint,
    transitions: &[NotamPublicationTransition],
    checkpoint_boundaries: &[usize],
) -> anyhow::Result<HashMap<usize, Arc<Vec<u8>>>> {
    let requested = checkpoint_boundaries
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut artifacts = HashMap::new();
    let mut state = NotamState::from_checkpoint(initial.clone(), &mut NotamApplyWork::default())
        .map_err(anyhow::Error::msg)?;
    for boundary in 0..=transitions.len() {
        if requested.contains(&boundary) {
            let checkpoint = state.checkpoint();
            let path = output_root
                .join("checkpoint-artifacts")
                .join(format!("{}.json.xz", checkpoint.state_id));
            artifacts.insert(
                boundary,
                Arc::new(write_immutable_xz_json_pretty_file(&path, &checkpoint)?),
            );
        }
        if let Some(transition) = transitions.get(boundary) {
            state
                .apply_delta(
                    NotamDelta::new(
                        transition.from_state_id.clone(),
                        transition.to_state_id.clone(),
                        transition.counters,
                        transition.mutations.clone(),
                    ),
                    &mut NotamApplyWork::default(),
                )
                .map_err(anyhow::Error::msg)?;
        }
    }
    Ok(artifacts)
}

fn install_checkpoint_artifact(
    boundary: usize,
    boundaries: &[Boundary],
    artifacts: &HashMap<usize, Arc<Vec<u8>>>,
) -> anyhow::Result<PreparedNotamClient> {
    let expected = &boundaries[boundary];
    let bytes = artifacts
        .get(&boundary)
        .with_context(|| format!("missing checkpoint artifact at boundary {boundary}"))?;
    let resource_id = format!("live_feeds/state/notams/{}", expected.state_id);
    let mut background_work = BackgroundNotamWork::default();
    let mut preparer = NotamProjectionPreparer::default();
    let postcard = prepare_notam_live_feed_state_resource_with_work(
        &resource_id,
        bytes,
        &mut preparer,
        &mut background_work,
        &mut NotamApplyWork::default(),
    )?;
    let envelope = decode_prepared_live_feed(&postcard)?;
    let PreparedLiveFeedPayload::Notams(PreparedNotamPayload::InstallDisplayCheckpoint(checkpoint)) =
        envelope.payload
    else {
        bail!("prepared NOTAM checkpoint used the wrong payload kind");
    };
    if background_work.records_decoded != expected.counters.notam_count {
        bail!("background checkpoint work did not count exact decoded records");
    }
    let index =
        NotamDisplayIndex::from_projection_checkpoint(checkpoint).map_err(anyhow::Error::msg)?;
    if index.state_id() != expected.state_id
        || preparer
            .canonical_checkpoint()
            .is_none_or(|checkpoint| checkpoint.counters != expected.counters)
    {
        bail!("client checkpoint install diverged at boundary {boundary}");
    }
    Ok(PreparedNotamClient { preparer, index })
}

struct TransitionRangeInput<'a> {
    client: &'a mut PreparedNotamClient,
    transitions: &'a [NotamPublicationTransition],
    boundaries: &'a [Boundary],
    start: usize,
    end: usize,
    max_span: usize,
    output_root: &'a Path,
    artifacts: &'a mut HashMap<(usize, usize), Arc<Vec<u8>>>,
}

fn apply_transition_range(input: TransitionRangeInput<'_>) -> anyhow::Result<()> {
    let TransitionRangeInput {
        client,
        transitions,
        boundaries,
        start,
        end,
        max_span,
        output_root,
        artifacts,
    } = input;
    let mut cursor = start;
    while cursor < end {
        let next = if max_span == usize::MAX {
            end
        } else {
            (cursor + max_span).min(end)
        };
        let bytes = if let Some(bytes) = artifacts.get(&(cursor, next)) {
            Arc::clone(bytes)
        } else {
            let snapshot = NotamPublicationSnapshot {
                current_state_id: boundaries[next].state_id.clone(),
                counters: boundaries[next].counters,
                procedure_notams_without_ui_anchor: 0,
                source_records_without_location: 0,
                source_record_count: boundaries[next].counters.notam_count,
                server_only_records_by_keyword: BTreeMap::new(),
                cursor: NotamPublicationCursor {
                    published_through_journal_seq: transitions[cursor].journal_seq - 1,
                    published_head_state_id: Some(boundaries[cursor].state_id.clone()),
                },
                transitions: transitions[cursor..next].to_vec(),
            };
            let delta = collapse_notam_transitions(&snapshot.cursor, &snapshot.transitions)?;
            let path = output_root.join("delta-artifacts").join(format!(
                "{}__{}.json.xz",
                delta.from_state_id, delta.to_state_id
            ));
            let bytes = Arc::new(write_immutable_xz_json_pretty_file(&path, &delta)?);
            artifacts.insert((cursor, next), Arc::clone(&bytes));
            bytes
        };

        let resource_id = format!(
            "live_feeds/delta/notams/{}/{}",
            boundaries[cursor].state_id, boundaries[next].state_id
        );
        let mut background_work = BackgroundNotamWork::default();
        let postcard = prepare_notam_live_feed_delta_resource_with_work(
            &resource_id,
            &bytes,
            &mut client.preparer,
            &mut background_work,
            &mut NotamApplyWork::default(),
        )?;
        let envelope = decode_prepared_live_feed(&postcard)?;
        let PreparedLiveFeedPayload::Notams(PreparedNotamPayload::ApplyDisplayDelta(delta)) =
            envelope.payload
        else {
            bail!("prepared NOTAM delta used the wrong payload kind");
        };
        if background_work.records_decoded < delta.mutations.len() as u64 {
            bail!("projected delta contains more mutations than the canonical delta");
        }
        client
            .index
            .apply_projection_delta(delta)
            .map_err(anyhow::Error::msg)?;
        if client.index.state_id() != boundaries[next].state_id
            || client
                .preparer
                .canonical_checkpoint()
                .is_none_or(|checkpoint| checkpoint.counters != boundaries[next].counters)
        {
            bail!("client diverged after catch-up span {cursor}..{next}");
        }
        cursor = next;
    }
    Ok(())
}

fn assert_exact_final_state(client: &PreparedNotamClient, expected: &NotamCheckpoint) {
    assert_eq!(client.index.state_id(), expected.state_id);
    assert_eq!(
        client.preparer.canonical_checkpoint().as_ref(),
        Some(expected)
    );
}

fn next_seed(seed: u64) -> u64 {
    seed.wrapping_mul(6364136223846793005).wrapping_add(1)
}
