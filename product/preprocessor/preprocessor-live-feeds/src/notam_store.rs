// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{bail, Context};
use chrono::Utc;
use notam_state::{
    bucket_for_id, compute_bucket_hash, compute_group_hash, compute_state_id, record_leaf_hash,
    NotamCounters, NotamHash, NotamMutation, NotamRecord, NotamState,
    NOTAM_MERKLE_BUCKETS_PER_GROUP, NOTAM_MERKLE_BUCKET_COUNT, NOTAM_MERKLE_GROUP_COUNT,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, Transaction};
use serde::Serialize;

use crate::{
    canonicalize_structured_notam_record, engine::sha256_hex, notam_projection_action,
    published_notam_record, structured_notam_record_from_json,
    validate_canonical_structured_notam_record, NotamProjectionAction, StructuredNotamRecord,
};

const NOTAM_STORE_SCHEMA_VERSION: u32 = 9;
const LEGACY_PROJECTION_SCHEMA_VERSION: u32 = 5;
const RAW_INGEST_CURSOR_METADATA_KEY: &str = "raw_ingest_cursor";
const STATE_ID_METADATA_KEY: &str = "notam_state_id";
const NOTAM_COUNT_METADATA_KEY: &str = "notam_count";
const AIRPORT_NOTAM_COUNT_METADATA_KEY: &str = "airport_notam_count";
const MULTIPLE_EFFECT_COUNT_METADATA_KEY: &str = "airport_notams_with_multiple_effects";
const OTHER_EFFECT_COUNT_METADATA_KEY: &str = "airport_notams_with_other_effect";
const RAW_INGEST_RETENTION_DAYS: i64 = 7;

#[derive(Debug, Clone)]
pub struct NotamStateReader {
    root: PathBuf,
}

impl NotamStateReader {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn current_records(&self) -> anyhow::Result<Vec<StructuredNotamRecord>> {
        let connection = self.open_connection()?;
        let mut statement = connection
            .prepare("SELECT record_json FROM current_notams ORDER BY id")
            .context("failed to prepare current NOTAM query")?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .context("failed to query current NOTAM records")?;
        rows.map(|row| {
            let record_json = row.context("failed to read current NOTAM record")?;
            let record = serde_json::from_str::<StructuredNotamRecord>(&record_json)
                .context("failed to decode current NOTAM record")?;
            validate_canonical_structured_notam_record(&record)
                .with_context(|| format!("invalid canonical NOTAM {}", record.id))?;
            Ok(record)
        })
        .collect()
    }

