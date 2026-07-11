use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use chrono::{DateTime, Datelike, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{engine::sha256_hex, structured_notam_records, StructuredNotamRecord};

const NOTAM_STORE_SCHEMA_VERSION: u32 = 1;

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
pub struct NotamIngestedSegment {
    pub path: PathBuf,
    pub sha256: String,
    pub message_count: usize,
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
        fs::create_dir_all(self.active_root())
            .with_context(|| format!("failed to create {}", self.active_root().display()))?;
        fs::create_dir_all(self.segments_root())
            .with_context(|| format!("failed to create {}", self.segments_root().display()))?;
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

    pub fn prepare_active_run(
        &self,
        started_at_utc: DateTime<Utc>,
        run_counter: u64,
    ) -> anyhow::Result<PathBuf> {
        let run_dir = self.active_root().join(format!(
            "{}_{run_counter:06}.open",
            started_at_utc.format("%Y%m%dT%H%M%SZ")
        ));
        remove_path_if_exists(&run_dir)?;
        fs::create_dir_all(&run_dir)
            .with_context(|| format!("failed to create {}", run_dir.display()))?;
        sync_dir(&self.active_root())?;
        Ok(run_dir)
    }

    pub fn complete_collector_run(
        &self,
        run_dir: &Path,
        message_count_hint: usize,
        last_received_at_utc: Option<String>,
    ) -> anyhow::Result<Option<NotamIngestedSegment>> {
        let messages_path = run_dir.join("messages.jsonl");
        if !messages_path.is_file() {
            remove_path_if_exists(run_dir)?;
            return Ok(None);
        }
        let segment =
            self.promote_messages_file(&messages_path, message_count_hint, last_received_at_utc)?;
        remove_path_if_exists(run_dir)?;
        sync_dir(&self.active_root())?;
        Ok(segment)
    }

    pub fn recover_active_runs(&self) -> anyhow::Result<Vec<NotamIngestedSegment>> {
        let mut recovered = Vec::new();
        if !self.active_root().is_dir() {
            return Ok(recovered);
        }
        let mut active_runs = fs::read_dir(self.active_root())
            .with_context(|| format!("failed to read {}", self.active_root().display()))?
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| format!("failed to read {}", self.active_root().display()))?;
        active_runs.sort_by_key(|entry| entry.path());
        for entry in active_runs {
            if !entry
                .file_type()
                .with_context(|| format!("failed to stat {}", entry.path().display()))?
                .is_dir()
            {
                continue;
            }
            let run_dir = entry.path();
            let messages_path = run_dir.join("messages.jsonl");
            if messages_path.is_file() {
                if let Some(segment) = self.promote_messages_file(&messages_path, 0, None)? {
                    recovered.push(segment);
                }
            }
            remove_path_if_exists(&run_dir)?;
        }
        sync_dir(&self.active_root())?;
        Ok(recovered)
    }

    pub fn apply_segment(&self, segment: &NotamIngestedSegment) -> anyhow::Result<bool> {
        let mut connection = self.open_connection()?;
        let tx = connection
            .transaction()
            .context("failed to start NOTAM sqlite transaction")?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT sha256 FROM ingest_segments WHERE path = ?1",
                [segment.path.to_string_lossy().as_ref()],
                |row| row.get(0),
            )
            .optional()
            .context("failed to query NOTAM ingest segment")?;
        if let Some(existing_sha256) = existing {
            if existing_sha256 != segment.sha256 {
                bail!(
                    "NOTAM ingest segment {} was previously applied with a different content hash",
                    segment.path.display()
                );
            }
            return Ok(false);
        }

        let records = structured_notam_records(&segment.path)?;
        for record in records {
            let status = record.notam_status.as_deref().unwrap_or("");
            if status.eq_ignore_ascii_case("CANCELLED") || status.eq_ignore_ascii_case("CANCELED") {
                tx.execute("DELETE FROM current_notams WHERE id = ?1", [&record.id])
                    .with_context(|| format!("failed to delete NOTAM {}", record.id))?;
            } else {
                let record_json = serde_json::to_string(&record)
                    .with_context(|| format!("failed to encode NOTAM {}", record.id))?;
                tx.execute(
                    "INSERT INTO current_notams (
                        id, status, last_updated_utc, record_json, updated_at_utc
                    ) VALUES (?1, ?2, ?3, ?4, ?5)
                    ON CONFLICT(id) DO UPDATE SET
                        status = excluded.status,
                        last_updated_utc = excluded.last_updated_utc,
                        record_json = excluded.record_json,
                        updated_at_utc = excluded.updated_at_utc",
                    params![
                        record.id.as_str(),
                        record.notam_status.as_deref(),
                        record.last_updated_utc.as_deref(),
                        record_json,
                        Utc::now().to_rfc3339()
                    ],
                )
                .with_context(|| format!("failed to upsert NOTAM {}", record.id))?;
            }
        }
        tx.execute(
            "INSERT INTO ingest_segments (
                path, sha256, message_count, ingested_at_utc, last_received_at_utc
            ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                segment.path.to_string_lossy(),
                segment.sha256.as_str(),
                segment.message_count as i64,
                Utc::now().to_rfc3339(),
                segment.last_received_at_utc.as_deref()
            ],
        )
        .context("failed to record NOTAM ingest segment")?;
        tx.commit()
            .context("failed to commit NOTAM sqlite transaction")?;
        Ok(true)
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
            records.push(
                serde_json::from_str::<StructuredNotamRecord>(&record_json)
                    .context("failed to parse NOTAM record JSON")?,
            );
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
        let connection = Connection::open(self.sqlite_path())
            .with_context(|| format!("failed to open {}", self.sqlite_path().display()))?;
        connection
            .execute_batch(
                "
                PRAGMA journal_mode = WAL;
                CREATE TABLE IF NOT EXISTS metadata (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS ingest_segments (
                    path TEXT PRIMARY KEY,
                    sha256 TEXT NOT NULL,
                    message_count INTEGER NOT NULL,
                    ingested_at_utc TEXT NOT NULL,
                    last_received_at_utc TEXT
                );
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
        connection
            .execute(
                "INSERT INTO metadata(key, value) VALUES ('schema_version', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [NOTAM_STORE_SCHEMA_VERSION.to_string()],
            )
            .context("failed to record NOTAM sqlite schema version")?;
        Ok(connection)
    }

    fn promote_messages_file(
        &self,
        messages_path: &Path,
        message_count_hint: usize,
        last_received_at_utc: Option<String>,
    ) -> anyhow::Result<Option<NotamIngestedSegment>> {
        let bytes = complete_jsonl_bytes(messages_path)?;
        if bytes.is_empty() {
            return Ok(None);
        }
        let message_count = count_json_lines(&bytes)?;
        if message_count_hint != 0 && message_count != message_count_hint {
            bail!(
                "NOTAM collector summary reported {message_count_hint} messages, but {} contains {message_count} complete records",
                messages_path.display()
            );
        }
        let last_received_at_utc =
            last_received_at_utc.or_else(|| last_received_at_from_jsonl_bytes(&bytes));
        let timestamp = last_received_at_utc
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);
        let hash = Sha256::digest(&bytes);
        let sha256 = format!("{hash:x}");
        let segment_dir = self
            .segments_root()
            .join(format!("{:04}", timestamp.year()))
            .join(format!("{:02}", timestamp.month()))
            .join(format!("{:02}", timestamp.day()));
        fs::create_dir_all(&segment_dir)
            .with_context(|| format!("failed to create {}", segment_dir.display()))?;
        let segment_path = segment_dir.join(format!(
            "{}_{}_{}.jsonl",
            timestamp.format("%Y%m%dT%H%M%SZ"),
            message_count,
            &sha256[..16]
        ));
        if !segment_path.is_file() {
            let temp_path = segment_path.with_extension("jsonl.tmp");
            {
                let mut file = File::create(&temp_path)
                    .with_context(|| format!("failed to create {}", temp_path.display()))?;
                file.write_all(&bytes)
                    .with_context(|| format!("failed to write {}", temp_path.display()))?;
                file.sync_all()
                    .with_context(|| format!("failed to sync {}", temp_path.display()))?;
            }
            fs::rename(&temp_path, &segment_path).with_context(|| {
                format!(
                    "failed to promote {} to {}",
                    temp_path.display(),
                    segment_path.display()
                )
            })?;
            sync_dir(&segment_dir)?;
        }
        Ok(Some(NotamIngestedSegment {
            path: segment_path,
            sha256,
            message_count,
            last_received_at_utc,
        }))
    }

    fn active_root(&self) -> PathBuf {
        self.root.join("ingest").join("active")
    }

    fn segments_root(&self) -> PathBuf {
        self.root.join("ingest").join("segments")
    }

    fn state_root(&self) -> PathBuf {
        self.root.join("state")
    }
}

