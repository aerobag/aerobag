// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fs::{self, File, OpenOptions},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use rusqlite::{backup::Backup, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{store::SCHEMA_VERSION, StorageLayout, StoreConfig, StoreError, StoreResult};

const BACKUP_MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupBlob {
    pub storage_key: String,
    pub ciphertext_sha256: String,
    pub ciphertext_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupManifest {
    pub schema_version: u32,
    pub created_at_epoch_ms: i64,
    pub snapshot_name: String,
    pub source_database_schema_version: u32,
    pub database_sha256: String,
    pub database_bytes: u64,
    pub linked_blob_count: u64,
    pub linked_blob_bytes: u64,
    pub blobs: Vec<BackupBlob>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupReport {
    pub snapshot_name: String,
    pub snapshot_path: PathBuf,
    pub elapsed_ms: u64,
    pub sqlite_snapshot_ms: u64,
    pub wal_growth_bytes: u64,
    pub linked_blob_count: u64,
    pub linked_blob_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreReport {
    pub snapshot_name: String,
    pub previous_live_path: Option<PathBuf>,
    pub restored_database_bytes: u64,
    pub restored_blob_count: u64,
    pub restored_blob_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum BackupIfDueReport {
    Created {
        backup: BackupReport,
    },
    NotDue {
        last_completed_at_epoch_ms: i64,
        next_due_at_epoch_ms: i64,
    },
}

pub fn create_backup(config: &StoreConfig, now_epoch_ms: i64) -> StoreResult<BackupReport> {
    let layout = StorageLayout::new(config.storage_root.clone());
    layout.ensure()?;
    let _reclamation_lock = layout.acquire_reclamation_lock()?;
    create_backup_locked(config, &layout, now_epoch_ms)
}

pub fn create_backup_if_due(
    config: &StoreConfig,
    now_epoch_ms: i64,
) -> StoreResult<BackupIfDueReport> {
    let layout = StorageLayout::new(config.storage_root.clone());
    layout.ensure()?;
    let _reclamation_lock = layout.acquire_reclamation_lock()?;
    let last_completed_at_epoch_ms = last_completed_backup(&layout)?;
    if let Some(last_completed_at_epoch_ms) = last_completed_at_epoch_ms {
        let interval_ms = config.backup_interval_seconds.saturating_mul(1_000);
        let next_due_at_epoch_ms = last_completed_at_epoch_ms
            .saturating_add(i64::try_from(interval_ms).unwrap_or(i64::MAX));
        if now_epoch_ms < next_due_at_epoch_ms {
            return Ok(BackupIfDueReport::NotDue {
                last_completed_at_epoch_ms,
                next_due_at_epoch_ms,
            });
        }
    }
    Ok(BackupIfDueReport::Created {
        backup: create_backup_locked(config, &layout, now_epoch_ms)?,
    })
}

fn create_backup_locked(
    config: &StoreConfig,
    layout: &StorageLayout,
    now_epoch_ms: i64,
) -> StoreResult<BackupReport> {
    let started = Instant::now();
    let snapshot_name = format!("snapshot-{now_epoch_ms}");
    let partial = layout
        .snapshots_root()
        .join(format!(".{snapshot_name}.partial"));
    let completed = layout.snapshots_root().join(&snapshot_name);
    if partial.exists() || completed.exists() {
        return Err(StoreError::internal(format!(
            "cloud backup snapshot already exists: {snapshot_name}"
        )));
    }
    fs::create_dir(&partial)
        .map_err(|error| StoreError::io("create partial cloud backup", error))?;

    let result = create_backup_inner(
        layout,
        &snapshot_name,
        &partial,
        &completed,
        now_epoch_ms,
        started,
    );
    match result {
        Ok(mut report) => {
            if let Err(error) =
                prune_snapshots(layout, config.backup_retained_snapshots, &snapshot_name)
            {
                let _ = record_backup_failure(layout, now_epoch_ms);
                return Err(error);
            }
            report.elapsed_ms = elapsed_ms(started);
            record_backup_success(layout, &report, now_epoch_ms)?;
            Ok(report)
        }
        Err(error) => {
            let _ = record_backup_failure(layout, now_epoch_ms);
            Err(error)
        }
    }
}

fn last_completed_backup(layout: &StorageLayout) -> StoreResult<Option<i64>> {
    let connection = open_live_database(layout)?;
    connection
        .query_row(
            "SELECT last_completed_at_epoch_ms FROM backup_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(StoreError::sqlite)
}

fn create_backup_inner(
    layout: &StorageLayout,
    snapshot_name: &str,
    partial: &Path,
    completed: &Path,
    now_epoch_ms: i64,
    started: Instant,
) -> StoreResult<BackupReport> {
    let source_database = layout.database_path();
    let snapshot_database = partial.join("metadata.sqlite3");
    let wal_path = source_database.with_extension("sqlite3-wal");
    let wal_before = file_size(&wal_path);
    let sqlite_started = Instant::now();
    let source = Connection::open_with_flags(
        &source_database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(StoreError::sqlite)?;
    source
        .busy_timeout(Duration::from_secs(30))
        .map_err(StoreError::sqlite)?;
    source
        .execute_batch("BEGIN DEFERRED; SELECT schema_version FROM metadata LIMIT 1;")
        .map_err(StoreError::sqlite)?;
    let source_schema_version = source
        .query_row("SELECT schema_version FROM metadata LIMIT 1", [], |row| {
            row.get(0)
        })
        .map_err(StoreError::sqlite)?;
    if source_schema_version != SCHEMA_VERSION {
        return Err(StoreError::internal(format!(
            "cannot back up ACS schema {source_schema_version}; expected {SCHEMA_VERSION}"
        )));
    }
    let mut destination = Connection::open(&snapshot_database).map_err(StoreError::sqlite)?;
    {
        let backup = Backup::new(&source, &mut destination).map_err(StoreError::sqlite)?;
        backup
            .run_to_completion(128, Duration::from_millis(10), None)
            .map_err(StoreError::sqlite)?;
    }
    destination
        .execute_batch("PRAGMA journal_mode=DELETE;")
        .map_err(StoreError::sqlite)?;
    drop(destination);
    let blobs = query_ready_blobs(&source)?;
    source.execute_batch("COMMIT").map_err(StoreError::sqlite)?;
    drop(source);
    let sqlite_snapshot_ms = elapsed_ms(sqlite_started);
    let wal_growth_bytes = file_size(&wal_path).saturating_sub(wal_before);

    let snapshot_blob_root = partial.join("blobs");
    fs::create_dir(&snapshot_blob_root)
        .map_err(|error| StoreError::io("create cloud backup blob directory", error))?;
    let mut linked_blob_bytes = 0_u64;
    for blob in &blobs {
        let source_path = blob_path(&layout.blob_root(), &blob.storage_key);
        let destination_path = blob_path(&snapshot_blob_root, &blob.storage_key);
        if file_size(&source_path) != blob.ciphertext_bytes {
            return Err(StoreError::internal(format!(
                "ready cloud blob {} is missing or has the wrong size",
                blob.storage_key
            )));
        }
        if let Some(parent) = destination_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| StoreError::io("create cloud backup blob shard", error))?;
        }
        fs::hard_link(&source_path, &destination_path)
            .map_err(|error| StoreError::io("hard-link cloud backup blob", error))?;
        linked_blob_bytes = linked_blob_bytes.saturating_add(blob.ciphertext_bytes);
    }
    let database_bytes = file_size(&snapshot_database);
    let manifest = BackupManifest {
        schema_version: BACKUP_MANIFEST_SCHEMA_VERSION,
        created_at_epoch_ms: now_epoch_ms,
        snapshot_name: snapshot_name.to_string(),
        source_database_schema_version: source_schema_version,
        database_sha256: sha256_file(&snapshot_database)?,
        database_bytes,
        linked_blob_count: blobs.len().try_into().unwrap_or(u64::MAX),
        linked_blob_bytes,
        blobs,
    };
    write_json_sync(&partial.join("manifest.json"), &manifest)?;
    sync_tree_files(partial)?;
    sync_directory(partial)?;
    fs::rename(partial, completed)
        .map_err(|error| StoreError::io("publish completed cloud backup", error))?;
    sync_directory(&layout.snapshots_root())?;
    Ok(BackupReport {
        snapshot_name: snapshot_name.to_string(),
        snapshot_path: completed.to_path_buf(),
        elapsed_ms: elapsed_ms(started),
        sqlite_snapshot_ms,
        wal_growth_bytes,
        linked_blob_count: manifest.linked_blob_count,
        linked_blob_bytes,
    })
}

pub fn verify_backup(snapshot_path: &Path) -> StoreResult<BackupManifest> {
    let manifest_path = snapshot_path.join("manifest.json");
    let manifest: BackupManifest = serde_json::from_slice(
        &fs::read(&manifest_path)
            .map_err(|error| StoreError::io("read cloud backup manifest", error))?,
    )
    .map_err(|error| StoreError::internal(format!("decode cloud backup manifest: {error}")))?;
    if manifest.schema_version != BACKUP_MANIFEST_SCHEMA_VERSION {
        return Err(StoreError::internal(format!(
            "unsupported cloud backup manifest schema {}; expected {}",
            manifest.schema_version, BACKUP_MANIFEST_SCHEMA_VERSION
        )));
    }
    let database_path = snapshot_path.join("metadata.sqlite3");
    if file_size(&database_path) != manifest.database_bytes
        || sha256_file(&database_path)? != manifest.database_sha256
    {
        return Err(StoreError::internal("cloud backup database hash mismatch"));
    }
    let connection = Connection::open_with_flags(&database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(StoreError::sqlite)?;
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(StoreError::sqlite)?;
    if integrity != "ok" {
        return Err(StoreError::internal(format!(
            "cloud backup SQLite integrity check failed: {integrity}"
        )));
    }
    let schema_version: u32 = connection
        .query_row("SELECT schema_version FROM metadata LIMIT 1", [], |row| {
            row.get(0)
        })
        .map_err(StoreError::sqlite)?;
    if schema_version != manifest.source_database_schema_version || schema_version != SCHEMA_VERSION
    {
        return Err(StoreError::internal(format!(
            "cloud backup database schema {schema_version} does not match manifest or server schema"
        )));
    }
    let database_blobs = query_ready_blobs(&connection)?;
    if database_blobs != manifest.blobs {
        return Err(StoreError::internal(
            "cloud backup blob inventory does not match its database snapshot",
        ));
    }
    if manifest.linked_blob_count != manifest.blobs.len() as u64
        || manifest.linked_blob_bytes
            != manifest
                .blobs
                .iter()
                .map(|blob| blob.ciphertext_bytes)
                .sum::<u64>()
    {
        return Err(StoreError::internal(
            "cloud backup blob summary does not match its manifest",
        ));
    }
    for blob in &manifest.blobs {
        let path = blob_path(&snapshot_path.join("blobs"), &blob.storage_key);
        if file_size(&path) != blob.ciphertext_bytes
            || sha256_file(&path)? != blob.ciphertext_sha256
        {
            return Err(StoreError::internal(format!(
                "cloud backup blob {} failed verification",
                blob.storage_key
            )));
        }
    }
    Ok(manifest)
}

pub fn restore_backup(
    storage_root: &Path,
    snapshot_path: &Path,
    now_epoch_ms: i64,
) -> StoreResult<RestoreReport> {
    let layout = StorageLayout::new(storage_root.to_path_buf());
    fs::create_dir_all(layout.locks_root())
        .map_err(|error| StoreError::io("create cloud restore lock directory", error))?;
    let _serve_lock = layout.acquire_serve_lock()?;
    let _reclamation_lock = layout.acquire_reclamation_lock()?;
    let manifest = verify_backup(snapshot_path)?;
    fs::create_dir_all(layout.recovery_root())
        .map_err(|error| StoreError::io("create cloud recovery directory", error))?;
    let staged = storage_root.join(format!(".restore-{now_epoch_ms}.partial"));
    if staged.exists() {
        return Err(StoreError::internal(
            "cloud restore staging path already exists",
        ));
    }
    fs::create_dir(&staged)
        .map_err(|error| StoreError::io("create cloud restore staging directory", error))?;
    copy_file_sync(
        &snapshot_path.join("metadata.sqlite3"),
        &staged.join("cloud.sqlite3"),
    )?;
    for blob in &manifest.blobs {
        let source = blob_path(&snapshot_path.join("blobs"), &blob.storage_key);
        let destination = blob_path(&staged.join("blobs"), &blob.storage_key);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| StoreError::io("create restored blob shard", error))?;
        }
        copy_file_sync(&source, &destination)?;
    }
    sync_directory(&staged)?;
    let live = layout.live_root();
    let previous_live = if live.exists() {
        let path = layout
            .recovery_root()
            .join(format!("pre-restore-{now_epoch_ms}"));
        fs::rename(&live, &path)
            .map_err(|error| StoreError::io("preserve pre-restore cloud state", error))?;
        Some(path)
    } else {
        None
    };
    if let Err(error) = fs::rename(&staged, &live) {
        if let Some(previous) = &previous_live {
            let _ = fs::rename(previous, &live);
        }
        return Err(StoreError::io("activate restored cloud state", error));
    }
    sync_directory(storage_root)?;
    Ok(RestoreReport {
        snapshot_name: manifest.snapshot_name,
        previous_live_path: previous_live,
        restored_database_bytes: manifest.database_bytes,
        restored_blob_count: manifest.linked_blob_count,
        restored_blob_bytes: manifest.linked_blob_bytes,
    })
}

fn query_ready_blobs(connection: &Connection) -> StoreResult<Vec<BackupBlob>> {
    let mut statement = connection
        .prepare(
            "SELECT blob_storage_key, ciphertext_sha256, ciphertext_bytes FROM objects WHERE state = 'ready' AND blob_storage_key IS NOT NULL ORDER BY blob_storage_key",
        )
        .map_err(StoreError::sqlite)?;
    let blobs = statement
        .query_map([], |row| {
            Ok(BackupBlob {
                storage_key: row.get(0)?,
                ciphertext_sha256: row.get(1)?,
                ciphertext_bytes: row.get(2)?,
            })
        })
        .map_err(StoreError::sqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(StoreError::sqlite)?;
    Ok(blobs)
}

fn record_backup_success(
    layout: &StorageLayout,
    report: &BackupReport,
    now_epoch_ms: i64,
) -> StoreResult<()> {
    let connection = open_live_database(layout)?;
    connection
        .execute(
            "UPDATE backup_state SET runs = runs + 1, last_completed_at_epoch_ms = ?1, last_elapsed_ms = ?2, peak_elapsed_ms = MAX(peak_elapsed_ms, ?2), last_sqlite_snapshot_ms = ?3, peak_sqlite_snapshot_ms = MAX(peak_sqlite_snapshot_ms, ?3), last_wal_growth_bytes = ?4, peak_wal_growth_bytes = MAX(peak_wal_growth_bytes, ?4), last_linked_blob_count = ?5, peak_linked_blob_count = MAX(peak_linked_blob_count, ?5), last_linked_blob_bytes = ?6, peak_linked_blob_bytes = MAX(peak_linked_blob_bytes, ?6), last_snapshot_name = ?7 WHERE singleton = 1",
            rusqlite::params![now_epoch_ms, report.elapsed_ms, report.sqlite_snapshot_ms, report.wal_growth_bytes, report.linked_blob_count, report.linked_blob_bytes, report.snapshot_name],
        )
        .map_err(StoreError::sqlite)?;
    Ok(())
}

fn record_backup_failure(layout: &StorageLayout, _now_epoch_ms: i64) -> StoreResult<()> {
    let connection = open_live_database(layout)?;
    connection
        .execute(
            "UPDATE backup_state SET runs = runs + 1, failures = failures + 1 WHERE singleton = 1",
            [],
        )
        .map_err(StoreError::sqlite)?;
    Ok(())
}

fn open_live_database(layout: &StorageLayout) -> StoreResult<Connection> {
    let connection = Connection::open(layout.database_path()).map_err(StoreError::sqlite)?;
    connection
        .busy_timeout(Duration::from_secs(30))
        .map_err(StoreError::sqlite)?;
    Ok(connection)
}

fn prune_snapshots(layout: &StorageLayout, retain: u64, current: &str) -> StoreResult<()> {
    let mut snapshots = fs::read_dir(layout.snapshots_root())
        .map_err(|error| StoreError::io("list cloud snapshots", error))?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            entry
                .file_name()
                .into_string()
                .ok()
                .map(|name| (name, entry.path()))
        })
        .filter(|(name, _)| name.starts_with("snapshot-") && name != current)
        .collect::<Vec<_>>();
    snapshots.sort_by(|left, right| right.0.cmp(&left.0));
    let keep_previous = retain.saturating_sub(1) as usize;
    for (_, path) in snapshots.into_iter().skip(keep_previous) {
        fs::remove_dir_all(path)
            .map_err(|error| StoreError::io("prune old cloud snapshot", error))?;
    }
    Ok(())
}

fn blob_path(root: &Path, storage_key: &str) -> PathBuf {
    root.join(&storage_key[..2])
        .join(format!("{storage_key}.blob"))
}

fn sha256_file(path: &Path) -> StoreResult<String> {
    let mut file =
        File::open(path).map_err(|error| StoreError::io("open file for hashing", error))?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| StoreError::io("hash cloud backup file", error))?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn write_json_sync(path: &Path, value: &impl Serialize) -> StoreResult<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| StoreError::internal(format!("encode cloud backup manifest: {error}")))?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| StoreError::io("create cloud backup manifest", error))?;
    file.write_all(&bytes)
        .map_err(|error| StoreError::io("write cloud backup manifest", error))?;
    file.write_all(b"\n")
        .map_err(|error| StoreError::io("finish cloud backup manifest", error))?;
    file.sync_all()
        .map_err(|error| StoreError::io("sync cloud backup manifest", error))
}