    fn open_connection(&self) -> anyhow::Result<Connection> {
        let path = self.root.join("state.sqlite");
        let connection = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| format!("failed to open NMS NOTAM state {}", path.display()))?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .context("failed to set NMS NOTAM sqlite busy timeout")?;
        Ok(connection)
    }

    #[cfg(test)]
    pub fn replace_records_for_test(
        root: &Path,
        records: &[StructuredNotamRecord],
    ) -> anyhow::Result<()> {
        fs::create_dir_all(root).with_context(|| format!("failed to create {}", root.display()))?;
        let mut connection = Connection::open(root.join("state.sqlite"))?;
        connection.execute_batch(
            "CREATE TABLE current_notams (
                id TEXT PRIMARY KEY,
                record_json TEXT NOT NULL
             );
             DELETE FROM current_notams;",
        )?;
        let tx = connection.transaction()?;
        for record in records {
            validate_canonical_structured_notam_record(record)?;
            tx.execute(
                "INSERT INTO current_notams(id, record_json) VALUES (?1, ?2)",
                params![record.id, serde_json::to_string(record)?],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct NotamPersistentStore {
    root: PathBuf,
}

#[derive(Debug)]
pub struct NotamStoreLock {
    file: File,
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedRawNotamSummary {
    pub applied_count: usize,
    pub changed_count: usize,
    pub removed_count: usize,
    pub rejected_count: usize,
    pub new_rejected_count: usize,
    pub max_ingest_seq: i64,
    pub last_received_at_utc: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotamRejectionStatus {
    pub unresolved_count: usize,
    pub oldest_unresolved_ingest_seq: Option<i64>,
    pub latest_unresolved_ingest_seq: Option<i64>,
    pub last_error: Option<String>,
    pub recent_rejections: Vec<NotamRejectionDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NotamRejectionDetail {
    pub ingest_seq: i64,
    pub first_rejected_at_utc: String,
    pub last_rejected_at_utc: String,
    pub rejection_count: usize,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetriedRejectedNotamSummary {
    pub retried_count: usize,
    pub resolved_count: usize,
    pub applied_count: usize,
    pub superseded_count: usize,
    pub changed_count: usize,
    pub removed_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotamPublicationTransition {
    pub journal_seq: i64,
    pub source_first_ingest_seq: i64,
    pub source_last_ingest_seq: i64,
    pub observed_at_utc: String,
    pub from_state_id: String,
    pub to_state_id: String,
    pub counters: NotamCounters,
    pub mutations: Vec<NotamMutation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotamPublicationCursor {
    pub published_through_journal_seq: i64,
    pub published_head_state_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotamPublicationSnapshot {
    pub current_state_id: String,
    pub counters: NotamCounters,
    pub cursor: NotamPublicationCursor,
    pub transitions: Vec<NotamPublicationTransition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynchronizedNotamSummary {
    pub state_id: String,
    pub changed_count: usize,
    pub removed_count: usize,
}

impl NotamPersistentStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn initialize(&self) -> anyhow::Result<()> {
        fs::create_dir_all(self.state_root())
            .with_context(|| format!("failed to create {}", self.state_root().display()))?;
        self.open_connection()?;
        Ok(())
    }

    pub fn acquire_lock(&self) -> anyhow::Result<NotamStoreLock> {
        fs::create_dir_all(&self.root)
            .with_context(|| format!("failed to create {}", self.root.display()))?;
        let path = self.root.join("lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .with_context(|| format!("failed to open NOTAM store lock {}", path.display()))?;
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc != 0 {
            let error = std::io::Error::last_os_error();
            if matches!(error.kind(), ErrorKind::WouldBlock) {
                bail!(
                    "NOTAM store is already locked by another process: {}",
                    path.display()
                );
            }
            return Err(error).with_context(|| format!("failed to lock {}", path.display()));
        }
        file.set_len(0)
            .with_context(|| format!("failed to clear {}", path.display()))?;
        writeln!(
            &mut &file,
            "pid={}\nlocked_at_utc={}",
            std::process::id(),
            Utc::now().to_rfc3339()
        )
        .with_context(|| format!("failed to write {}", path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync {}", path.display()))?;
        Ok(NotamStoreLock { file, path })
    }

    pub fn synchronize_current_records(
        &self,
        records: &[StructuredNotamRecord],
        observed_at_utc: &str,
    ) -> anyhow::Result<SynchronizedNotamSummary> {
        let mut incoming = BTreeMap::new();
        for record in records {
            validate_canonical_structured_notam_record(record)
                .with_context(|| format!("invalid canonical NOTAM {}", record.id))?;
            if notam_projection_action(record)? != NotamProjectionAction::Upsert {
                bail!(
                    "canonical current-state synchronization contains inactive NOTAM {}",
                    record.id
                );
            }
            if incoming.insert(record.id.clone(), record).is_some() {
                bail!(
                    "canonical current-state synchronization contains duplicate NOTAM {}",
                    record.id
                );
            }
        }

        let mut connection = self.open_connection()?;
        let tx = connection
            .transaction()
            .context("failed to start canonical NOTAM synchronization")?;
        let mut touched = {
            let mut statement = tx
                .prepare("SELECT id, record_json FROM notam_client_records ORDER BY id")
                .context("failed to prepare current published NOTAM query")?;
            let records = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .context("failed to query current published NOTAMs")?
                .map(|row| {
                    let (id, json) = row.context("failed to read current published NOTAM")?;
                    let record = serde_json::from_str::<NotamRecord>(&json)
                        .with_context(|| format!("failed to decode published NOTAM {id}"))?;
                    Ok((id, Some(record)))
                })
                .collect::<anyhow::Result<BTreeMap<_, _>>>()?;
            records
        };

        for id in touched.keys().cloned().collect::<Vec<_>>() {
            if incoming.contains_key(&id) {
                continue;
            }
            tx.execute("DELETE FROM current_notams WHERE id = ?1", [&id])
                .with_context(|| format!("failed to remove stale canonical NOTAM {id}"))?;
            tx.execute("DELETE FROM notam_client_records WHERE id = ?1", [&id])
                .with_context(|| format!("failed to remove stale published NOTAM {id}"))?;
        }
        for (id, record) in incoming {
            if !touched.contains_key(&id) {
                touched.insert(id.clone(), None);
            }
            apply_record_to_projection(&tx, ProjectionTable::Current, record, observed_at_utc)?;
        }

        let source_sequence = read_metadata(&tx, "canonical_source_sync_sequence")?
            .map(|value| {
                value
                    .parse::<u64>()
                    .context("failed to parse canonical NOTAM source sequence")
            })
            .transpose()?
            .unwrap_or(0)
            .checked_add(1)
            .context("canonical NOTAM source sequence overflow")?;
        let source_sequence = i64::try_from(source_sequence)
            .context("canonical NOTAM source sequence exceeds i64")?;
        let (changed_count, removed_count) = finalize_projection_transition(
            &tx,
            source_sequence,
            source_sequence,
            observed_at_utc,
            &touched,
        )?;
        tx.execute(
            "INSERT INTO metadata(key, value) VALUES ('canonical_source_sync_sequence', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [source_sequence.to_string()],
        )
        .context("failed to update canonical NOTAM source sequence")?;
        let state_id = read_metadata(&tx, STATE_ID_METADATA_KEY)?
            .context("NOTAM projection is missing its current state ID")?;
        tx.commit()
            .context("failed to commit canonical NOTAM synchronization")?;
        Ok(SynchronizedNotamSummary {
            state_id,
            changed_count,
            removed_count,
        })
    }

    pub fn apply_pending_raw_messages(
        &self,
        limit: usize,
    ) -> anyhow::Result<Option<AppliedRawNotamSummary>> {
        if limit == 0 {
            bail!("NOTAM raw ingest apply limit must be greater than zero");
        }
        let mut connection = self.open_connection()?;
        let last_cursor = raw_ingest_cursor(&connection)?;
        let rows = {
            let mut statement = connection
                .prepare(
                    "
                    SELECT ingest_seq, received_at_utc, message_json
                    FROM raw_notam_messages
                    WHERE ingest_seq > ?1
                    ORDER BY ingest_seq
                    LIMIT ?2
                    ",
                )
                .context("failed to query raw NOTAM ingest messages")?;
            let rows = statement
                .query_map(params![last_cursor, limit as i64], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .context("failed to query raw NOTAM ingest messages")?
                .collect::<Result<Vec<_>, _>>()
                .context("failed to read raw NOTAM ingest message")?;
            rows
        };
        if rows.is_empty() {
            return Ok(None);
        }

        let applied_at_utc = Utc::now().to_rfc3339();
        let mut rejected_count = 0_usize;
        let mut new_rejected_count = 0_usize;
        let mut last_received_at_utc = None;
        let first_ingest_seq = rows
            .first()
            .map(|row| row.0)
            .context("raw NOTAM rows unexpectedly empty")?;
        let max_ingest_seq = rows
            .last()
            .map(|row| row.0)
            .context("raw NOTAM rows unexpectedly empty")?;
        let tx = connection
            .transaction()
            .context("failed to start NOTAM raw ingest apply transaction")?;
        let mut touched = BTreeMap::<String, Option<NotamRecord>>::new();
        for (ingest_seq, received_at_utc, message_json) in &rows {
            last_received_at_utc = Some(received_at_utc.clone());
            match structured_notam_record_from_json(message_json) {
                Ok(Some(record)) => {
                    if !touched.contains_key(&record.id) {
                        touched.insert(record.id.clone(), read_client_record(&tx, &record.id)?);
                    }
                    apply_record_to_projection(
                        &tx,
                        ProjectionTable::Current,
                        &record,
                        &applied_at_utc,
                    )?;
                    record_notam_identity_cursor(&tx, &record.id, *ingest_seq)?;
                }
                Ok(None) => {}
                Err(error) => {
                    let is_new = quarantine_raw_notam_message(
                        &tx,
                        *ingest_seq,
                        &applied_at_utc,
                        &format!("{error:#}"),
                    )?;
                    rejected_count += 1;
                    new_rejected_count += usize::from(is_new);
                }
            }
        }
        let (changed_count, removed_count) = finalize_projection_transition(
            &tx,
            first_ingest_seq,
            max_ingest_seq,
            last_received_at_utc
                .as_deref()
                .unwrap_or(applied_at_utc.as_str()),
            &touched,
        )?;
        let max_ingest_seq_text = max_ingest_seq.to_string();
        tx.execute(
            "INSERT INTO metadata(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![RAW_INGEST_CURSOR_METADATA_KEY, max_ingest_seq_text.as_str()],
        )
        .context("failed to update NOTAM raw ingest cursor")?;
        tx.execute(
            "UPDATE raw_notam_messages
             SET applied_at_utc = ?1
             WHERE ingest_seq <= ?2 AND applied_at_utc IS NULL",
            params![applied_at_utc.as_str(), max_ingest_seq],
        )
        .context("failed to mark raw NOTAM ingest rows applied")?;
        tx.commit()
            .context("failed to commit NOTAM raw ingest apply transaction")?;

        self.prune_applied_raw_messages()?;

        Ok(Some(AppliedRawNotamSummary {
            applied_count: rows.len(),
            changed_count,
            removed_count,
            rejected_count,
            new_rejected_count,
            max_ingest_seq,
            last_received_at_utc,
        }))
    }

    pub fn rejection_status(&self) -> anyhow::Result<NotamRejectionStatus> {
        let connection = self.open_connection()?;
        let (unresolved_count, oldest, latest) = connection
            .query_row(
                "SELECT COUNT(*), MIN(ingest_seq), MAX(ingest_seq)
                 FROM rejected_notam_messages
                 WHERE resolved_at_utc IS NULL",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                    ))
                },
            )
            .context("failed to summarize rejected NOTAM messages")?;
        let last_error = connection
            .query_row(
                "SELECT error
                 FROM rejected_notam_messages
                 WHERE resolved_at_utc IS NULL
                 ORDER BY ingest_seq DESC
                 LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .context("failed to read latest rejected NOTAM error")?;
        let recent_rejections = {
            let mut statement = connection
                .prepare(
                    "SELECT ingest_seq, first_rejected_at_utc, last_rejected_at_utc,
                            rejection_count, error
                     FROM rejected_notam_messages
                     WHERE resolved_at_utc IS NULL
                     ORDER BY ingest_seq DESC
                     LIMIT 20",
                )
                .context("failed to prepare recent rejected NOTAM query")?;
            let rows = statement
                .query_map([], |row| {
                    Ok(NotamRejectionDetail {
                        ingest_seq: row.get(0)?,
                        first_rejected_at_utc: row.get(1)?,
                        last_rejected_at_utc: row.get(2)?,
                        rejection_count: row.get::<_, i64>(3)? as usize,
                        error: row.get(4)?,
                    })
                })
                .context("failed to query recent rejected NOTAM messages")?
                .collect::<Result<Vec<_>, _>>()
                .context("failed to read recent rejected NOTAM messages")?;
            rows
        };
        Ok(NotamRejectionStatus {
            unresolved_count: unresolved_count as usize,
            oldest_unresolved_ingest_seq: oldest,
            latest_unresolved_ingest_seq: latest,
            last_error,
            recent_rejections,
        })
    }

    pub fn retry_rejected_raw_messages(
        &self,
    ) -> anyhow::Result<Option<RetriedRejectedNotamSummary>> {
        let mut connection = self.open_connection()?;
        let rejected_rows = {
            let mut statement = connection
                .prepare(
                    "SELECT rejected.ingest_seq, raw.message_json
                     FROM rejected_notam_messages AS rejected
                     LEFT JOIN raw_notam_messages AS raw
                       ON raw.ingest_seq = rejected.ingest_seq
                     WHERE rejected.resolved_at_utc IS NULL
                     ORDER BY rejected.ingest_seq",
                )
                .context("failed to prepare rejected NOTAM retry query")?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
                })
                .context("failed to query rejected NOTAM messages")?
                .collect::<Result<Vec<_>, _>>()
                .context("failed to read rejected NOTAM messages")?;
            rows
        };
        if rejected_rows.is_empty() {
            return Ok(None);
        }
        for (ingest_seq, message_json) in &rejected_rows {
            if message_json.is_none() {
                bail!("rejected NOTAM row {ingest_seq} is missing its retained raw message");
            }
        }
        let retried_at_utc = Utc::now().to_rfc3339();
        let tx = connection
            .transaction()
            .context("failed to start rejected NOTAM retry transaction")?;
        let mut resolved_count = 0_usize;
        let mut applied_count = 0_usize;
        let mut superseded_count = 0_usize;
        let mut touched = BTreeMap::<String, Option<NotamRecord>>::new();
        for (ingest_seq, message_json) in &rejected_rows {
            let message_json = message_json.as_deref().with_context(|| {
                format!("rejected NOTAM row {ingest_seq} is missing its retained raw message")
            })?;
            match structured_notam_record_from_json(message_json) {
                Ok(record) => {
                    if let Some(record) = record {
                        let latest_ingest_seq = latest_notam_identity_cursor(&tx, &record.id)?;
                        if latest_ingest_seq.is_some_and(|latest| latest > *ingest_seq) {
                            superseded_count += 1;
                        } else {
                            if !touched.contains_key(&record.id) {
                                touched.insert(
                                    record.id.clone(),
                                    read_client_record(&tx, &record.id)?,
                                );
                            }
                            apply_record_to_projection(
                                &tx,
                                ProjectionTable::Current,
                                &record,
                                &retried_at_utc,
                            )?;
                            record_notam_identity_cursor(&tx, &record.id, *ingest_seq)?;
                            applied_count += 1;
                        }
                    }
                    tx.execute(
                        "UPDATE rejected_notam_messages
                         SET resolved_at_utc = ?1
                         WHERE ingest_seq = ?2 AND resolved_at_utc IS NULL",
                        params![retried_at_utc.as_str(), ingest_seq],
                    )
                    .with_context(|| {
                        format!("failed to resolve rejected NOTAM row {ingest_seq}")
                    })?;
                    resolved_count += 1;
                }
                Err(error) => {
                    tx.execute(
                        "UPDATE rejected_notam_messages
                         SET last_rejected_at_utc = ?1,
                             rejection_count = rejection_count + 1,
                             error = ?2
                         WHERE ingest_seq = ?3 AND resolved_at_utc IS NULL",
                        params![retried_at_utc.as_str(), format!("{error:#}"), ingest_seq],
                    )
                    .with_context(|| {
                        format!("failed to refresh rejected NOTAM row {ingest_seq}")
                    })?;
                }
            }
        }
        let source_first_ingest_seq = rejected_rows
            .first()
            .map(|row| row.0)
            .context("rejected NOTAM retry rows unexpectedly empty")?;
        let source_last_ingest_seq = rejected_rows
            .last()
            .map(|row| row.0)
            .context("rejected NOTAM retry rows unexpectedly empty")?;
        let (changed_count, removed_count) = finalize_projection_transition(
            &tx,
            source_first_ingest_seq,
            source_last_ingest_seq,
            &retried_at_utc,
            &touched,
        )?;
        tx.commit()
            .context("failed to commit rejected NOTAM retry transaction")?;
        self.prune_applied_raw_messages()?;
        Ok(Some(RetriedRejectedNotamSummary {
            retried_count: rejected_rows.len(),
            resolved_count,
            applied_count,
            superseded_count,
            changed_count,
            removed_count,
        }))
    }

    pub fn current_records(&self) -> anyhow::Result<Vec<StructuredNotamRecord>> {
        let connection = self.open_connection()?;
        let mut statement = connection
            .prepare("SELECT record_json FROM current_notams ORDER BY id")
            .context("failed to query current NOTAM records")?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .context("failed to query current NOTAM records")?;
        let mut records = Vec::new();
        for row in rows {
            let record_json = row.context("failed to read NOTAM record JSON")?;
            let record = serde_json::from_str::<StructuredNotamRecord>(&record_json)
                .context("failed to parse NOTAM record JSON")?;
            validate_canonical_structured_notam_record(&record)
                .with_context(|| format!("failed to validate canonical NOTAM {}", record.id))?;
            records.push(record);
        }
        Ok(records)
    }

    pub fn current_checkpoint(&self) -> anyhow::Result<notam_state::NotamCheckpoint> {
        let mut connection = self.open_connection()?;
        let tx = connection
            .transaction()
            .context("failed to start consistent NOTAM checkpoint read")?;
        let state_id = read_metadata(&tx, STATE_ID_METADATA_KEY)?
            .context("NOTAM projection is missing its current state ID")?;
        let counters = read_projection_counters(&tx)?;
        let records = {
            let mut statement = tx
                .prepare("SELECT record_json FROM notam_client_records ORDER BY id")
                .context("failed to prepare NOTAM checkpoint query")?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .context("failed to query NOTAM checkpoint records")?
                .collect::<Result<Vec<_>, _>>()
                .context("failed to read NOTAM checkpoint records")?;
            rows.into_iter()
                .map(|json| {
                    serde_json::from_str::<NotamRecord>(&json)
                        .context("failed to decode NOTAM checkpoint record")
                })
                .collect::<anyhow::Result<Vec<_>>>()?
        };
        let checkpoint = notam_state::NotamCheckpoint::new(state_id, counters, records);
        let verified = NotamState::from_checkpoint(
            checkpoint.clone(),
            &mut notam_state::NotamApplyWork::default(),
        )
        .map_err(anyhow::Error::msg)
        .context("failed to verify NOTAM checkpoint")?;
        if verified.state_id() != checkpoint.state_id {
            bail!("verified NOTAM checkpoint changed state identity");
        }
        tx.commit()
            .context("failed to finish consistent NOTAM checkpoint read")?;
        Ok(checkpoint)
    }

    pub fn current_state_summary(&self) -> anyhow::Result<(String, NotamCounters)> {
        let mut connection = self.open_connection()?;
        let tx = connection
            .transaction()
            .context("failed to start consistent NOTAM summary read")?;
        let state_id = read_metadata(&tx, STATE_ID_METADATA_KEY)?
            .context("NOTAM projection is missing its current state ID")?;
        let counters = read_projection_counters(&tx)?;
        tx.commit()
            .context("failed to finish consistent NOTAM summary read")?;
        Ok((state_id, counters))
    }

    pub fn publication_snapshot(&self) -> anyhow::Result<NotamPublicationSnapshot> {
        let mut connection = self.open_connection()?;
        let tx = connection
            .transaction()
            .context("failed to start consistent NOTAM publication read")?;
        let current_state_id = read_metadata(&tx, STATE_ID_METADATA_KEY)?
            .context("NOTAM projection is missing its current state ID")?;
        let counters = read_projection_counters(&tx)?;
        let cursor = tx
            .query_row(
                "SELECT published_through_journal_seq, published_head_state_id
                 FROM notam_publication_cursor WHERE singleton = 1",
                [],
                |row| {
                    Ok(NotamPublicationCursor {
                        published_through_journal_seq: row.get(0)?,
                        published_head_state_id: row.get(1)?,
                    })
                },
            )
            .context("failed to read NOTAM publication cursor")?;
        let transitions =
            read_pending_publication_transitions(&tx, cursor.published_through_journal_seq)?;
        if let Some(first) = transitions.first() {
            if cursor.published_head_state_id.is_some()
                && cursor.published_head_state_id.as_deref() != Some(&first.from_state_id)
            {
                bail!(
                    "NOTAM publication cursor head {:?} does not join journal state {}",
                    cursor.published_head_state_id,
                    first.from_state_id
                );
            }
        }
        if let Some(last) = transitions.last() {
            if last.to_state_id != current_state_id || last.counters != counters {
                bail!(
                    "NOTAM publication journal ends at {} {:?}, but projection is {} {:?}",
                    last.to_state_id,
                    last.counters,
                    current_state_id,
                    counters
                );
            }
        } else if let Some(head) = cursor.published_head_state_id.as_deref() {
            if head != current_state_id {
                bail!(
                    "NOTAM publication cursor is {head}, but projection is {current_state_id} with no pending journal"
                );
            }
        }
        tx.commit()
            .context("failed to finish consistent NOTAM publication read")?;
        Ok(NotamPublicationSnapshot {
            current_state_id,
            counters,
            cursor,
            transitions,
        })
    }

    pub fn publication_cursor(&self) -> anyhow::Result<NotamPublicationCursor> {
        let connection = self.open_connection()?;
        connection
            .query_row(
                "SELECT published_through_journal_seq, published_head_state_id
                 FROM notam_publication_cursor WHERE singleton = 1",
                [],
                |row| {
                    Ok(NotamPublicationCursor {
                        published_through_journal_seq: row.get(0)?,
                        published_head_state_id: row.get(1)?,
                    })
                },
            )
            .context("failed to read NOTAM publication cursor")
    }

    #[cfg(test)]
    pub fn set_publication_cursor_for_test(
        &self,
        cursor: &NotamPublicationCursor,
    ) -> anyhow::Result<()> {
        let connection = self.open_connection()?;
        connection
            .execute(
                "UPDATE notam_publication_cursor
                 SET published_through_journal_seq = ?1, published_head_state_id = ?2
                 WHERE singleton = 1",
                params![
                    cursor.published_through_journal_seq,
                    cursor.published_head_state_id
                ],
            )
            .context("failed to set test NOTAM publication cursor")?;
        Ok(())
    }

    pub fn pending_publication_transitions(
        &self,
    ) -> anyhow::Result<Vec<NotamPublicationTransition>> {
        let connection = self.open_connection()?;
        let cursor = connection
            .query_row(
                "SELECT published_through_journal_seq
                 FROM notam_publication_cursor WHERE singleton = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .context("failed to read NOTAM publication sequence")?;
        read_pending_publication_transitions(&connection, cursor)
    }

    pub fn advance_publication_cursor(
        &self,
        journal_seq: i64,
        expected_from_state_id: Option<&str>,
        to_state_id: &str,
    ) -> anyhow::Result<()> {
        let mut connection = self.open_connection()?;
        let tx = connection
            .transaction()
            .context("failed to start NOTAM publication cursor transaction")?;
        let current = tx
            .query_row(
                "SELECT published_through_journal_seq, published_head_state_id
                 FROM notam_publication_cursor WHERE singleton = 1",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .context("failed to read NOTAM publication cursor for advancement")?;
        if journal_seq < current.0 {
            bail!(
                "cannot move NOTAM publication cursor backward from {} to {journal_seq}",
                current.0
            );
        }
        if journal_seq == current.0 && current.1.as_deref() == Some(to_state_id) {
            tx.commit()
                .context("failed to finish idempotent NOTAM cursor advancement")?;
            return Ok(());
        }
        if current.1.as_deref() != expected_from_state_id {
            bail!(
                "NOTAM publication cursor head {:?} does not match expected {:?}",
                current.1,
                expected_from_state_id
            );
        }
        let published_at_utc = Utc::now().to_rfc3339();
        tx.execute(
            "UPDATE notam_publication_journal
             SET published_at_utc = COALESCE(published_at_utc, ?1)
             WHERE journal_seq > ?2 AND journal_seq <= ?3",
            params![published_at_utc, current.0, journal_seq],
        )
        .context("failed to mark NOTAM journal rows published")?;
        tx.execute(
            "UPDATE notam_publication_cursor
             SET published_through_journal_seq = ?1, published_head_state_id = ?2
             WHERE singleton = 1",
            params![journal_seq, to_state_id],
        )
        .context("failed to advance NOTAM publication cursor")?;
        tx.commit()
            .context("failed to commit NOTAM publication cursor")?;
        Ok(())
    }

    pub fn prune_published_journal_before(&self, cutoff_utc: &str) -> anyhow::Result<usize> {
        let mut connection = self.open_connection()?;
        let tx = connection
            .transaction()
            .context("failed to start NOTAM journal prune transaction")?;
        let cursor: i64 = tx
            .query_row(
                "SELECT published_through_journal_seq
                 FROM notam_publication_cursor WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .context("failed to read NOTAM publication cursor for pruning")?;
        tx.execute(
            "DELETE FROM notam_publication_operations
             WHERE journal_seq IN (
                 SELECT journal_seq FROM notam_publication_journal
                 WHERE journal_seq <= ?1
                   AND published_at_utc IS NOT NULL
                   AND published_at_utc < ?2
             )",
            params![cursor, cutoff_utc],
        )
        .context("failed to prune published NOTAM journal operations")?;
        let removed = tx
            .execute(
                "DELETE FROM notam_publication_journal
                 WHERE journal_seq <= ?1
                   AND published_at_utc IS NOT NULL
                   AND published_at_utc < ?2",
                params![cursor, cutoff_utc],
            )
            .context("failed to prune published NOTAM journal rows")?;
        tx.commit()
            .context("failed to commit NOTAM journal pruning")?;
        Ok(removed)
    }

    pub fn current_fingerprint(&self) -> anyhow::Result<String> {
        let records = self.current_records()?;
        let by_id = records
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect::<BTreeMap<_, _>>();
        let bytes = serde_json::to_vec(&by_id).context("failed to encode current NOTAM state")?;
        Ok(sha256_hex(&bytes))
    }

    pub fn sqlite_path(&self) -> PathBuf {
        self.state_root().join("current.sqlite")
    }

    #[cfg(test)]
    pub fn insert_raw_message_for_test(
        &self,
        dedupe_key: &str,
        received_at_utc: &str,
        message_json: &str,
    ) -> anyhow::Result<()> {
        let connection = self.open_connection()?;
        connection
            .execute(
                "INSERT OR IGNORE INTO raw_notam_messages (
                    dedupe_key, jms_message_id, received_at_utc, message_json, message_sha256, committed_at_utc
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    dedupe_key,
                    serde_json::from_str::<serde_json::Value>(message_json)
                        .ok()
                        .and_then(|value| value
                            .get("jmsMessageId")
                            .and_then(|message_id| message_id.as_str())
                            .map(str::to_string)),
                    received_at_utc,
                    message_json,
                    sha256_hex(message_json.as_bytes()),
                    Utc::now().to_rfc3339()
                ],
            )
            .context("failed to insert raw NOTAM test message")?;
        Ok(())
    }

    fn open_connection(&self) -> anyhow::Result<Connection> {
        fs::create_dir_all(self.state_root())
            .with_context(|| format!("failed to create {}", self.state_root().display()))?;
        let mut connection = Connection::open(self.sqlite_path())
            .with_context(|| format!("failed to open {}", self.sqlite_path().display()))?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .context("failed to configure NOTAM sqlite busy timeout")?;
        connection
            .execute_batch(
                "
                PRAGMA journal_mode = WAL;
                PRAGMA synchronous = FULL;
                PRAGMA auto_vacuum = INCREMENTAL;
                CREATE TABLE IF NOT EXISTS metadata (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS raw_notam_messages (
                    ingest_seq INTEGER PRIMARY KEY AUTOINCREMENT,
                    dedupe_key TEXT NOT NULL UNIQUE,
                    jms_message_id TEXT,
                    received_at_utc TEXT NOT NULL,
                    message_json TEXT NOT NULL,
                    message_sha256 TEXT NOT NULL,
                    committed_at_utc TEXT NOT NULL,
                    applied_at_utc TEXT
                );
                CREATE INDEX IF NOT EXISTS raw_notam_messages_applied_idx
                    ON raw_notam_messages(applied_at_utc, ingest_seq);
                CREATE TABLE IF NOT EXISTS current_notams (
                    id TEXT PRIMARY KEY,
                    status TEXT,
                    last_updated_utc TEXT,
                    record_json TEXT NOT NULL,
                    updated_at_utc TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS current_notams_status_idx
                    ON current_notams(status);
                CREATE INDEX IF NOT EXISTS current_notams_last_updated_idx
                    ON current_notams(last_updated_utc);
                CREATE TABLE IF NOT EXISTS rejected_notam_messages (
                    ingest_seq INTEGER PRIMARY KEY,
                    first_rejected_at_utc TEXT NOT NULL,
                    last_rejected_at_utc TEXT NOT NULL,
                    resolved_at_utc TEXT,
                    rejection_count INTEGER NOT NULL,
                    error TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS rejected_notam_messages_unresolved_idx
                    ON rejected_notam_messages(resolved_at_utc, ingest_seq);
                CREATE TABLE IF NOT EXISTS notam_identity_cursors (
                    id TEXT PRIMARY KEY,
                    ingest_seq INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS notam_client_records (
                    id TEXT PRIMARY KEY,
                    record_json TEXT NOT NULL,
                    record_hash BLOB NOT NULL,
                    merkle_bucket INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS notam_client_records_bucket_idx
                    ON notam_client_records(merkle_bucket, id);
                CREATE TABLE IF NOT EXISTS notam_merkle_buckets (
                    bucket INTEGER PRIMARY KEY,
                    record_count INTEGER NOT NULL,
                    bucket_hash BLOB NOT NULL
                );
                CREATE TABLE IF NOT EXISTS notam_merkle_groups (
                    group_id INTEGER PRIMARY KEY,
                    group_hash BLOB NOT NULL
                );
                CREATE TABLE IF NOT EXISTS notam_publication_journal (
                    journal_seq INTEGER PRIMARY KEY AUTOINCREMENT,
                    source_first_ingest_seq INTEGER NOT NULL,
                    source_last_ingest_seq INTEGER NOT NULL,
                    observed_at_utc TEXT NOT NULL,
                    from_state_id TEXT NOT NULL,
                    to_state_id TEXT NOT NULL,
                    notam_count INTEGER NOT NULL,
                    airport_notam_count INTEGER NOT NULL,
                    airport_notams_with_multiple_effects INTEGER NOT NULL,
                    airport_notams_with_other_effect INTEGER NOT NULL,
                    mutation_count INTEGER NOT NULL,
                    published_at_utc TEXT
                );
                CREATE TABLE IF NOT EXISTS notam_publication_operations (
                    journal_seq INTEGER NOT NULL,
                    operation_index INTEGER NOT NULL,
                    notam_id TEXT NOT NULL,
                    operation TEXT NOT NULL,
                    record_json TEXT,
                    PRIMARY KEY(journal_seq, operation_index),
                    UNIQUE(journal_seq, notam_id),
                    FOREIGN KEY(journal_seq) REFERENCES notam_publication_journal(journal_seq)
                );
                CREATE TABLE IF NOT EXISTS notam_publication_cursor (
                    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                    published_through_journal_seq INTEGER NOT NULL,
                    published_head_state_id TEXT
                );
                ",
            )
            .context("failed to initialize NOTAM sqlite schema")?;
        self.ensure_schema(&mut connection)?;
        Ok(connection)
    }

    fn ensure_schema(&self, connection: &mut Connection) -> anyhow::Result<()> {
        let schema_version = connection
            .query_row(
                "SELECT value FROM metadata WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .context("failed to query NOTAM sqlite schema version")?;
        match schema_version.as_deref() {
            None => self.migrate_incremental_schema(connection),
            Some("9") => Ok(()),
            Some("7") => self.migrate_incremental_schema(connection),
            Some("6") => {
                self.migrate_schema_v6_to_v7(connection)?;
                self.migrate_incremental_schema(connection)
            }
            Some("5") => self.migrate_incremental_schema(connection),
            Some("4") => {
                connection
                    .execute(
                        "UPDATE metadata SET value = ?1 WHERE key = 'schema_version'",
                        [LEGACY_PROJECTION_SCHEMA_VERSION.to_string()],
                    )
                    .context("failed to promote NOTAM sqlite schema 4")?;
                self.migrate_incremental_schema(connection)
            }
            Some("3") => {
                self.reproject_schema_v3(connection)?;
                self.migrate_incremental_schema(connection)
            }
            Some("2") => {
                self.reproject_schema_v2(connection)?;
                self.migrate_incremental_schema(connection)
            }
            Some(version) => bail!(
                "unsupported NOTAM sqlite schema {version}; required {NOTAM_STORE_SCHEMA_VERSION}"
            ),
        }
    }

    fn migrate_schema_v6_to_v7(&self, connection: &mut Connection) -> anyhow::Result<()> {
        let migrated_at_utc = Utc::now().to_rfc3339();
        let tx = connection
            .transaction()
            .context("failed to start NOTAM schema 6 journal migration")?;
        tx.execute(
            "ALTER TABLE notam_publication_journal ADD COLUMN published_at_utc TEXT",
            [],
        )
        .context("failed to add NOTAM journal publication timestamp")?;
        tx.execute(
            "UPDATE notam_publication_journal
             SET published_at_utc = ?1
             WHERE journal_seq <= (
                 SELECT published_through_journal_seq
                 FROM notam_publication_cursor WHERE singleton = 1
             )",
            [migrated_at_utc],
        )
        .context("failed to timestamp migrated published NOTAM journal rows")?;
        tx.execute(
            "UPDATE metadata SET value = ?1 WHERE key = 'schema_version'",
            ["7"],
        )
        .context("failed to promote NOTAM sqlite schema 7")?;
        tx.commit()
            .context("failed to commit NOTAM schema 6 journal migration")
    }

    fn migrate_incremental_schema(&self, connection: &mut Connection) -> anyhow::Result<()> {
        let source_rows = {
            let mut statement = connection
                .prepare("SELECT record_json FROM current_notams ORDER BY id")
                .context("failed to prepare NOTAM incremental migration query")?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .context("failed to query NOTAM incremental migration rows")?
                .collect::<Result<Vec<_>, _>>()
                .context("failed to read NOTAM incremental migration rows")?;
            rows
        };

        let tx = connection
            .transaction()
            .context("failed to start NOTAM incremental migration")?;
        tx.execute_batch(
            "DELETE FROM notam_client_records;
             DELETE FROM notam_merkle_buckets;
             DELETE FROM notam_merkle_groups;
             DELETE FROM notam_publication_operations;
             DELETE FROM notam_publication_journal;
             DELETE FROM notam_publication_cursor;",
        )
        .context("failed to clear NOTAM incremental migration targets")?;

        let mut state = NotamState::empty();
        let mut work = Default::default();
        let mut bucket_members = (0..NOTAM_MERKLE_BUCKET_COUNT)
            .map(|_| BTreeMap::<String, NotamHash>::new())
            .collect::<Vec<_>>();
        for record_json in source_rows {
            let structured = serde_json::from_str::<StructuredNotamRecord>(&record_json)
                .context("failed to decode NOTAM incremental migration record")?;
            let record = published_notam_record(&structured);
            let client_json = serde_json::to_string(&record)
                .with_context(|| format!("failed to encode published NOTAM {}", record.id))?;
            let hash = record_leaf_hash(&record)
                .map_err(anyhow::Error::msg)
                .with_context(|| format!("failed to hash published NOTAM {}", record.id))?;
            let bucket = bucket_for_id(&record.id);
            tx.execute(
                "INSERT INTO notam_client_records(id, record_json, record_hash, merkle_bucket)
                 VALUES (?1, ?2, ?3, ?4)",
                params![record.id, client_json, hash.as_slice(), bucket as i64],
            )
            .context("failed to insert NOTAM incremental migration record")?;
            bucket_members[bucket].insert(record.id.clone(), hash);
            state
                .apply_mutation(NotamMutation::Upsert { record }, &mut work)
                .map_err(anyhow::Error::msg)
                .context("failed to build NOTAM incremental migration state")?;
        }

        let bucket_hashes = bucket_members
            .iter()
            .enumerate()
            .map(|(bucket, members)| {
                compute_bucket_hash(bucket, members.iter().map(|(id, hash)| (id.as_str(), hash)))
            })
            .collect::<Vec<_>>();
        for (bucket, hash) in bucket_hashes.iter().enumerate() {
            tx.execute(
                "INSERT INTO notam_merkle_buckets(bucket, record_count, bucket_hash)
                 VALUES (?1, ?2, ?3)",
                params![
                    bucket as i64,
                    bucket_members[bucket].len() as i64,
                    hash.as_slice()
                ],
            )
            .context("failed to insert NOTAM Merkle migration bucket")?;
        }
        let mut group_hashes = Vec::with_capacity(NOTAM_MERKLE_GROUP_COUNT);
        for group in 0..NOTAM_MERKLE_GROUP_COUNT {
            let hash = compute_group_hash(group, &bucket_hashes)
                .map_err(anyhow::Error::msg)
                .context("failed to compute NOTAM Merkle migration group")?;
            tx.execute(
                "INSERT INTO notam_merkle_groups(group_id, group_hash) VALUES (?1, ?2)",
                params![group as i64, hash.as_slice()],
            )
            .context("failed to insert NOTAM Merkle migration group")?;
            group_hashes.push(hash);
        }
        let state_id = compute_state_id(&group_hashes, state.counters())
            .map_err(anyhow::Error::msg)
            .context("failed to compute NOTAM incremental migration root")?;
        if state_id != state.state_id() {
            bail!(
                "NOTAM incremental migration root mismatch: SQL {state_id}, shared state {}",
                state.state_id()
            );
        }
        write_projection_metadata(&tx, &state_id, state.counters())?;
        tx.execute(
            "INSERT INTO notam_publication_cursor(
                singleton, published_through_journal_seq, published_head_state_id
             ) VALUES (1, 0, NULL)",
            [],
        )
        .context("failed to initialize NOTAM publication cursor")?;
        tx.execute(
            "INSERT INTO metadata(key, value) VALUES ('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [NOTAM_STORE_SCHEMA_VERSION.to_string()],
        )
        .context("failed to promote NOTAM incremental schema")?;
        tx.commit()
            .context("failed to commit NOTAM incremental migration")?;
        Ok(())
    }

    fn reproject_schema_v2(&self, connection: &mut Connection) -> anyhow::Result<()> {
        let raw_rows = {
            let mut statement = connection
                .prepare(
                    "SELECT ingest_seq, message_json
                     FROM raw_notam_messages
                     ORDER BY ingest_seq",
                )
                .context("failed to prepare NOTAM schema reprojection query")?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                })
                .context("failed to query NOTAM schema reprojection rows")?
                .collect::<Result<Vec<_>, _>>()
                .context("failed to read NOTAM schema reprojection rows")?;
            rows
        };
        let current_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM current_notams", [], |row| row.get(0))
            .context("failed to count current NOTAM projection")?;
        if current_count > 0 && raw_rows.first().map(|row| row.0) != Some(1) {
            bail!(
                "cannot reproject NOTAM schema 2 without complete raw history beginning at ingest sequence 1"
            );
        }

        let reprojected_at_utc = Utc::now().to_rfc3339();
        let max_ingest_seq = raw_rows.last().map(|row| row.0).unwrap_or(0);
        let tx = connection
            .transaction()
            .context("failed to start NOTAM schema reprojection transaction")?;
        tx.execute_batch(
            "DROP TABLE IF EXISTS current_notams_v4;
             CREATE TABLE current_notams_v4 (
                id TEXT PRIMARY KEY,
                status TEXT,
                last_updated_utc TEXT,
                record_json TEXT NOT NULL,
                updated_at_utc TEXT NOT NULL
             );",
        )
        .context("failed to create NOTAM schema 4 projection")?;
        for (ingest_seq, message_json) in &raw_rows {
            if let Some(record) =
                structured_notam_record_from_json(message_json).with_context(|| {
                    format!("failed to normalize raw NOTAM reprojection row {ingest_seq}")
                })?
            {
                apply_record_to_projection(
                    &tx,
                    ProjectionTable::SchemaV4,
                    &record,
                    &reprojected_at_utc,
                )?;
                record_notam_identity_cursor(&tx, &record.id, *ingest_seq)?;
            }
        }
        tx.execute(
            "INSERT INTO metadata(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![RAW_INGEST_CURSOR_METADATA_KEY, max_ingest_seq.to_string()],
        )
        .context("failed to update NOTAM reprojection cursor")?;
        tx.execute(
            "UPDATE raw_notam_messages
             SET applied_at_utc = COALESCE(applied_at_utc, ?1)",
            [&reprojected_at_utc],
        )
        .context("failed to mark reprojected NOTAM rows applied")?;
        tx.execute(
            "UPDATE metadata SET value = ?1 WHERE key = 'schema_version'",
            [LEGACY_PROJECTION_SCHEMA_VERSION.to_string()],
        )
        .context("failed to promote NOTAM sqlite schema version")?;
        tx.execute_batch(
            "ALTER TABLE current_notams RENAME TO current_notams_v2_retired;
             ALTER TABLE current_notams_v4 RENAME TO current_notams;
             DROP TABLE current_notams_v2_retired;
             CREATE INDEX current_notams_status_idx ON current_notams(status);
             CREATE INDEX current_notams_last_updated_idx
                ON current_notams(last_updated_utc);",
        )
        .context("failed to atomically promote NOTAM schema 4 projection")?;
        tx.commit()
            .context("failed to commit NOTAM schema reprojection")?;
        Ok(())
    }

    fn reproject_schema_v3(&self, connection: &mut Connection) -> anyhow::Result<()> {
        let rows = {
            let mut statement = connection
                .prepare("SELECT record_json, updated_at_utc FROM current_notams ORDER BY id")
                .context("failed to prepare NOTAM schema 3 reprojection query")?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .context("failed to query NOTAM schema 3 projection rows")?
                .collect::<Result<Vec<_>, _>>()
                .context("failed to read NOTAM schema 3 projection rows")?;
            rows
        };

        let tx = connection
            .transaction()
            .context("failed to start NOTAM schema 3 reprojection transaction")?;
        tx.execute_batch(
            "DROP TABLE IF EXISTS current_notams_v4;
             CREATE TABLE current_notams_v4 (
                id TEXT PRIMARY KEY,
                status TEXT,
                last_updated_utc TEXT,
                record_json TEXT NOT NULL,
                updated_at_utc TEXT NOT NULL
             );",
        )
        .context("failed to create NOTAM schema 4 projection")?;
        for (record_json, updated_at_utc) in rows {
            let record = serde_json::from_str::<StructuredNotamRecord>(&record_json)
                .context("failed to decode NOTAM schema 3 projection row")?;
            let record = canonicalize_structured_notam_record(record)
                .context("failed to classify NOTAM schema 3 projection row")?;
            apply_record_to_projection(&tx, ProjectionTable::SchemaV4, &record, &updated_at_utc)?;
        }
        tx.execute(
            "UPDATE metadata SET value = ?1 WHERE key = 'schema_version'",
            [LEGACY_PROJECTION_SCHEMA_VERSION.to_string()],
        )
        .context("failed to promote NOTAM sqlite schema version")?;
        tx.execute_batch(
            "ALTER TABLE current_notams RENAME TO current_notams_v3_retired;
             ALTER TABLE current_notams_v4 RENAME TO current_notams;
             DROP TABLE current_notams_v3_retired;
             CREATE INDEX current_notams_status_idx ON current_notams(status);
             CREATE INDEX current_notams_last_updated_idx
                ON current_notams(last_updated_utc);",
        )
        .context("failed to atomically promote NOTAM schema 4 projection")?;
        tx.commit()
            .context("failed to commit NOTAM schema 3 reprojection")?;
        Ok(())
    }

    fn prune_applied_raw_messages(&self) -> anyhow::Result<()> {
        let connection = self.open_connection()?;
        let deleted = connection
            .execute(
                "
                DELETE FROM raw_notam_messages
                WHERE applied_at_utc IS NOT NULL
                  AND ingest_seq <= ?1
                  AND NOT EXISTS (
                      SELECT 1
                      FROM rejected_notam_messages AS rejected
                      WHERE rejected.ingest_seq = raw_notam_messages.ingest_seq
                        AND rejected.resolved_at_utc IS NULL
                  )
                  AND julianday(applied_at_utc) < julianday('now', ?2)
                ",
                params![
                    raw_ingest_cursor(&connection)?,
                    format!("-{} days", RAW_INGEST_RETENTION_DAYS)
                ],
            )
            .context("failed to prune applied raw NOTAM ingest rows")?;
        if deleted > 0 {
            connection
                .execute_batch(
                    "
                    PRAGMA wal_checkpoint(TRUNCATE);
                    PRAGMA incremental_vacuum;
                    ",
                )
                .context("failed to compact NOTAM sqlite after raw ingest prune")?;
        }
        Ok(())
    }

    fn state_root(&self) -> PathBuf {
        self.root.join("state")
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProjectionTable {
    Current,
    SchemaV4,
}

impl ProjectionTable {
    fn name(self) -> &'static str {
        match self {
            Self::Current => "current_notams",
            Self::SchemaV4 => "current_notams_v4",
        }
    }
}

fn latest_notam_identity_cursor(tx: &Transaction<'_>, id: &str) -> anyhow::Result<Option<i64>> {
    tx.query_row(
        "SELECT ingest_seq FROM notam_identity_cursors WHERE id = ?1",
        [id],
        |row| row.get::<_, i64>(0),
    )
    .optional()
    .with_context(|| format!("failed to read NOTAM identity cursor for {id}"))
}

fn record_notam_identity_cursor(
    tx: &Transaction<'_>,
    id: &str,
    ingest_seq: i64,
) -> anyhow::Result<()> {
    tx.execute(
        "INSERT INTO notam_identity_cursors(id, ingest_seq) VALUES (?1, ?2)
         ON CONFLICT(id) DO UPDATE SET ingest_seq = excluded.ingest_seq
         WHERE excluded.ingest_seq > notam_identity_cursors.ingest_seq",
        params![id, ingest_seq],
    )
    .with_context(|| format!("failed to record NOTAM identity cursor for {id}"))?;
    Ok(())
}

fn quarantine_raw_notam_message(
    tx: &Transaction<'_>,
    ingest_seq: i64,
    rejected_at_utc: &str,
    error: &str,
) -> anyhow::Result<bool> {
    let inserted = tx
        .execute(
            "INSERT OR IGNORE INTO rejected_notam_messages (
                ingest_seq,
                first_rejected_at_utc,
                last_rejected_at_utc,
                resolved_at_utc,
                rejection_count,
                error
             ) VALUES (?1, ?2, ?2, NULL, 1, ?3)",
            params![ingest_seq, rejected_at_utc, error],
        )
        .with_context(|| format!("failed to quarantine raw NOTAM row {ingest_seq}"))?;
    if inserted == 0 {
        tx.execute(
            "UPDATE rejected_notam_messages
             SET last_rejected_at_utc = ?1,
                 resolved_at_utc = NULL,
                 rejection_count = rejection_count + 1,
                 error = ?2
             WHERE ingest_seq = ?3",
            params![rejected_at_utc, error, ingest_seq],
        )
        .with_context(|| format!("failed to refresh quarantined raw NOTAM row {ingest_seq}"))?;
    }
    Ok(inserted > 0)
}

fn apply_record_to_projection(
    tx: &Transaction<'_>,
    table: ProjectionTable,
    record: &StructuredNotamRecord,
    updated_at_utc: &str,
) -> anyhow::Result<(usize, usize)> {
    let table = table.name();
    match notam_projection_action(record)? {
        NotamProjectionAction::Remove => {
            let sql = format!("DELETE FROM {table} WHERE id = ?1");
            let removed = tx
                .execute(&sql, [&record.id])
                .with_context(|| format!("failed to delete NOTAM {}", record.id))?;
            if table == ProjectionTable::Current.name() {
                tx.execute(
                    "DELETE FROM notam_client_records WHERE id = ?1",
                    [&record.id],
                )
                .with_context(|| format!("failed to delete published NOTAM {}", record.id))?;
            }
            return Ok((0, removed));
        }
        NotamProjectionAction::Upsert => {}
    }

    let record_json = serde_json::to_string(record)
        .with_context(|| format!("failed to encode NOTAM {}", record.id))?;
    let existing: Option<String> = tx
        .query_row(
            &format!("SELECT record_json FROM {table} WHERE id = ?1"),
            [&record.id],
            |row| row.get(0),
        )
        .optional()
        .with_context(|| format!("failed to query NOTAM {}", record.id))?;
    tx.execute(
        &format!(
            "INSERT INTO {table} (
            id, status, last_updated_utc, record_json, updated_at_utc
         ) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
            status = excluded.status,
            last_updated_utc = excluded.last_updated_utc,
            record_json = excluded.record_json,
            updated_at_utc = excluded.updated_at_utc"
        ),
        params![
            record.id.as_str(),
            record.notam_status.as_deref(),
            record.last_updated_utc.as_deref(),
            record_json,
            updated_at_utc,
        ],
    )
    .with_context(|| format!("failed to upsert NOTAM {}", record.id))?;
    if table == ProjectionTable::Current.name() {
        let client_record = published_notam_record(record);
        let client_json = serde_json::to_string(&client_record)
            .with_context(|| format!("failed to encode published NOTAM {}", record.id))?;
        let hash = record_leaf_hash(&client_record)
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("failed to hash published NOTAM {}", record.id))?;
        tx.execute(
            "INSERT INTO notam_client_records(id, record_json, record_hash, merkle_bucket)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
                record_json = excluded.record_json,
                record_hash = excluded.record_hash,
                merkle_bucket = excluded.merkle_bucket",
            params![
                client_record.id,
                client_json,
                hash.as_slice(),
                bucket_for_id(&record.id) as i64
            ],
        )
        .with_context(|| format!("failed to upsert published NOTAM {}", record.id))?;
    }
    Ok((
        (existing.as_deref() != Some(record_json.as_str())) as usize,
        0,
    ))
}

fn read_client_record(tx: &Transaction<'_>, id: &str) -> anyhow::Result<Option<NotamRecord>> {
    let record_json = tx
        .query_row(
            "SELECT record_json FROM notam_client_records WHERE id = ?1",
            [id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .with_context(|| format!("failed to read published NOTAM {id}"))?;
    record_json
        .map(|json| {
            serde_json::from_str(&json)
                .with_context(|| format!("failed to decode published NOTAM {id}"))
        })
        .transpose()
}

fn read_journal_mutations(
    connection: &Connection,
    journal_seq: i64,
) -> anyhow::Result<Vec<NotamMutation>> {
    let mut statement = connection
        .prepare(
            "SELECT notam_id, operation, record_json
             FROM notam_publication_operations
             WHERE journal_seq = ?1
             ORDER BY operation_index",
        )
        .with_context(|| format!("failed to prepare NOTAM journal {journal_seq} operations"))?;
    let rows = statement
        .query_map([journal_seq], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .with_context(|| format!("failed to query NOTAM journal {journal_seq} operations"))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to read NOTAM journal {journal_seq} operations"))?;
    rows.into_iter()
        .map(
            |(notam_id, operation, record_json)| match operation.as_str() {
                "upsert" => {
                    let json = record_json.with_context(|| {
                        format!("NOTAM journal {journal_seq} upsert {notam_id} has no record")
                    })?;
                    let record = serde_json::from_str::<NotamRecord>(&json).with_context(|| {
                        format!("failed to decode NOTAM journal {journal_seq} record {notam_id}")
                    })?;
                    if record.id != notam_id {
                        bail!(
                            "NOTAM journal {journal_seq} operation ID {notam_id} contains {}",
                            record.id
                        );
                    }
                    Ok(NotamMutation::Upsert { record })
                }
                "remove" => {
                    if record_json.is_some() {
                        bail!("NOTAM journal {journal_seq} removal {notam_id} contains a record");
                    }
                    Ok(NotamMutation::Remove { notam_id })
                }
                other => bail!(
                    "NOTAM journal {journal_seq} operation {notam_id} has unknown kind {other}"
                ),
            },
        )
        .collect()
}

fn read_pending_publication_transitions(
    connection: &Connection,
    cursor: i64,
) -> anyhow::Result<Vec<NotamPublicationTransition>> {
    let parents = {
        let mut statement = connection
            .prepare(
                "SELECT journal_seq, source_first_ingest_seq, source_last_ingest_seq,
                        observed_at_utc, from_state_id, to_state_id, notam_count,
                        airport_notam_count, airport_notams_with_multiple_effects,
                        airport_notams_with_other_effect, mutation_count
                 FROM notam_publication_journal
                 WHERE journal_seq > ?1
                 ORDER BY journal_seq",
            )
            .context("failed to prepare pending NOTAM publication query")?;
        let rows = statement
            .query_map([cursor], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    NotamCounters {
                        notam_count: row.get::<_, i64>(6)? as u64,
                        airport_notam_count: row.get::<_, i64>(7)? as u64,
                        airport_notams_with_multiple_effects: row.get::<_, i64>(8)? as u64,
                        airport_notams_with_other_effect: row.get::<_, i64>(9)? as u64,
                    },
                    row.get::<_, i64>(10)? as usize,
                ))
            })
            .context("failed to query pending NOTAM publications")?
            .collect::<Result<Vec<_>, _>>()
            .context("failed to read pending NOTAM publications")?;
        rows
    };
    let mut transitions = Vec::with_capacity(parents.len());
    for (
        journal_seq,
        source_first_ingest_seq,
        source_last_ingest_seq,
        observed_at_utc,
        from_state_id,
        to_state_id,
        counters,
        expected_mutation_count,
    ) in parents
    {
        let mutations = read_journal_mutations(connection, journal_seq)?;
        if mutations.len() != expected_mutation_count {
            bail!(
                "NOTAM journal {journal_seq} has {} operations; expected {expected_mutation_count}",
                mutations.len()
            );
        }
        notam_state::validate_mutation_order(&mutations)
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("invalid NOTAM journal {journal_seq}"))?;
        transitions.push(NotamPublicationTransition {
            journal_seq,
            source_first_ingest_seq,
            source_last_ingest_seq,
            observed_at_utc,
            from_state_id,
            to_state_id,
            counters,
            mutations,
        });
    }
    for pair in transitions.windows(2) {
        if pair[0].to_state_id != pair[1].from_state_id {
            bail!(
                "NOTAM publication journal does not form one chain at {} -> {}",
                pair[0].journal_seq,
                pair[1].journal_seq
            );
        }
    }
    Ok(transitions)
}

fn finalize_projection_transition(
    tx: &Transaction<'_>,
    source_first_ingest_seq: i64,
    source_last_ingest_seq: i64,
    observed_at_utc: &str,
    touched: &BTreeMap<String, Option<NotamRecord>>,
) -> anyhow::Result<(usize, usize)> {
    let mut mutations = BTreeMap::<String, NotamMutation>::new();
    let mut counters = read_projection_counters(tx)?;
    let mut affected_buckets = std::collections::BTreeSet::new();
    for (id, old_record) in touched {
        let new_record = read_client_record(tx, id)?;
        if old_record == &new_record {
            continue;
        }
        if let Some(record) = old_record {
            subtract_counter_contribution(&mut counters, record)?;
            affected_buckets.insert(bucket_for_id(&record.id));
        }
        let mutation = if let Some(record) = new_record {
            add_counter_contribution(&mut counters, &record);
            affected_buckets.insert(bucket_for_id(&record.id));
            NotamMutation::Upsert { record }
        } else {
            NotamMutation::Remove {
                notam_id: id.clone(),
            }
        };
        mutations.insert(id.clone(), mutation);
    }
    if mutations.is_empty() {
        return Ok((0, 0));
    }

    let from_state_id = read_metadata(tx, STATE_ID_METADATA_KEY)?
        .context("NOTAM projection is missing its current state ID")?;
    for bucket in &affected_buckets {
        let members = {
            let mut statement = tx
                .prepare(
                    "SELECT id, record_hash
                     FROM notam_client_records
                     WHERE merkle_bucket = ?1
                     ORDER BY id",
                )
                .context("failed to prepare affected NOTAM bucket query")?;
            let rows = statement
                .query_map([*bucket as i64], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
                })
                .context("failed to query affected NOTAM bucket")?
                .collect::<Result<Vec<_>, _>>()
                .context("failed to read affected NOTAM bucket")?;
            rows
        };
        let members = members
            .into_iter()
            .map(|(id, hash)| Ok((id, decode_hash(&hash)?)))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let bucket_hash = compute_bucket_hash(
            *bucket,
            members.iter().map(|(id, hash)| (id.as_str(), hash)),
        );
        tx.execute(
            "UPDATE notam_merkle_buckets
             SET record_count = ?1, bucket_hash = ?2
             WHERE bucket = ?3",
            params![members.len() as i64, bucket_hash.as_slice(), *bucket as i64],
        )
        .with_context(|| format!("failed to update NOTAM Merkle bucket {bucket}"))?;
    }

    let bucket_hashes = read_ordered_hashes(
        tx,
        "SELECT bucket_hash FROM notam_merkle_buckets ORDER BY bucket",
        NOTAM_MERKLE_BUCKET_COUNT,
        "NOTAM Merkle buckets",
    )?;
    let affected_groups = affected_buckets
        .iter()
        .map(|bucket| bucket / NOTAM_MERKLE_BUCKETS_PER_GROUP)
        .collect::<std::collections::BTreeSet<_>>();
    for group in affected_groups {
        let group_hash = compute_group_hash(group, &bucket_hashes)
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("failed to compute NOTAM Merkle group {group}"))?;
        tx.execute(
            "UPDATE notam_merkle_groups SET group_hash = ?1 WHERE group_id = ?2",
            params![group_hash.as_slice(), group as i64],
        )
        .with_context(|| format!("failed to update NOTAM Merkle group {group}"))?;
    }
    let group_hashes = read_ordered_hashes(
        tx,
        "SELECT group_hash FROM notam_merkle_groups ORDER BY group_id",
        NOTAM_MERKLE_GROUP_COUNT,
        "NOTAM Merkle groups",
    )?;
    let to_state_id = compute_state_id(&group_hashes, counters)
        .map_err(anyhow::Error::msg)
        .context("failed to compute NOTAM projection state ID")?;
    if to_state_id == from_state_id {
        bail!("NOTAM mutations did not change the state ID");
    }

    let changed_count = mutations
        .values()
        .filter(|mutation| matches!(mutation, NotamMutation::Upsert { .. }))
        .count();
    let removed_count = mutations.len() - changed_count;
    tx.execute(
        "INSERT INTO notam_publication_journal(
            source_first_ingest_seq,
            source_last_ingest_seq,
            observed_at_utc,
            from_state_id,
            to_state_id,
            notam_count,
            airport_notam_count,
            airport_notams_with_multiple_effects,
            airport_notams_with_other_effect,
            mutation_count
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            source_first_ingest_seq,
            source_last_ingest_seq,
            observed_at_utc,
            from_state_id,
            to_state_id,
            counters.notam_count as i64,
            counters.airport_notam_count as i64,
            counters.airport_notams_with_multiple_effects as i64,
            counters.airport_notams_with_other_effect as i64,
            mutations.len() as i64,
        ],
    )
    .context("failed to append NOTAM publication journal transition")?;
    let journal_seq = tx.last_insert_rowid();
    for (operation_index, mutation) in mutations.into_values().enumerate() {
        let (id, operation, record_json) = match mutation {
            NotamMutation::Upsert { record } => {
                let json = serde_json::to_string(&record)
                    .with_context(|| format!("failed to journal published NOTAM {}", record.id))?;
                (record.id, "upsert", Some(json))
            }
            NotamMutation::Remove { notam_id } => (notam_id, "remove", None),
        };
        tx.execute(
            "INSERT INTO notam_publication_operations(
                journal_seq, operation_index, notam_id, operation, record_json
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                journal_seq,
                operation_index as i64,
                id,
                operation,
                record_json
            ],
        )
        .context("failed to append NOTAM publication operation")?;
    }
    write_projection_metadata(tx, &to_state_id, counters)?;
    Ok((changed_count, removed_count))
}

fn read_projection_counters(tx: &Transaction<'_>) -> anyhow::Result<NotamCounters> {
    Ok(NotamCounters {
        notam_count: read_metadata_u64(tx, NOTAM_COUNT_METADATA_KEY)?,
        airport_notam_count: read_metadata_u64(tx, AIRPORT_NOTAM_COUNT_METADATA_KEY)?,
        airport_notams_with_multiple_effects: read_metadata_u64(
            tx,
            MULTIPLE_EFFECT_COUNT_METADATA_KEY,
        )?,
        airport_notams_with_other_effect: read_metadata_u64(tx, OTHER_EFFECT_COUNT_METADATA_KEY)?,
    })
}

fn write_projection_metadata(
    tx: &Transaction<'_>,
    state_id: &str,
    counters: NotamCounters,
) -> anyhow::Result<()> {
    for (key, value) in [
        (STATE_ID_METADATA_KEY, state_id.to_string()),
        (NOTAM_COUNT_METADATA_KEY, counters.notam_count.to_string()),
        (
            AIRPORT_NOTAM_COUNT_METADATA_KEY,
            counters.airport_notam_count.to_string(),
        ),
        (
            MULTIPLE_EFFECT_COUNT_METADATA_KEY,
            counters.airport_notams_with_multiple_effects.to_string(),
        ),
        (
            OTHER_EFFECT_COUNT_METADATA_KEY,
            counters.airport_notams_with_other_effect.to_string(),
        ),
    ] {
        tx.execute(
            "INSERT INTO metadata(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .with_context(|| format!("failed to write NOTAM metadata {key}"))?;
    }
    Ok(())
}

fn read_metadata(tx: &Transaction<'_>, key: &str) -> anyhow::Result<Option<String>> {
    tx.query_row("SELECT value FROM metadata WHERE key = ?1", [key], |row| {
        row.get(0)
    })
    .optional()
    .with_context(|| format!("failed to read NOTAM metadata {key}"))
}

fn read_metadata_u64(tx: &Transaction<'_>, key: &str) -> anyhow::Result<u64> {
    read_metadata(tx, key)?
        .with_context(|| format!("NOTAM projection is missing metadata {key}"))?
        .parse()
        .with_context(|| format!("failed to parse NOTAM metadata {key}"))
}

fn read_ordered_hashes(
    tx: &Transaction<'_>,
    sql: &str,
    expected_count: usize,
    label: &str,
) -> anyhow::Result<Vec<NotamHash>> {
    let mut statement = tx
        .prepare(sql)
        .with_context(|| format!("failed to prepare {label} query"))?;
    let rows = statement
        .query_map([], |row| row.get::<_, Vec<u8>>(0))
        .with_context(|| format!("failed to query {label}"))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to read {label}"))?;
    if rows.len() != expected_count {
        bail!("{label} has {} rows; expected {expected_count}", rows.len());
    }
    rows.iter().map(|row| decode_hash(row)).collect()
}

fn decode_hash(bytes: &[u8]) -> anyhow::Result<NotamHash> {
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("NOTAM Merkle hash has {} bytes; expected 32", bytes.len()))
}

fn counter_contribution(record: &NotamRecord) -> NotamCounters {
    let airport = record
        .airport_id
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    NotamCounters {
        notam_count: 1,
        airport_notam_count: u64::from(airport),
        airport_notams_with_multiple_effects: u64::from(
            airport && record.airport_effects.len() > 1,
        ),
        airport_notams_with_other_effect: u64::from(
            airport
                && record
                    .airport_effects
                    .contains(&product_contracts::AirportNotamEffect::Other),
        ),
    }
}

fn add_counter_contribution(counters: &mut NotamCounters, record: &NotamRecord) {
    let contribution = counter_contribution(record);
    counters.notam_count += contribution.notam_count;
    counters.airport_notam_count += contribution.airport_notam_count;
    counters.airport_notams_with_multiple_effects +=
        contribution.airport_notams_with_multiple_effects;
    counters.airport_notams_with_other_effect += contribution.airport_notams_with_other_effect;
}

fn subtract_counter_contribution(
    counters: &mut NotamCounters,
    record: &NotamRecord,
) -> anyhow::Result<()> {
    let contribution = counter_contribution(record);
    counters.notam_count = counters
        .notam_count
        .checked_sub(contribution.notam_count)
        .context("NOTAM count underflow")?;
    counters.airport_notam_count = counters
        .airport_notam_count
        .checked_sub(contribution.airport_notam_count)
        .context("airport NOTAM count underflow")?;
    counters.airport_notams_with_multiple_effects = counters
        .airport_notams_with_multiple_effects
        .checked_sub(contribution.airport_notams_with_multiple_effects)
        .context("multiple-effect NOTAM count underflow")?;
    counters.airport_notams_with_other_effect = counters
        .airport_notams_with_other_effect
        .checked_sub(contribution.airport_notams_with_other_effect)
        .context("other-effect NOTAM count underflow")?;
    Ok(())
}

impl Drop for NotamStoreLock {
    fn drop(&mut self) {
        let _ = unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
        let _ = self.file.sync_all();
        let _ = &self.path;
    }
}

fn raw_ingest_cursor(connection: &Connection) -> anyhow::Result<i64> {
    let value: Option<String> = connection
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [RAW_INGEST_CURSOR_METADATA_KEY],
            |row| row.get(0),
        )
        .optional()
        .context("failed to query NOTAM raw ingest cursor")?;
    value
        .as_deref()
        .unwrap_or("0")
        .parse::<i64>()
        .context("failed to parse NOTAM raw ingest cursor")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn canonical_state_sync_journals_updates_and_removals() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        let first = structured_notam_record_from_json(&captured_notam_variant(
            "PUBLISHED",
            "NOTAMN",
            Some("RWY"),
            "1",
            Some("N"),
            "RWY 01 CLSD.",
        ))?
        .context("missing first canonical NOTAM")?;
        let second = structured_notam_record_from_json(&captured_notam_variant(
            "PUBLISHED",
            "NOTAMN",
            Some("TWY"),
            "2",
            Some("N"),
            "TWY A CLSD.",
        ))?
        .context("missing second canonical NOTAM")?;

        let initial =
            store.synchronize_current_records(&[first.clone(), second], "2026-07-24T12:00:00Z")?;
        assert_eq!(initial.changed_count, 2);
        assert_eq!(initial.removed_count, 0);

        let mut updated = first;
        updated.text = Some("RWY 01 CLSD. TWY B CLSD.".to_string());
        updated = canonicalize_structured_notam_record(updated)?;
        let next = store
            .synchronize_current_records(std::slice::from_ref(&updated), "2026-07-24T12:03:00Z")?;
        assert_eq!(next.changed_count, 1);
        assert_eq!(next.removed_count, 1);

        let snapshot = store.publication_snapshot()?;
        assert_eq!(snapshot.transitions.len(), 2);
        let mut replayed = NotamState::empty();
        let mut work = Default::default();
        for transition in snapshot.transitions {
            for mutation in transition.mutations {
                replayed
                    .apply_mutation(mutation, &mut work)
                    .map_err(anyhow::Error::msg)?;
            }
        }
        assert_eq!(replayed.state_id(), next.state_id);
        assert_eq!(replayed.checkpoint(), store.current_checkpoint()?);
        assert_eq!(
            replayed.checkpoint().records,
            vec![published_notam_record(&updated)]
        );
        Ok(())
    }

    #[test]
    fn lock_prevents_second_notam_consumer_in_same_store() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize()?;

        let _lock = store.acquire_lock()?;
        let error = store.acquire_lock().expect_err("second lock must fail");

        assert!(
            format!("{error:#}").contains("already locked"),
            "unexpected error: {error:#}"
        );
        Ok(())
    }

    #[test]
    fn committed_raw_messages_advance_projection_cursor() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize()?;
        store.insert_raw_message_for_test(
            "message-a",
            "2026-07-11T02:03:00Z",
            &captured_notam_line("PUBLISHED"),
        )?;

        let summary = store
            .apply_pending_raw_messages(10)?
            .expect("raw row should apply");

        assert_eq!(summary.applied_count, 1);
        assert_eq!(summary.changed_count, 1);
        assert_eq!(summary.removed_count, 0);
        let records = store.current_records()?;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "D:AAA:2026:N:1");
        assert_eq!(records[0].notam_status.as_deref(), Some("ACTIVE"));
        assert_eq!(records[0].notam_function.as_deref(), Some("NOTAMN"));
        assert_eq!(records[0].airport_id.as_deref(), Some("KAAA"));
        assert!(store.apply_pending_raw_messages(10)?.is_none());
        Ok(())
    }

    #[test]
    fn projection_journal_replays_exactly_through_shared_state() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize()?;
        let initial = store.current_checkpoint()?;

        store.insert_raw_message_for_test(
            "message-a",
            "2026-07-11T02:03:00Z",
            &captured_notam_line("PUBLISHED"),
        )?;
        store.apply_pending_raw_messages(10)?;
        store.insert_raw_message_for_test(
            "message-b",
            "2026-07-11T02:04:00Z",
            &captured_notam_line("CANCELLED"),
        )?;
        store.apply_pending_raw_messages(10)?;

        let transitions = store.pending_publication_transitions()?;
        assert_eq!(transitions.len(), 2);
        assert_eq!(transitions[0].mutations.len(), 1);
        assert!(matches!(
            transitions[0].mutations[0],
            NotamMutation::Upsert { .. }
        ));
        assert!(matches!(
            transitions[1].mutations[0],
            NotamMutation::Remove { .. }
        ));
        assert_eq!(transitions[0].to_state_id, transitions[1].from_state_id);

        let mut replayed =
            NotamState::from_checkpoint(initial, &mut notam_state::NotamApplyWork::default())
                .map_err(anyhow::Error::msg)?;
        for transition in &transitions {
            replayed
                .apply_delta(
                    notam_state::NotamDelta::new(
                        transition.from_state_id.clone(),
                        transition.to_state_id.clone(),
                        transition.counters,
                        transition.mutations.clone(),
                    ),
                    &mut notam_state::NotamApplyWork::default(),
                )
                .map_err(anyhow::Error::msg)?;
        }
        let current = store.current_checkpoint()?;
        assert_eq!(replayed.state_id(), current.state_id);
        assert_eq!(replayed.counters(), current.counters);
        assert_eq!(
            replayed
                .canonical_records()
                .map(|(_, record)| record.clone())
                .collect::<Vec<_>>(),
            current.records
        );

        store.advance_publication_cursor(
            transitions[1].journal_seq,
            None,
            &transitions[1].to_state_id,
        )?;
        assert!(store.pending_publication_transitions()?.is_empty());
        assert_eq!(
            store.publication_cursor()?.published_head_state_id,
            Some(transitions[1].to_state_id.clone())
        );
        store.insert_raw_message_for_test(
            "message-c",
            "2026-07-11T02:05:00Z",
            &captured_notam_line("PUBLISHED"),
        )?;
        store.apply_pending_raw_messages(10)?;
        assert_eq!(
            store.prune_published_journal_before("9999-01-01T00:00:00Z")?,
            2
        );
        assert_eq!(store.pending_publication_transitions()?.len(), 1);
        let connection = Connection::open(store.sqlite_path())?;
        assert_eq!(
            connection.query_row(
                "SELECT COUNT(*) FROM notam_publication_journal",
                [],
                |row| { row.get::<_, i64>(0) }
            )?,
            1
        );
        assert_eq!(
            connection.query_row(
                "SELECT COUNT(*) FROM notam_publication_operations",
                [],
                |row| row.get::<_, i64>(0)
            )?,
            1
        );
        Ok(())
    }

    #[test]
    fn rejected_raw_message_does_not_block_later_messages() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize()?;
        store.insert_raw_message_for_test(
            "message-a",
            "2026-07-11T02:03:00Z",
            &captured_notam_variant(
                "PUBLISHED",
                "NOTAMN",
                Some("RWY"),
                "1",
                Some("N"),
                "RWY 01 CLSD.",
            ),
        )?;
        store.insert_raw_message_for_test(
            "message-b",
            "2026-07-11T02:04:00Z",
            &captured_notam_variant(
                "PUBLISHED",
                "NOTAMN",
                Some("RWY"),
                "2",
                Some("UNEXPECTED"),
                "RWY 02 CLSD.",
            ),
        )?;
        store.insert_raw_message_for_test(
            "message-c",
            "2026-07-11T02:05:00Z",
            &captured_notam_variant(
                "PUBLISHED",
                "NOTAMN",
                Some("RWY"),
                "3",
                Some("N"),
                "RWY 03 CLSD.",
            ),
        )?;

        let summary = store
            .apply_pending_raw_messages(10)?
            .expect("raw rows should be consumed");

        assert_eq!(summary.applied_count, 3);
        assert_eq!(summary.changed_count, 2);
        assert_eq!(summary.rejected_count, 1);
        assert_eq!(summary.new_rejected_count, 1);
        assert_eq!(summary.max_ingest_seq, 3);
        assert_eq!(
            store
                .current_records()?
                .into_iter()
                .map(|record| record.id)
                .collect::<Vec<_>>(),
            vec!["D:AAA:2026:N:1", "D:AAA:2026:N:3"]
        );
        assert!(store.apply_pending_raw_messages(10)?.is_none());
        let rejection = store.rejection_status()?;
        assert_eq!(rejection.unresolved_count, 1);
        assert_eq!(rejection.oldest_unresolved_ingest_seq, Some(2));
        assert_eq!(rejection.latest_unresolved_ingest_seq, Some(2));
        assert_eq!(
            rejection.last_error.as_deref(),
            Some("unsupported NOTAM type UNEXPECTED")
        );
        assert_eq!(rejection.recent_rejections.len(), 1);
        assert_eq!(rejection.recent_rejections[0].ingest_seq, 2);
        assert_eq!(
            rejection.recent_rejections[0].error,
            "unsupported NOTAM type UNEXPECTED"
        );
        Ok(())
    }

    #[test]
    fn repaired_rejection_does_not_override_later_identity_update() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize()?;
        store.insert_raw_message_for_test(
            "message-a",
            "2026-07-11T02:03:00Z",
            &captured_notam_variant(
                "PUBLISHED",
                "NOTAMN",
                Some("RWY"),
                "9",
                Some("N"),
                "RWY 09 CLSD.",
            ),
        )?;
        store.insert_raw_message_for_test(
            "message-b",
            "2026-07-11T02:04:00Z",
            &captured_notam_variant(
                "CANCELLED",
                "NOTAMN",
                Some("RWY"),
                "9",
                Some("N"),
                "RWY 09 CLSD.",
            ),
        )?;
        let connection = Connection::open(store.sqlite_path())?;
        connection.execute(
            "INSERT INTO metadata(key, value) VALUES (?1, '2')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [RAW_INGEST_CURSOR_METADATA_KEY],
        )?;
        connection.execute(
            "UPDATE raw_notam_messages SET applied_at_utc = '2026-07-11T02:05:00Z'",
            [],
        )?;
        connection.execute(
            "INSERT INTO rejected_notam_messages (
                ingest_seq, first_rejected_at_utc, last_rejected_at_utc,
                resolved_at_utc, rejection_count, error
             ) VALUES (1, '2026-07-11T02:05:00Z', '2026-07-11T02:05:00Z', NULL, 1, 'old parser rejection')",
            [],
        )?;
        connection.execute(
            "INSERT INTO notam_identity_cursors(id, ingest_seq)
             VALUES ('D:AAA:2026:N:9', 2)",
            [],
        )?;
        drop(connection);

        let summary = store
            .retry_rejected_raw_messages()?
            .expect("repaired row should retry");

        assert_eq!(summary.retried_count, 1);
        assert_eq!(summary.resolved_count, 1);
        assert_eq!(summary.applied_count, 0);
        assert_eq!(summary.superseded_count, 1);
        assert_eq!(summary.changed_count, 0);
        assert_eq!(summary.removed_count, 0);
        assert!(store.current_records()?.is_empty());
        assert_eq!(store.rejection_status()?.unresolved_count, 0);
        Ok(())
    }

    #[test]
    fn repaired_rejection_applies_when_not_superseded() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize()?;
        store.insert_raw_message_for_test(
            "message-a",
            "2026-07-11T02:03:00Z",
            &captured_notam_variant(
                "PUBLISHED",
                "NOTAMN",
                Some("RWY"),
                "9",
                Some("N"),
                "RWY 09 CLSD.",
            ),
        )?;
        let connection = Connection::open(store.sqlite_path())?;
        connection.execute(
            "INSERT INTO metadata(key, value) VALUES (?1, '1')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [RAW_INGEST_CURSOR_METADATA_KEY],
        )?;
        connection.execute(
            "UPDATE raw_notam_messages SET applied_at_utc = '2026-07-11T02:05:00Z'",
            [],
        )?;
        connection.execute(
            "INSERT INTO rejected_notam_messages (
                ingest_seq, first_rejected_at_utc, last_rejected_at_utc,
                resolved_at_utc, rejection_count, error
             ) VALUES (1, '2026-07-11T02:05:00Z', '2026-07-11T02:05:00Z', NULL, 1, 'old parser rejection')",
            [],
        )?;
        drop(connection);

        let summary = store
            .retry_rejected_raw_messages()?
            .expect("repaired row should retry");

        assert_eq!(summary.resolved_count, 1);
        assert_eq!(summary.applied_count, 1);
        assert_eq!(summary.superseded_count, 0);
        assert_eq!(summary.changed_count, 1);
        assert_eq!(store.current_records()?[0].id, "D:AAA:2026:N:9");
        assert_eq!(store.rejection_status()?.unresolved_count, 0);
        Ok(())
    }

    #[test]
    fn raw_retention_preserves_only_unresolved_rejections() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize()?;
        for index in 1..=3 {
            store.insert_raw_message_for_test(
                &format!("message-{index}"),
                "2000-01-01T00:00:00Z",
                &captured_notam_variant(
                    "PUBLISHED",
                    "NOTAMN",
                    Some("RWY"),
                    &index.to_string(),
                    Some("N"),
                    "RWY CLSD.",
                ),
            )?;
        }
        let connection = Connection::open(store.sqlite_path())?;
        connection.execute(
            "INSERT INTO metadata(key, value) VALUES (?1, '3')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [RAW_INGEST_CURSOR_METADATA_KEY],
        )?;
        connection.execute(
            "UPDATE raw_notam_messages SET applied_at_utc = '2000-01-01T00:00:00Z'",
            [],
        )?;
        connection.execute(
            "INSERT INTO rejected_notam_messages (
                ingest_seq, first_rejected_at_utc, last_rejected_at_utc,
                resolved_at_utc, rejection_count, error
             ) VALUES (2, '2000-01-01T00:00:00Z', '2000-01-01T00:00:00Z', NULL, 1, 'old parser rejection')",
            [],
        )?;
        drop(connection);

        store.prune_applied_raw_messages()?;

        let connection = Connection::open(store.sqlite_path())?;
        let retained = connection
            .prepare("SELECT ingest_seq FROM raw_notam_messages ORDER BY ingest_seq")?
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(retained, vec![2]);
        Ok(())
    }

    #[test]
    fn committed_cancelled_notam_deletes_current_record() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize()?;
        let published_json = captured_notam_line("PUBLISHED");
        let cancelled_json = captured_notam_line("CANCELLED");
        store.insert_raw_message_for_test("message-a", "2026-07-11T02:03:00Z", &published_json)?;
        store.apply_pending_raw_messages(10)?;
        assert_eq!(store.current_records()?.len(), 1);

        store.insert_raw_message_for_test("message-b", "2026-07-11T02:04:00Z", &cancelled_json)?;

        let summary = store
            .apply_pending_raw_messages(10)?
            .expect("cancellation row should apply");

        assert_eq!(summary.applied_count, 1);
        assert_eq!(summary.changed_count, 0);
        assert_eq!(summary.removed_count, 1);
        assert!(store.current_records()?.is_empty());
        let connection = Connection::open(store.sqlite_path())?;
        let raw_messages = connection
            .prepare("SELECT message_json FROM raw_notam_messages ORDER BY ingest_seq")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(raw_messages, vec![published_json, cancelled_json]);
        Ok(())
    }

    #[test]
    fn cancellation_uses_same_identity_across_transport_dialects() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize()?;
        store.insert_raw_message_for_test(
            "message-a",
            "2026-07-11T02:03:00Z",
            &captured_notam_variant(
                "PUBLISHED",
                "N",
                Some("RWY"),
                "001",
                Some("N"),
                "RWY 01 CLSD.",
            ),
        )?;
        store.apply_pending_raw_messages(10)?;
        assert_eq!(store.current_records()?[0].id, "D:AAA:2026:N:1");

        store.insert_raw_message_for_test(
            "message-b",
            "2026-07-11T02:04:00Z",
            &captured_notam_variant(
                "CANCELLED",
                "NOTAMN",
                Some("RWY"),
                "1",
                None,
                "RWY 01 CLSD.",
            ),
        )?;
        let summary = store
            .apply_pending_raw_messages(10)?
            .expect("cancellation row should apply");
        assert_eq!(summary.removed_count, 1);
        assert!(store.current_records()?.is_empty());
        Ok(())
    }

    #[test]
    fn replacement_type_cancellation_uses_canonical_new_notam_identity() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize()?;
        store.insert_raw_message_for_test(
            "message-a",
            "2026-07-11T02:03:00Z",
            &captured_notam_variant(
                "PUBLISHED",
                "N",
                Some("TWY"),
                "057",
                Some("N"),
                "TWY K HOLD PAD CLSD.",
            ),
        )?;
        store.apply_pending_raw_messages(10)?;
        assert_eq!(store.current_records()?[0].id, "D:AAA:2026:N:57");

        store.insert_raw_message_for_test(
            "message-b",
            "2026-07-11T02:04:00Z",
            &captured_notam_variant(
                "CANCELLED",
                "N",
                Some("TWY"),
                "057",
                Some("R"),
                "TWY K HOLD PAD CLSD.",
            ),
        )?;
        let summary = store
            .apply_pending_raw_messages(10)?
            .expect("replacement-type cancellation row should apply");

        assert_eq!(summary.removed_count, 1);
        assert!(store.current_records()?.is_empty());
        Ok(())
    }

    #[test]
    fn domestic_keyword_comes_from_body_when_transport_is_missing_or_wrong() -> anyhow::Result<()> {
        let missing = structured_notam_record_from_json(&captured_notam_variant(
            "ACTIVE",
            "NOTAMN",
            None,
            "2",
            Some("N"),
            "AD AP CLSD.",
        ))?
        .expect("record");
        assert_eq!(missing.notam_keyword.as_deref(), Some("AD"));

        let conflicting = structured_notam_record_from_json(&captured_notam_variant(
            "ACTIVE",
            "NOTAMN",
            Some("RWY"),
            "3",
            Some("N"),
            "NAV ILS RWY 33L U/S.",
        ))?
        .expect("record");
        assert_eq!(conflicting.notam_keyword.as_deref(), Some("NAV"));
        assert_eq!(conflicting.airport_id, None);
        Ok(())
    }

    #[test]
    fn schema_v4_migration_adds_rejection_tracking() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize()?;
        let connection = Connection::open(store.sqlite_path())?;
        connection.execute_batch(
            "DROP TABLE rejected_notam_messages;
             DROP TABLE notam_identity_cursors;
             UPDATE metadata SET value = '4' WHERE key = 'schema_version';",
        )?;
        drop(connection);

        store.initialize()?;

        let connection = Connection::open(store.sqlite_path())?;
        assert_eq!(
            connection.query_row(
                "SELECT value FROM metadata WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )?,
            NOTAM_STORE_SCHEMA_VERSION.to_string()
        );
        for table in ["rejected_notam_messages", "notam_identity_cursors"] {
            assert_eq!(
                connection.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get::<_, i64>(0),
                )?,
                1
            );
        }
        Ok(())
    }

    #[test]
    fn schema_v8_is_rejected_after_procedure_identity_contract_roll() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize()?;
        let connection = Connection::open(store.sqlite_path())?;
        connection.execute(
            "UPDATE metadata SET value = '8' WHERE key = 'schema_version'",
            [],
        )?;
        drop(connection);

        let error = store.initialize().unwrap_err().to_string();
        assert!(error.contains("unsupported NOTAM sqlite schema 8; required 9"));
        Ok(())
    }

    #[test]
    fn schema_v6_migration_rebuilds_state_for_the_current_contract() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize()?;
        store.insert_raw_message_for_test(
            "message-a",
            "2026-07-11T02:03:00Z",
            &captured_notam_line("PUBLISHED"),
        )?;
        store.apply_pending_raw_messages(10)?;
        let transition = store.pending_publication_transitions()?.remove(0);
        store.advance_publication_cursor(transition.journal_seq, None, &transition.to_state_id)?;

        let connection = Connection::open(store.sqlite_path())?;
        connection.execute(
            "UPDATE metadata SET value = '6' WHERE key = 'schema_version'",
            [],
        )?;
        connection.execute(
            "ALTER TABLE notam_publication_journal DROP COLUMN published_at_utc",
            [],
        )?;
        drop(connection);

        store.initialize()?;

        assert_eq!(store.publication_cursor()?.published_head_state_id, None);
        assert!(store.pending_publication_transitions()?.is_empty());
        let connection = Connection::open(store.sqlite_path())?;
        assert_eq!(
            connection.query_row(
                "SELECT value FROM metadata WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )?,
            NOTAM_STORE_SCHEMA_VERSION.to_string()
        );
        assert_eq!(
            connection.query_row(
                "SELECT COUNT(*) FROM pragma_table_info('notam_publication_journal')
                 WHERE name = 'published_at_utc'",
                [],
                |row| row.get::<_, i64>(0),
            )?,
            1
        );
        assert_eq!(
            connection.query_row("SELECT COUNT(*) FROM notam_client_records", [], |row| row
                .get::<_, i64>(
                0
            ),)?,
            1
        );
        Ok(())
    }

    #[test]
    fn schema_v2_reprojection_replays_canonical_cancellation() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize()?;
        store.insert_raw_message_for_test(
            "message-a",
            "2026-07-11T02:03:00Z",
            &captured_notam_variant(
                "PUBLISHED",
                "N",
                Some("RWY"),
                "001",
                Some("N"),
                "RWY 01 CLSD.",
            ),
        )?;
        store.insert_raw_message_for_test(
            "message-b",
            "2026-07-11T02:04:00Z",
            &captured_notam_variant(
                "CANCELLED",
                "NOTAMN",
                Some("RWY"),
                "1",
                None,
                "RWY 01 CLSD.",
            ),
        )?;

        let connection = Connection::open(store.sqlite_path())?;
        let mut stale =
            structured_notam_record_from_json(&captured_notam_line("PUBLISHED"))?.expect("record");
        stale.id = "D:AAA:2026:N:001".to_string();
        stale.notam_number = Some("001".to_string());
        stale.notam_status = Some("PUBLISHED".to_string());
        stale.notam_function = Some("N".to_string());
        connection.execute(
            "INSERT INTO current_notams (
                id, status, last_updated_utc, record_json, updated_at_utc
             ) VALUES (?1, ?2, NULL, ?3, ?4)",
            params![
                stale.id,
                stale.notam_status,
                serde_json::to_string(&stale)?,
                "2026-07-11T02:03:00Z",
            ],
        )?;
        connection.execute(
            "UPDATE metadata SET value = '2' WHERE key = 'schema_version'",
            [],
        )?;
        drop(connection);

        assert!(store.current_records()?.is_empty());
        let connection = Connection::open(store.sqlite_path())?;
        assert_eq!(
            connection.query_row(
                "SELECT value FROM metadata WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )?,
            NOTAM_STORE_SCHEMA_VERSION.to_string()
        );
        assert_eq!(raw_ingest_cursor(&connection)?, 2);
        Ok(())
    }

    #[test]
    fn schema_v3_reprojection_adds_airport_effects_to_current_records() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize()?;
        let record =
            structured_notam_record_from_json(&captured_notam_line("PUBLISHED"))?.expect("record");
        let mut stale_json = serde_json::to_value(&record)?;
        stale_json
            .as_object_mut()
            .expect("record object")
            .remove("airport_effects");
        let connection = Connection::open(store.sqlite_path())?;
        connection.execute(
            "INSERT INTO current_notams (
                id, status, last_updated_utc, record_json, updated_at_utc
             ) VALUES (?1, ?2, NULL, ?3, ?4)",
            params![
                record.id,
                record.notam_status,
                serde_json::to_string(&stale_json)?,
                "2026-07-11T02:03:00Z",
            ],
        )?;
        connection.execute(
            "UPDATE metadata SET value = '3' WHERE key = 'schema_version'",
            [],
        )?;
        drop(connection);

        let records = store.current_records()?;
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].airport_effects,
            std::collections::BTreeSet::from([
                product_contracts::AirportNotamEffect::RunwayClosed,
                product_contracts::AirportNotamEffect::SurfaceCondition,
            ])
        );
        let connection = Connection::open(store.sqlite_path())?;
        assert_eq!(
            connection.query_row(
                "SELECT value FROM metadata WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )?,
            NOTAM_STORE_SCHEMA_VERSION.to_string()
        );
        Ok(())
    }

    fn captured_notam_line(status: &str) -> String {
        captured_notam_variant(
            status,
            "NOTAMN",
            Some("RWY"),
            "1",
            Some("N"),
            "RWY 01 CLSD.",
        )
    }

    fn captured_notam_variant(
        status: &str,
        function: &str,
        keyword: Option<&str>,
        number: &str,
        notam_type: Option<&str>,
        text: &str,
    ) -> String {
        let notam_type = notam_type
            .map(|value| format!("<event:type>{value}</event:type>"))
            .unwrap_or_default();
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<AIXMBasicMessage xmlns:event="http://www.aixm.aero/schema/5.1/event">
  <hasMember>
    <event:Event>
      <event:timeSlice>
        <event:EventTimeSlice>
          <event:scenario>95</event:scenario>
          <event:textNOTAM>
            <event:NOTAM>
              <event:number>{number}</event:number>
              <event:year>2026</event:year>
              {notam_type}
              <event:issued>2026-07-11T02:00:00.000Z</event:issued>
              <event:location>AAA</event:location>
              <event:effectiveStart>202607110200</event:effectiveStart>
              <event:effectiveEnd>202607111200</event:effectiveEnd>
              <event:text>{text}</event:text>
            </event:NOTAM>
          </event:textNOTAM>
        </event:EventTimeSlice>
      </event:timeSlice>
    </event:Event>
  </hasMember>
</AIXMBasicMessage>"#
        );
        let mut properties = serde_json::Map::from_iter([
            (
                "us_gov_dot_faa_aim_fns_nds_SourceType".to_string(),
                serde_json::json!("D"),
            ),
            (
                "us_gov_dot_faa_aim_fns_nds_ICAOId".to_string(),
                serde_json::json!("KAAA"),
            ),
            (
                "us_gov_dot_faa_aim_fns_nds_LocationDesignator".to_string(),
                serde_json::json!("AAA"),
            ),
            (
                "us_gov_dot_faa_aim_fns_nds_NOTAMStatus".to_string(),
                serde_json::json!(status),
            ),
            (
                "us_gov_dot_faa_aim_fns_nds_NOTAMFunction".to_string(),
                serde_json::json!(function),
            ),
        ]);
        if let Some(keyword) = keyword {
            properties.insert(
                "us_gov_dot_faa_aim_fns_nds_NOTAMKeyword".to_string(),
                serde_json::json!(keyword),
            );
        }
        serde_json::json!({
            "jmsMessageId": "ID:test",
            "receivedAtUtc": "2026-07-11T02:03:00Z",
            "properties": properties,
            "bodyText": xml
        })
        .to_string()
    }
}
