// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
};

use app_core::{
    decode_prepared_live_feed, prepare_live_feed_delta_resource_with_notam_work,
    prepare_live_feed_state_resource_with_notam_work, AirportNotamIndex, BackgroundNotamWork,
    PreparedLiveFeedPayload, PreparedNotamPayload,
};
use notam_state::{NotamApplyWork, NotamCheckpoint, NotamDelta, NotamState};
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;

use crate::notam_store::{
    NotamPersistentStore, NotamPublicationCursor, NotamPublicationSnapshot,
    NotamPublicationTransition,
};

#[derive(Debug, Deserialize)]
struct TraceManifest {
    schema_version: u32,
    start_ingest_seq: i64,
    end_ingest_seq: i64,
    starting_snapshot: TraceFile,
    segments: Vec<TraceSegment>,
}

#[derive(Debug, Deserialize)]
struct TraceFile {
    file: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Deserialize)]
struct TraceSegment {
    file: String,
    cursor_first: i64,
    cursor_last: i64,
    applied_message_count: u64,
    uncompressed_bytes: u64,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Deserialize)]
struct TraceExpectations {
    schema_version: u32,
    start_ingest_seq: i64,
    end_ingest_seq: i64,
    message_count: usize,
    transition_count: usize,
    mutation_count: usize,
    removal_count: usize,
    repeated_mutation_id_count: usize,
    final_state_id: String,
}

#[derive(Debug, Deserialize)]
struct CapturedRawMessage {
    ingest_seq: i64,
    dedupe_key: String,
    received_at_utc: String,
    message_json: String,
}

struct LoadedTrace {
    start_ingest_seq: i64,
    end_ingest_seq: i64,
    starting_records: Vec<(String, Option<String>, Option<String>, String, String)>,
    identity_cursors: Vec<(String, i64)>,
    messages: Vec<CapturedRawMessage>,
    expected: TraceExpectations,
}

#[derive(Clone)]
struct Boundary {
    state_id: String,
    counters: notam_state::NotamCounters,
}

