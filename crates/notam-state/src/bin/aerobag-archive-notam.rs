// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Materialize a daily archive baseline using the same validated engine as clients.

use notam_state::{NotamApplyWork, NotamCheckpoint, NotamDelta, NotamState};
use serde::Deserialize;
use std::io;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    checkpoint: NotamCheckpoint,
    deltas: Vec<NotamDelta>,
    target_state_id: String,
}

fn materialize(request: Request) -> Result<NotamCheckpoint, Box<dyn std::error::Error>> {
    let mut work = NotamApplyWork::default();
    let mut state = NotamState::from_checkpoint(request.checkpoint, &mut work)?;
    for delta in request.deltas {
        state.apply_delta(delta, &mut work)?;
    }
    if state.state_id() != request.target_state_id {
        return Err("archive baseline does not reach requested NOTAM state".into());
    }
    Ok(state.checkpoint())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request = serde_json::from_reader(io::stdin().lock())?;
    serde_json::to_writer(io::stdout().lock(), &materialize(request)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use notam_state::{NotamMutation, NotamRecord};

    fn upsert(id: &str, text: &str) -> NotamMutation {
        let record: NotamRecord = serde_json::from_value(serde_json::json!({
            "id": id, "airport_id": "KSEA", "text": text,
        }))
        .unwrap();
        NotamMutation::Upsert { record }
    }

    fn update(state: &mut NotamState, mutation: NotamMutation) -> NotamDelta {
        let from = state.state_id().to_string();
        state
            .apply_mutation(mutation.clone(), &mut NotamApplyWork::default())
            .unwrap();
        NotamDelta::new(
            from,
            state.state_id().to_string(),
            state.counters(),
            vec![mutation],
        )
    }

    #[test]
    fn next_day_checkpoint_replays_suffix_including_deletions_without_old_day() {
        let mut source = NotamState::empty();
        update(&mut source, upsert("A", "original"));
        let old = source.checkpoint();
        let first_day = vec![
            update(&mut source, upsert("B", "second")),
            update(&mut source, upsert("A", "revised")),
        ];
        let next_day = materialize(Request {
            checkpoint: old,
            deltas: first_day,
            target_state_id: source.state_id().to_string(),
        })
        .unwrap();
        let encoded = serde_json::to_vec(&next_day).unwrap();
        let removed = update(
            &mut source,
            NotamMutation::Remove {
                notam_id: "A".to_string(),
            },
        );
        let restored = materialize(Request {
            checkpoint: serde_json::from_slice(&encoded).unwrap(),
            deltas: vec![removed],
            target_state_id: source.state_id().to_string(),
        })
        .unwrap();
        assert_eq!(restored, source.checkpoint());
        assert_eq!(restored.records.len(), 1);
        assert_eq!(restored.records[0].id, "B");
    }

    #[test]
    fn missing_or_reordered_delta_is_rejected_instead_of_archiving_a_partial_state() {
        let mut source = NotamState::empty();
        let checkpoint = source.checkpoint();
        let first = update(&mut source, upsert("A", "first"));
        let second = update(&mut source, upsert("B", "second"));
        for deltas in [vec![second.clone()], vec![second, first]] {
            assert!(materialize(Request {
                checkpoint: checkpoint.clone(),
                deltas,
                target_state_id: source.state_id().to_string(),
            })
            .is_err());
        }
    }

    #[test]
    fn independent_checkpoint_preserves_identity_and_rejects_wrong_target() {
        let checkpoint = NotamState::empty().checkpoint();
        let baseline = materialize(Request {
            checkpoint: checkpoint.clone(),
            deltas: Vec::new(),
            target_state_id: checkpoint.state_id.clone(),
        })
        .unwrap();
        assert_eq!(baseline, checkpoint);
        assert!(materialize(Request {
            checkpoint,
            deltas: Vec::new(),
            target_state_id: "wrong".to_string(),
        })
        .is_err());
    }
}
