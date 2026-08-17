// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    thread,
    time::Instant,
};

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;

use super::{
    file_size, object_storage_bytes, query_strings, release_account_usage, remove_file_if_present,
    CloudStore, StoreError, StoreResult,
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GcReport {
    pub marked_objects: u64,
    pub deleted_objects: u64,
    pub deleted_ciphertext_bytes: u64,
    pub deleted_blob_files: u64,
    /// Longest contiguous hold of the database writer during this run.
    pub database_pause_ms: u64,
    /// Sum of all database-writer holds during this run.
    pub database_work_ms: u64,
    pub total_elapsed_ms: u64,
}

impl CloudStore {
    pub fn run_gc(&self, now_epoch_ms: i64, grace_ms: i64) -> StoreResult<GcReport> {
        let _reclamation_lock = self.inner.layout.acquire_reclamation_lock()?;
        let started = Instant::now();
        let mut report = self.run_gc_inner(now_epoch_ms, grace_ms)?;
        report.total_elapsed_ms = elapsed_ms(started);
        let counters = &self.inner.counters;
        counters.gc_runs.fetch_add(1, Ordering::Relaxed);
        counters
            .gc_last_database_pause_ms
            .store(report.database_pause_ms, Ordering::Relaxed);
        counters
            .gc_peak_database_pause_ms
            .fetch_max(report.database_pause_ms, Ordering::Relaxed);
        counters
            .gc_total_database_work_ms
            .fetch_add(report.database_work_ms, Ordering::Relaxed);
        counters
            .gc_last_elapsed_ms
            .store(report.total_elapsed_ms, Ordering::Relaxed);
        counters
            .gc_peak_elapsed_ms
            .fetch_max(report.total_elapsed_ms, Ordering::Relaxed);
        Ok(report)
    }

    fn run_gc_inner(&self, now_epoch_ms: i64, grace_ms: i64) -> StoreResult<GcReport> {
        self.run_gc_inner_with_progress(now_epoch_ms, grace_ms, |_| {})
    }

    pub(super) fn run_gc_inner_with_progress(
        &self,
        now_epoch_ms: i64,
        grace_ms: i64,
        mut account_complete: impl FnMut(&str),
    ) -> StoreResult<GcReport> {
        let cutoff = now_epoch_ms - grace_ms;
        let accounts = {
            let connection = self.read_connection()?;
            query_strings(
                &connection,
                "SELECT account_locator FROM accounts ORDER BY account_locator",
                [],
            )?
        };
        let mut report = GcReport {
            marked_objects: 0,
            deleted_objects: 0,
            deleted_ciphertext_bytes: 0,
            deleted_blob_files: 0,
            database_pause_ms: 0,
            database_work_ms: 0,
            total_elapsed_ms: 0,
        };
        let mut blob_keys = Vec::new();
        for account in accounts {
            let mut connection = self.connection()?;
            let database_pause_started = Instant::now();
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(StoreError::sqlite)?;
            let object_rows = load_object_graph(&transaction, &account)?;
            let root_children = transaction
                .query_row(
                    "SELECT children_json FROM roots WHERE account_locator = ?1",
                    [&account],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(StoreError::sqlite)?
                .map(|json| decode_children(&json))
                .transpose()?
                .unwrap_or_default();
            let mut marked = BTreeSet::new();
            let mut pending = VecDeque::from(root_children);
            while let Some(object_id) = pending.pop_front() {
                if !marked.insert(object_id.clone()) {
                    continue;
                }
                if let Some(object) = object_rows.get(&object_id) {
                    pending.extend(object.children.iter().cloned());
                }
            }
            report.marked_objects += marked.len() as u64;
            for (object_id, object) in object_rows {
                if marked.contains(&object_id) || object.created_at_epoch_ms > cutoff {
                    continue;
                }
                transaction
                    .execute(
                        "DELETE FROM objects WHERE account_locator = ?1 AND object_id = ?2",
                        params![account, object_id],
                    )
                    .map_err(StoreError::sqlite)?;
                release_account_usage(&transaction, &account, object.storage_bytes, 1)?;
                report.deleted_objects += 1;
                report.deleted_ciphertext_bytes += object.ciphertext_bytes;
                if let Some(key) = object.blob_storage_key {
                    blob_keys.push(key);
                }
            }
            transaction.commit().map_err(StoreError::sqlite)?;
            let database_work_ms = elapsed_ms(database_pause_started);
            report.database_pause_ms = report.database_pause_ms.max(database_work_ms);
            report.database_work_ms = report.database_work_ms.saturating_add(database_work_ms);
            drop(connection);
            account_complete(&account);
            thread::yield_now();
        }
        let referenced_blob_keys = {
            let connection = self.read_connection()?;
            query_strings(
                &connection,
                "SELECT blob_storage_key FROM objects WHERE blob_storage_key IS NOT NULL",
                [],
            )?
            .into_iter()
            .collect::<BTreeSet<_>>()
        };
        for key in blob_keys {
            if remove_file_if_present(&self.blob_path(&key))? {
                report.deleted_blob_files += 1;
            }
        }
        for path in blob_files(&self.inner.blob_root)? {
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let referenced = file_name
                .strip_suffix(".blob")
                .is_some_and(|key| referenced_blob_keys.contains(key));
            if referenced || file_modified_epoch_ms(&path).is_some_and(|modified| modified > cutoff)
            {
                continue;
            }
            let bytes = file_size(&path);
            if remove_file_if_present(&path)? {
                report.deleted_blob_files += 1;
                report.deleted_ciphertext_bytes += bytes;
            }
        }
        if report.deleted_objects > 0 {
            self.note_durable_write(now_epoch_ms);
        }
        Ok(report)
    }
}

#[derive(Debug)]
struct GcObject {
    children: Vec<String>,
    storage_bytes: u64,
    ciphertext_bytes: u64,
    blob_storage_key: Option<String>,
    created_at_epoch_ms: i64,
}

fn load_object_graph(
    transaction: &Transaction<'_>,
    account_locator: &str,
) -> StoreResult<BTreeMap<String, GcObject>> {
    let mut statement = transaction
        .prepare("SELECT object_id, children_json, ciphertext_bytes, blob_storage_key, created_at_epoch_ms FROM objects WHERE account_locator = ?1")
        .map_err(StoreError::sqlite)?;
    let rows = statement
        .query_map([account_locator], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u64>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(StoreError::sqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(StoreError::sqlite)?;
    rows.into_iter()
        .map(
            |(id, children, ciphertext_bytes, blob_storage_key, created_at_epoch_ms)| {
                let storage_bytes = object_storage_bytes(&id, ciphertext_bytes, &children);
                Ok((
                    id,
                    GcObject {
                        children: decode_children(&children)?,
                        storage_bytes,
                        ciphertext_bytes,
                        blob_storage_key,
                        created_at_epoch_ms,
                    },
                ))
            },
        )
        .collect()
}

fn decode_children(json: &str) -> StoreResult<Vec<String>> {
    serde_json::from_str(json)
        .map_err(|error| StoreError::internal(format!("decode visible object edges: {error}")))
}

fn blob_files(blob_root: &Path) -> StoreResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    for shard in
        fs::read_dir(blob_root).map_err(|error| StoreError::io("scan cloud blob root", error))?
    {
        let shard = shard.map_err(|error| StoreError::io("read cloud blob shard", error))?;
        if !shard
            .file_type()
            .map_err(|error| StoreError::io("read cloud blob shard type", error))?
            .is_dir()
        {
            continue;
        }
        for entry in fs::read_dir(shard.path())
            .map_err(|error| StoreError::io("scan cloud blob shard", error))?
        {
            let entry = entry.map_err(|error| StoreError::io("read cloud blob entry", error))?;
            if entry
                .file_type()
                .map_err(|error| StoreError::io("read cloud blob entry type", error))?
                .is_file()
            {
                files.push(entry.path());
            }
        }
    }
    Ok(files)
}

fn file_modified_epoch_ms(path: &Path) -> Option<i64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as i64)
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}