impl Drop for NotamStoreLock {
    fn drop(&mut self) {
        let _ = unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
        let _ = self.file.sync_all();
        let _ = &self.path;
    }
}

fn complete_jsonl_bytes(path: &Path) -> anyhow::Result<Vec<u8>> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    if bytes.is_empty() {
        return Ok(bytes);
    }
    let complete_len = if bytes.ends_with(b"\n") {
        bytes.len()
    } else {
        match bytes.iter().rposition(|byte| *byte == b'\n') {
            Some(index) => index + 1,
            None => 0,
        }
    };
    let bytes = bytes[..complete_len].to_vec();
    count_json_lines(&bytes)?;
    Ok(bytes)
}

fn count_json_lines(bytes: &[u8]) -> anyhow::Result<usize> {
    let mut count = 0;
    for (index, line) in bytes.split(|byte| *byte == b'\n').enumerate() {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        serde_json::from_slice::<serde_json::Value>(line)
            .with_context(|| format!("failed to parse NOTAM journal line {}", index + 1))?;
        count += 1;
    }
    Ok(count)
}

fn last_received_at_from_jsonl_bytes(bytes: &[u8]) -> Option<String> {
    bytes
        .split(|byte| *byte == b'\n')
        .rev()
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
        .find_map(|line| {
            serde_json::from_slice::<serde_json::Value>(line)
                .ok()
                .and_then(|value| {
                    value
                        .get("receivedAtUtc")
                        .and_then(|received| received.as_str())
                        .map(str::to_string)
                })
        })
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

fn remove_path_if_exists(path: &Path) -> anyhow::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.is_dir() {
                fs::remove_dir_all(path)
                    .with_context(|| format!("failed to remove {}", path.display()))?;
            } else {
                fs::remove_file(path)
                    .with_context(|| format!("failed to remove {}", path.display()))?;
            }
            Ok(())
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to stat {}", path.display())),
    }
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
    fn active_run_recovery_promotes_complete_lines_and_ignores_partial_tail() -> anyhow::Result<()>
    {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize(&identity("queue-a"))?;
        let run_dir = store.prepare_active_run(Utc::now(), 1)?;
        fs::write(
            run_dir.join("messages.jsonl"),
            format!("{}\n{{\"partial\"", captured_notam_line("PUBLISHED")),
        )?;

        let recovered = store.recover_active_runs()?;

        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].message_count, 1);
        assert!(!run_dir.exists());
        assert_eq!(count_json_lines(&fs::read(&recovered[0].path)?)?, 1);
        Ok(())
    }

    #[test]
    fn applying_cancelled_notam_deletes_current_record() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize(&identity("queue-a"))?;
        let segment_dir = temp.path().join("segments");
        fs::create_dir_all(&segment_dir)?;
        let published_path = segment_dir.join("published.jsonl");
        let cancelled_path = segment_dir.join("cancelled.jsonl");
        fs::write(
            &published_path,
            format!("{}\n", captured_notam_line("PUBLISHED")),
        )?;
        fs::write(
            &cancelled_path,
            format!("{}\n", captured_notam_line("CANCELLED")),
        )?;

        store.apply_segment(&segment(&published_path, 1)?)?;
        assert_eq!(store.current_records()?.len(), 1);

        store.apply_segment(&segment(&cancelled_path, 1)?)?;
        assert!(store.current_records()?.is_empty());
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

    fn segment(path: &Path, message_count: usize) -> anyhow::Result<NotamIngestedSegment> {
        let bytes = fs::read(path)?;
        Ok(NotamIngestedSegment {
            path: path.to_path_buf(),
            sha256: sha256_hex(&bytes),
            message_count,
            last_received_at_utc: Some("2026-07-11T02:03:00Z".to_string()),
        })
    }

    fn captured_notam_line(status: &str) -> String {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<AIXMBasicMessage xmlns:event="http://www.aixm.aero/schema/5.1/event">
  <hasMember>
    <event:Event>
      <event:timeSlice>
        <event:EventTimeSlice>
          <event:scenario>95</event:scenario>
          <event:textNOTAM>
            <event:NOTAM>
              <event:number>1</event:number>
              <event:year>2026</event:year>
              <event:type>N</event:type>
              <event:issued>2026-07-11T02:00:00.000Z</event:issued>
              <event:location>AAA</event:location>
              <event:effectiveStart>202607110200</event:effectiveStart>
              <event:effectiveEnd>202607111200</event:effectiveEnd>
              <event:text>RWY 01 CLSD.</event:text>
            </event:NOTAM>
          </event:textNOTAM>
        </event:EventTimeSlice>
      </event:timeSlice>
    </event:Event>
  </hasMember>
</AIXMBasicMessage>"#;
        serde_json::json!({
            "jmsMessageId": "ID:test",
            "receivedAtUtc": "2026-07-11T02:03:00Z",
            "properties": {
                "us_gov_dot_faa_aim_fns_nds_SourceType": "D",
                "us_gov_dot_faa_aim_fns_nds_ICAOId": "KAAA",
                "us_gov_dot_faa_aim_fns_nds_LocationDesignator": "AAA",
                "us_gov_dot_faa_aim_fns_nds_NOTAMStatus": status,
                "us_gov_dot_faa_aim_fns_nds_NOTAMFunction": "NOTAMN",
                "us_gov_dot_faa_aim_fns_nds_NOTAMKeyword": "RWY"
            },
            "bodyText": xml
        })
        .to_string()
    }
}