fn copy_file_sync(source: &Path, destination: &Path) -> StoreResult<()> {
    let mut source_file =
        File::open(source).map_err(|error| StoreError::io("open cloud restore source", error))?;
    let mut destination_file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .map_err(|error| StoreError::io("create cloud restore file", error))?;
    std::io::copy(&mut source_file, &mut destination_file)
        .map_err(|error| StoreError::io("copy cloud restore file", error))?;
    destination_file
        .sync_all()
        .map_err(|error| StoreError::io("sync cloud restore file", error))
}

fn sync_tree_files(root: &Path) -> StoreResult<()> {
    for entry in
        fs::read_dir(root).map_err(|error| StoreError::io("read cloud backup directory", error))?
    {
        let entry = entry.map_err(|error| StoreError::io("read cloud backup entry", error))?;
        let path = entry.path();
        if entry
            .file_type()
            .map_err(|error| StoreError::io("inspect cloud backup entry", error))?
            .is_dir()
        {
            sync_tree_files(&path)?;
            sync_directory(&path)?;
        } else {
            File::open(&path)
                .and_then(|file| file.sync_all())
                .map_err(|error| StoreError::io("sync cloud backup file", error))?;
        }
    }
    Ok(())
}

fn sync_directory(path: &Path) -> StoreResult<()> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| StoreError::io("sync cloud storage directory", error))
}