#[test]
#[ignore = "requires the external incremental NOTAM trace"]
fn captured_notam_trace_converges_across_checkpoint_and_catchup_schedules() -> anyhow::Result<()> {
    let trace = load_trace_fixture()?;
    let temp = tempdir()?;
    let store = NotamPersistentStore::new(temp.path().join("notam-store"));
    store.bootstrap_incremental_trace_for_test(
        trace.start_ingest_seq,
        &trace.starting_records,
        &trace.identity_cursors,
    )?;
    let initial_checkpoint = store.current_checkpoint()?;

    for message in &trace.messages {
        store.insert_captured_raw_message_for_test(
            message.ingest_seq,
            &message.dedupe_key,
            &message.received_at_utc,
            &message.message_json,
        )?;
        let applied = store
            .apply_pending_raw_messages(1)?
            .with_context(|| format!("captured message {} was not applied", message.ingest_seq))?;
        if applied.max_ingest_seq != message.ingest_seq {
            bail!(
                "captured message {} advanced raw cursor to {}",
                message.ingest_seq,
                applied.max_ingest_seq
            );
        }
    }

    let snapshot = store.publication_snapshot()?;
    let transitions = snapshot.transitions;
    let final_checkpoint = store.current_checkpoint()?;
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
    assert_eq!(trace.start_ingest_seq, trace.expected.start_ingest_seq);
    assert_eq!(trace.end_ingest_seq, trace.expected.end_ingest_seq);
    assert_eq!(trace.messages.len(), trace.expected.message_count);
    assert_eq!(transitions.len(), trace.expected.transition_count);
    assert_eq!(mutation_count, trace.expected.mutation_count);
    assert_eq!(removal_count, trace.expected.removal_count);
    assert_eq!(repeated_id_count, trace.expected.repeated_mutation_id_count);
    assert_eq!(final_checkpoint.state_id, trace.expected.final_state_id);
    assert!(
        transitions.len() >= 100,
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
            apply_transition_range(
                &mut client,
                &transitions,
                &boundaries,
                checkpoint_boundary,
                transitions.len(),
                max_span,
                temp.path(),
                &mut delta_artifacts,
            )?;
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
        apply_transition_range(
            &mut client,
            &transitions,
            &boundaries,
            checkpoint_boundary,
            disconnect_boundary,
            online_span,
            temp.path(),
            &mut delta_artifacts,
        )?;

        let mut catchup_start = disconnect_boundary;
        if schedule % 3 == 0 {
            if let Some(new_checkpoint) = checkpoint_boundaries
                .iter()
                .copied()
                .filter(|boundary| {
                    *boundary > disconnect_boundary && *boundary <= transitions.len()
                })
                .last()
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
        apply_transition_range(
            &mut client,
            &transitions,
            &boundaries,
            catchup_start,
            transitions.len(),
            catchup_span,
            temp.path(),
            &mut delta_artifacts,
        )?;
        assert_exact_final_state(&client, &final_checkpoint);
        path_count += 1;
    }

    eprintln!(
        "NOTAM fixture {}..{}: messages={} transitions={} mutations={} removals={} repeated_ids={} checkpoints={} delta_spans={} client_paths={} final_state={}",
        trace.start_ingest_seq,
        trace.end_ingest_seq,
        trace.messages.len(),
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

fn load_trace_fixture() -> anyhow::Result<LoadedTrace> {
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
    let fixture_root = test_artifacts_root.join("notams").join("incremental-trace");
    let manifest_path = fixture_root.join("capture_manifest.json");
    if !manifest_path.is_file() {
        bail!(
            "incremental NOTAM fixture is missing {}; set AEROBAG_TEST_ARTIFACTS_ROOT",
            manifest_path.display()
        );
    }
    let manifest: TraceManifest = serde_json::from_slice(
        &fs::read(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    if manifest.schema_version != 2 {
        bail!(
            "unsupported NOTAM trace schema {}; expected 2",
            manifest.schema_version
        );
    }
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
    let start_path = fixture_root.join(&manifest.starting_snapshot.file);
    verify_trace_file(&start_path, &manifest.starting_snapshot)?;

    let connection = Connection::open_with_flags(
        &start_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("failed to open {}", start_path.display()))?;
    let starting_records = {
        let mut statement = connection.prepare(
            "SELECT id, status, last_updated_utc, record_json, updated_at_utc
             FROM starting_notams ORDER BY id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    let identity_cursors = {
        let mut statement = connection
            .prepare("SELECT id, ingest_seq FROM starting_identity_cursors ORDER BY id")?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    let mut messages = Vec::new();
    let mut expected_next = manifest.start_ingest_seq + 1;
    for segment in &manifest.segments {
        let segment_path = fixture_root.join(&segment.file);
        let compressed = fs::read(&segment_path)
            .with_context(|| format!("failed to read {}", segment_path.display()))?;
        verify_trace_bytes(&segment_path, &compressed, segment.bytes, &segment.sha256)?;
        let decoded = nav_kv_package::decode_xz_if_needed(&compressed)
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("failed to decode {}", segment_path.display()))?;
        if decoded.len() as u64 != segment.uncompressed_bytes {
            bail!(
                "{} decoded to {} bytes; expected {}",
                segment_path.display(),
                decoded.len(),
                segment.uncompressed_bytes
            );
        }
        let mut segment_messages = decoded
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice::<CapturedRawMessage>(line))
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| format!("failed to parse {}", segment_path.display()))?;
        if segment.cursor_first != expected_next
            || segment.cursor_last < segment.cursor_first
            || segment_messages.len() as u64 != segment.applied_message_count
        {
            bail!("invalid NOTAM trace segment metadata for {}", segment.file);
        }
        expected_next = segment.cursor_last + 1;
        messages.append(&mut segment_messages);
    }
    if expected_next != manifest.end_ingest_seq + 1 {
        bail!(
            "NOTAM trace ended at {}, expected {}",
            expected_next - 1,
            manifest.end_ingest_seq
        );
    }
    if messages
        .windows(2)
        .any(|pair| pair[0].ingest_seq >= pair[1].ingest_seq)
    {
        bail!("NOTAM trace messages are not strictly ordered");
    }
    Ok(LoadedTrace {
        start_ingest_seq: manifest.start_ingest_seq,
        end_ingest_seq: manifest.end_ingest_seq,
        starting_records,
        identity_cursors,
        messages,
        expected,
    })
}

fn verify_trace_file(path: &Path, expected: &TraceFile) -> anyhow::Result<()> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    verify_trace_bytes(path, &bytes, expected.bytes, &expected.sha256)
}

fn verify_trace_bytes(
    path: &Path,
    bytes: &[u8],
    expected_bytes: u64,
    expected_sha256: &str,
) -> anyhow::Result<()> {
    if bytes.len() as u64 != expected_bytes || sha256_hex(bytes) != expected_sha256 {
        bail!(
            "NOTAM trace fixture identity mismatch for {}",
            path.display()
        );
    }
    Ok(())
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
) -> anyhow::Result<AirportNotamIndex> {
    let expected = &boundaries[boundary];
    let bytes = artifacts
        .get(&boundary)
        .with_context(|| format!("missing checkpoint artifact at boundary {boundary}"))?;
    let resource_id = format!("live_feeds/state/notams/{}", expected.state_id);
    let mut background_work = BackgroundNotamWork::default();
    let (_, postcard) = prepare_live_feed_state_resource_with_notam_work(
        &resource_id,
        bytes,
        &mut background_work,
    )?;
    let envelope = decode_prepared_live_feed(&postcard)?;
    let PreparedLiveFeedPayload::Notams(PreparedNotamPayload::InstallCheckpoint(checkpoint)) =
        envelope.payload
    else {
        bail!("prepared NOTAM checkpoint used the wrong payload kind");
    };
    if background_work.records_decoded != checkpoint.records.len() as u64 {
        bail!("background checkpoint work did not count exact decoded records");
    }
    let mut main_work = NotamApplyWork::default();
    let index = AirportNotamIndex::from_checkpoint(checkpoint, &mut main_work)
        .map_err(anyhow::Error::msg)?;
    if index.state_id() != expected.state_id || index.counters() != expected.counters {
        bail!("client checkpoint install diverged at boundary {boundary}");
    }
    Ok(index)
}

#[allow(clippy::too_many_arguments)]
fn apply_transition_range(
    client: &mut AirportNotamIndex,
    transitions: &[NotamPublicationTransition],
    boundaries: &[Boundary],
    start: usize,
    end: usize,
    max_span: usize,
    output_root: &Path,
    artifacts: &mut HashMap<(usize, usize), Arc<Vec<u8>>>,
) -> anyhow::Result<()> {
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
        let (_, postcard) = prepare_live_feed_delta_resource_with_notam_work(
            &resource_id,
            &serde_json::Value::Null,
            &bytes,
            &mut background_work,
        )?;
        let envelope = decode_prepared_live_feed(&postcard)?;
        let PreparedLiveFeedPayload::Notams(PreparedNotamPayload::ApplyDelta(delta)) =
            envelope.payload
        else {
            bail!("prepared NOTAM delta used the wrong payload kind");
        };
        if background_work.records_decoded != delta.mutations.len() as u64 {
            bail!("background delta work did not count exact mutations");
        }
        let expected_mutations = delta.mutations.len() as u64;
        let mut main_work = NotamApplyWork::default();
        client
            .apply_delta(delta, &mut main_work)
            .map_err(anyhow::Error::msg)?;
        if main_work.mutations_applied != expected_mutations
            || main_work.full_record_collection_iterations != 0
            || main_work.full_state_serializations != 0
        {
            bail!("ordinary client delta application performed non-incremental work");
        }
        if client.state_id() != boundaries[next].state_id
            || client.counters() != boundaries[next].counters
        {
            bail!("client diverged after catch-up span {cursor}..{next}");
        }
        cursor = next;
    }
    Ok(())
}

fn assert_exact_final_state(client: &AirportNotamIndex, expected: &NotamCheckpoint) {
    assert_eq!(client.state_id(), expected.state_id);
    assert_eq!(client.counters(), expected.counters);
    assert_eq!(client.checkpoint(), *expected);
}

fn next_seed(seed: u64) -> u64 {
    seed.wrapping_mul(6364136223846793005).wrapping_add(1)
}
