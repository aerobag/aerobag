use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use crate::NotamProjectionAction;
use crate::{
    engine::sha256_hex, notam_projection_action, structured_notam_record_from_json,
    validate_canonical_structured_notam_record, StructuredNotamRecord,
};

const NOTAM_STORE_SCHEMA_VERSION: u32 = 3;
const RAW_INGEST_CURSOR_METADATA_KEY: &str = "raw_ingest_cursor";
const RAW_INGEST_RETENTION_DAYS: i64 = 7;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwimNotamSubscriptionIdentity {
    pub provider_url: String,
    pub queue: String,
    pub connection_factory: String,
    pub username: String,
    pub vpn: String,
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
    pub max_ingest_seq: i64,
    pub last_received_at_utc: Option<String>,
}

impl NotamPersistentStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn initialize(&self, identity: &SwimNotamSubscriptionIdentity) -> anyhow::Result<()> {
        fs::create_dir_all(self.state_root())
            .with_context(|| format!("failed to create {}", self.state_root().display()))?;
        self.check_or_write_identity(identity)?;
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
        let mut changed_count = 0_usize;
        let mut removed_count = 0_usize;
        let mut last_received_at_utc = None;
        let max_ingest_seq = rows
            .last()
            .map(|row| row.0)
            .context("raw NOTAM rows unexpectedly empty")?;
        let tx = connection
            .transaction()
            .context("failed to start NOTAM raw ingest apply transaction")?;
        for (ingest_seq, received_at_utc, message_json) in &rows {
            last_received_at_utc = Some(received_at_utc.clone());
            if let Some(record) = structured_notam_record_from_json(message_json)
                .with_context(|| format!("failed to normalize raw NOTAM ingest row {ingest_seq}"))?
            {
                let (changed, removed) = apply_record_to_projection(
                    &tx,
                    ProjectionTable::Current,
                    &record,
                    &applied_at_utc,
                )?;
                changed_count += changed;
                removed_count += removed;
            }
        }
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
            max_ingest_seq,
            last_received_at_utc,
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

    fn check_or_write_identity(
        &self,
        identity: &SwimNotamSubscriptionIdentity,
    ) -> anyhow::Result<()> {
        let path = self.root.join("subscription.json");
        if path.exists() {
            let existing = serde_json::from_slice::<SwimNotamSubscriptionIdentity>(
                &fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?,
            )
            .with_context(|| format!("failed to parse {}", path.display()))?;
            if existing != *identity {
                bail!(
                    "NOTAM store identity does not match credentials: state root {} belongs to a different subscription",
                    self.root.display()
                );
            }
            return Ok(());
        }
        write_json_pretty_file(&path, identity)?;
        sync_dir(&self.root)?;
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
            None => {
                connection
                    .execute(
                        "INSERT INTO metadata(key, value) VALUES ('schema_version', ?1)",
                        [NOTAM_STORE_SCHEMA_VERSION.to_string()],
                    )
                    .context("failed to initialize NOTAM sqlite schema version")?;
                Ok(())
            }
            Some("3") => Ok(()),
            Some("2") => self.reproject_schema_v2(connection),
            Some(version) => bail!(
                "unsupported NOTAM sqlite schema {version}; required {NOTAM_STORE_SCHEMA_VERSION}"
            ),
        }
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
            "DROP TABLE IF EXISTS current_notams_v3;
             CREATE TABLE current_notams_v3 (
                id TEXT PRIMARY KEY,
                status TEXT,
                last_updated_utc TEXT,
                record_json TEXT NOT NULL,
                updated_at_utc TEXT NOT NULL
             );",
        )
        .context("failed to create NOTAM schema 3 projection")?;
        for (ingest_seq, message_json) in &raw_rows {
            if let Some(record) =
                structured_notam_record_from_json(message_json).with_context(|| {
                    format!("failed to normalize raw NOTAM reprojection row {ingest_seq}")
                })?
            {
                apply_record_to_projection(
                    &tx,
                    ProjectionTable::SchemaV3,
                    &record,
                    &reprojected_at_utc,
                )?;
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
            [NOTAM_STORE_SCHEMA_VERSION.to_string()],
        )
        .context("failed to promote NOTAM sqlite schema version")?;
        tx.execute_batch(
            "ALTER TABLE current_notams RENAME TO current_notams_v2_retired;
             ALTER TABLE current_notams_v3 RENAME TO current_notams;
             DROP TABLE current_notams_v2_retired;
             CREATE INDEX current_notams_status_idx ON current_notams(status);
             CREATE INDEX current_notams_last_updated_idx
                ON current_notams(last_updated_utc);",
        )
        .context("failed to atomically promote NOTAM schema 3 projection")?;
        tx.commit()
            .context("failed to commit NOTAM schema reprojection")?;
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

#[derive(Clone, Copy)]
enum ProjectionTable {
    Current,
    SchemaV3,
}

impl ProjectionTable {
    fn name(self) -> &'static str {
        match self {
            Self::Current => "current_notams",
            Self::SchemaV3 => "current_notams_v3",
        }
    }
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
    Ok((
        (existing.as_deref() != Some(record_json.as_str())) as usize,
        0,
    ))
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

fn write_json_pretty_file<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let temp = path.with_extension("tmp");
    {
        let mut file =
            File::create(&temp).with_context(|| format!("failed to create {}", temp.display()))?;
        serde_json::to_writer_pretty(&mut file, value)
            .with_context(|| format!("failed to write {}", temp.display()))?;
        file.write_all(b"\n")
            .with_context(|| format!("failed to write {}", temp.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync {}", temp.display()))?;
    }
    fs::rename(&temp, path)
        .with_context(|| format!("failed to promote {} to {}", temp.display(), path.display()))?;
    if let Some(parent) = path.parent() {
        sync_dir(parent)?;
    }
    Ok(())
}

fn sync_dir(path: &Path) -> anyhow::Result<()> {
    let dir = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    dir.sync_all()
        .with_context(|| format!("failed to sync {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn lock_prevents_second_notam_consumer_in_same_store() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize(&identity("queue-a"))?;

        let _lock = store.acquire_lock()?;
        let error = store.acquire_lock().expect_err("second lock must fail");

        assert!(
            format!("{error:#}").contains("already locked"),
            "unexpected error: {error:#}"
        );
        Ok(())
    }

    #[test]
    fn identity_mismatch_fails_loudly() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize(&identity("queue-a"))?;

        let error = store
            .initialize(&identity("queue-b"))
            .expect_err("identity mismatch must fail");

        assert!(
            format!("{error:#}").contains("does not match credentials"),
            "unexpected error: {error:#}"
        );
        Ok(())
    }

    #[test]
    fn committed_raw_messages_advance_projection_cursor() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize(&identity("queue-a"))?;
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
    fn committed_cancelled_notam_deletes_current_record() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize(&identity("queue-a"))?;
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
    fn cancellation_uses_same_identity_across_swim_dialects() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize(&identity("queue-a"))?;
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
    fn schema_v2_reprojection_replays_canonical_cancellation() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize(&identity("queue-a"))?;
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
            "3"
        );
        assert_eq!(raw_ingest_cursor(&connection)?, 2);
        Ok(())
    }

    fn identity(queue: &str) -> SwimNotamSubscriptionIdentity {
        SwimNotamSubscriptionIdentity {
            provider_url: "smfs://example.test:55443".to_string(),
            queue: queue.to_string(),
            connection_factory: "example.CF".to_string(),
            username: "example.user".to_string(),
            vpn: "example-vpn".to_string(),
        }
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