fn file_size(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    use product_contracts::{
        AcsCreateAccountRequest, AcsEncryptedValue, AcsErrorCode, ACS_CONTRACT_ID,
    };
    use tempfile::TempDir;

    use crate::CloudStore;

    fn store_with_blob(root: &TempDir) -> (StoreConfig, CloudStore) {
        let mut config = StoreConfig::for_test_data_root(root.path().to_path_buf());
        config.inline_threshold_bytes = 1;
        let store = CloudStore::open(config.clone()).unwrap();
        let challenge = store.issue_creation_challenge("network", 10).unwrap();
        store
            .create_account(
                &AcsCreateAccountRequest {
                    contract_id: ACS_CONTRACT_ID.to_string(),
                    account_locator: "account".to_string(),
                    signing_key_id: "key".to_string(),
                    signing_public_key_base64url: URL_SAFE_NO_PAD.encode([7_u8; 32]),
                    creation_challenge: challenge.challenge,
                },
                &[7_u8; 32],
                "network",
                11,
            )
            .unwrap();
        store
            .create_object(
                "account",
                "object",
                &AcsEncryptedValue::from_ciphertext(b"blob ciphertext", vec![]),
                20,
            )
            .unwrap();
        (config, store)
    }

    #[test]
    fn backup_survives_live_gc_and_restores_a_verified_store() {
        let root = TempDir::new().unwrap();
        let (config, store) = store_with_blob(&root);
        let report = create_backup(&config, 100).unwrap();
        let manifest = verify_backup(&report.snapshot_path).unwrap();
        assert_eq!(manifest.linked_blob_count, 1);
        assert_eq!(manifest.linked_blob_bytes, 15);
        let status = store.status(200).unwrap();
        for (id, warning, critical) in [
            (
                "backup_age_seconds",
                config.backup_age_seconds_warning,
                config.backup_age_seconds_critical,
            ),
            (
                "backup_elapsed_ms",
                config.backup_elapsed_ms_warning,
                config.backup_elapsed_ms_critical,
            ),
            (
                "backup_sqlite_snapshot_ms",
                config.backup_sqlite_snapshot_ms_warning,
                config.backup_sqlite_snapshot_ms_critical,
            ),
            (
                "backup_wal_growth_bytes",
                config.backup_wal_growth_bytes_warning,
                config.backup_wal_growth_bytes_critical,
            ),
            (
                "backup_linked_blob_count",
                config.backup_linked_blob_count_warning,
                config.backup_linked_blob_count_critical,
            ),
            (
                "backup_linked_blob_bytes",
                config.backup_linked_blob_bytes_warning,
                config.backup_linked_blob_bytes_critical,
            ),
        ] {
            let metric = status
                .metrics
                .iter()
                .find(|metric| metric.id == id)
                .unwrap();
            assert_eq!(metric.warning_at, Some(warning));
            assert_eq!(metric.critical_at, Some(critical));
        }

        store.delete_object("account", "object", 200).unwrap();
        store.run_gc(300, 0).unwrap();
        verify_backup(&report.snapshot_path).unwrap();
        drop(store);

        let restored = restore_backup(root.path(), &report.snapshot_path, 400).unwrap();
        assert_eq!(restored.restored_blob_count, 1);
        let store = CloudStore::open(config).unwrap();
        let object = store.read_object("account", "object").unwrap();
        assert_eq!(object.value.ciphertext().unwrap(), b"blob ciphertext");
    }

    #[test]
    fn verification_rejects_a_corrupted_blob() {
        let root = TempDir::new().unwrap();
        let (config, _store) = store_with_blob(&root);
        let report = create_backup(&config, 100).unwrap();
        let manifest = verify_backup(&report.snapshot_path).unwrap();
        let blob = blob_path(
            &report.snapshot_path.join("blobs"),
            &manifest.blobs[0].storage_key,
        );
        fs::write(blob, b"wrong").unwrap();
        let error = verify_backup(&report.snapshot_path).unwrap_err();
        assert_eq!(error.code, AcsErrorCode::Internal);
        assert!(error.message.contains("failed verification"));
    }

    #[test]
    fn scheduled_backup_uses_persisted_due_time_while_backup_now_bypasses_it() {
        let root = TempDir::new().unwrap();
        let (config, _store) = store_with_blob(&root);
        let first = create_backup_if_due(&config, 100).unwrap();
        assert!(matches!(first, BackupIfDueReport::Created { .. }));

        let next_due = 100 + config.backup_interval_seconds as i64 * 1_000;
        assert_eq!(
            create_backup_if_due(&config, 200).unwrap(),
            BackupIfDueReport::NotDue {
                last_completed_at_epoch_ms: 100,
                next_due_at_epoch_ms: next_due,
            }
        );
        let forced = create_backup(&config, 300).unwrap();
        assert_eq!(forced.snapshot_name, "snapshot-300");
        assert_eq!(
            create_backup_if_due(&config, 400).unwrap(),
            BackupIfDueReport::NotDue {
                last_completed_at_epoch_ms: 300,
                next_due_at_epoch_ms: 300 + config.backup_interval_seconds as i64 * 1_000,
            }
        );
    }

    #[test]
    fn pinned_wal_reader_does_not_block_writes_and_reclamation_lock_blocks_gc() {
        let root = TempDir::new().unwrap();
        let (config, store) = store_with_blob(&root);
        let source = Connection::open_with_flags(
            StorageLayout::new(root.path().to_path_buf()).database_path(),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        source.execute_batch("BEGIN DEFERRED").unwrap();
        source
            .query_row("SELECT schema_version FROM metadata", [], |row| {
                row.get::<_, u32>(0)
            })
            .unwrap();
        let (write_sender, write_receiver) = std::sync::mpsc::channel();
        let writer = store.clone();
        let write_thread = std::thread::spawn(move || {
            write_sender
                .send(writer.create_object(
                    "account",
                    "during-backup",
                    &AcsEncryptedValue::from_ciphertext(b"new value", vec![]),
                    30,
                ))
                .unwrap();
        });
        write_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("a pinned WAL reader must not stop ACS writes")
            .unwrap();
        source.execute_batch("ROLLBACK").unwrap();
        write_thread.join().unwrap();

        let layout = StorageLayout::new(config.storage_root.clone());
        let reclamation_lock = layout.acquire_reclamation_lock().unwrap();
        let (gc_sender, gc_receiver) = std::sync::mpsc::channel();
        let collector = store.clone();
        let gc_thread = std::thread::spawn(move || {
            gc_sender.send(collector.run_gc(100, 0)).unwrap();
        });
        assert!(gc_receiver.recv_timeout(Duration::from_millis(50)).is_err());
        drop(reclamation_lock);
        gc_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("GC should continue after backup releases reclamation")
            .unwrap();
        gc_thread.join().unwrap();
    }
}
