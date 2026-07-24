use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use preprocessor_live_feeds::nms_initial_load::{
    parse_nms_api_update, NmsApiUpdate, NmsApiUpdateAction, NmsNotamClassification,
};
use preprocessor_live_feeds::StructuredNotamRecord;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{capture_initial_load, load_initial_load_capture, NmsApiSource};

const NMS_API_STORE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct CollectorOptions {
    pub poll_interval: Duration,
    pub overlap: Duration,
    pub run_duration: Option<Duration>,
    pub max_polls: Option<usize>,
}

impl Default for CollectorOptions {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(180),
            overlap: Duration::from_secs(600),
            run_duration: None,
            max_polls: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NmsApiPollSummary {
    pub started_at_utc: String,
    pub query_since_utc: String,
    pub domestic_received: usize,
    pub fdc_received: usize,
    pub new_payloads: usize,
    pub duplicate_payloads: usize,
    pub upserted: usize,
    pub removed: usize,
    pub expired: usize,
    pub current_records: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum NmsCollectorEvent {
    StateReady {
        installed_initial_load: bool,
        current_records: usize,
        cursor_utc: String,
    },
    PollApplied {
        summary: NmsApiPollSummary,
    },
    PollFailed {
        failed_at_utc: String,
        attempt: usize,
        error: String,
        cursor_utc: String,
    },
}

#[derive(Clone)]
pub struct NmsApiCollectorStore {
    root: PathBuf,
}

struct NmsApiCollectorLock {
    _file: File,
}

impl NmsApiCollectorStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn initialize(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.root)
            .with_context(|| format!("failed to create {}", self.root.display()))?;
        fs::create_dir_all(self.capture_root())
            .with_context(|| format!("failed to create {}", self.capture_root().display()))?;
        let connection = self.open_connection()?;
        let schema_version = metadata(&connection, "schema_version")?;
        match schema_version.as_deref() {
            None => set_metadata(
                &connection,
                "schema_version",
                &NMS_API_STORE_SCHEMA_VERSION.to_string(),
            ),
            Some("1") => Ok(()),
            Some(version) => bail!(
                "unsupported NMS API collector schema {version}; required {NMS_API_STORE_SCHEMA_VERSION}"
            ),
        }
    }

    pub fn is_baseline_installed(&self) -> anyhow::Result<bool> {
        let connection = self.open_connection()?;
        Ok(metadata(&connection, "baseline_installed_at_utc")?.is_some())
    }

    fn acquire_lock(&self) -> anyhow::Result<NmsApiCollectorLock> {
        let path = self.root.join("lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .with_context(|| format!("failed to open NMS API collector lock {}", path.display()))?;
        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == ErrorKind::WouldBlock {
                bail!(
                    "NMS API collector state is already locked: {}",
                    self.root.display()
                );
            }
            return Err(error)
                .with_context(|| format!("failed to lock NMS API state {}", self.root.display()));
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
        Ok(NmsApiCollectorLock { _file: file })
    }

    pub fn capture_root(&self) -> PathBuf {
        self.root.join("captures")
    }

    pub fn install_baseline(
        &self,
        environment: &str,
        api_base_url: Option<&str>,
        capture_started_at: DateTime<Utc>,
        capture_path: &Path,
        records: &[StructuredNotamRecord],
    ) -> anyhow::Result<()> {
        if records.is_empty() {
            bail!("NMS API Initial Load baseline is empty");
        }
        let installed_at = Utc::now();
        let mut connection = self.open_connection()?;
        let tx = connection
            .transaction()
            .context("failed to start NMS API baseline transaction")?;
        tx.execute("DELETE FROM current_notams", [])
            .context("failed to clear NMS API current state")?;
        for record in records {
            if record.notam_status.as_deref() != Some("ACTIVE") {
                bail!("Initial Load record {} is not active", record.id);
            }
            upsert_current(&tx, record, installed_at)?;
        }
        for (key, value) in [
            ("source_environment", environment.to_string()),
            ("api_base_url", api_base_url.unwrap_or_default().to_string()),
            (
                "baseline_capture_path",
                capture_path.to_string_lossy().into_owned(),
            ),
            ("baseline_installed_at_utc", installed_at.to_rfc3339()),
            ("poll_cursor_utc", capture_started_at.to_rfc3339()),
        ] {
            set_metadata(&tx, key, &value)?;
        }
        tx.commit()
            .context("failed to commit NMS API Initial Load baseline")
    }

    pub fn poll_cursor(&self) -> anyhow::Result<DateTime<Utc>> {
        let connection = self.open_connection()?;
        let value = metadata(&connection, "poll_cursor_utc")?
            .context("NMS API collector has no poll cursor")?;
        parse_timestamp(&value, "NMS API poll cursor")
    }

    pub fn apply_poll(
        &self,
        started_at: DateTime<Utc>,
        query_since: DateTime<Utc>,
        domestic_xml: Vec<String>,
        fdc_xml: Vec<String>,
    ) -> anyhow::Result<NmsApiPollSummary> {
        let domestic_received = domestic_xml.len();
        let fdc_received = fdc_xml.len();
        let mut parsed = Vec::with_capacity(domestic_received + fdc_received);
        for (classification, messages) in [
            (NmsNotamClassification::Domestic, domestic_xml),
            (NmsNotamClassification::Fdc, fdc_xml),
        ] {
            for xml in messages {
                let update = parse_nms_api_update(&xml, classification).with_context(|| {
                    format!(
                        "failed to parse {} lastUpdatedDate record",
                        classification.api_name()
                    )
                })?;
                let last_updated = update
                    .record
                    .last_updated_utc
                    .as_deref()
                    .context("NMS API update has no lastUpdated time")
                    .and_then(|value| parse_timestamp(value, "NMS API update"))?;
                parsed.push(ParsedRawUpdate {
                    classification,
                    payload_sha256: sha256_hex(xml.as_bytes()),
                    raw_aixm: xml,
                    last_updated,
                    update,
                });
            }
        }
        parsed.sort_by(|left, right| {
            left.last_updated
                .cmp(&right.last_updated)
                .then_with(|| left.update.record.id.cmp(&right.update.record.id))
                .then_with(|| left.payload_sha256.cmp(&right.payload_sha256))
        });

        let completed_at = Utc::now();
        let mut connection = self.open_connection()?;
        let tx = connection
            .transaction()
            .context("failed to start NMS API poll transaction")?;
        tx.execute(
            "INSERT INTO poll_runs (
                started_at_utc, query_since_utc, completed_at_utc,
                domestic_received, fdc_received
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                started_at.to_rfc3339(),
                query_since.to_rfc3339(),
                completed_at.to_rfc3339(),
                domestic_received as i64,
                fdc_received as i64,
            ],
        )
        .context("failed to record NMS API poll")?;
        let poll_id = tx.last_insert_rowid();
        let mut new_payloads = 0usize;
        let mut duplicate_payloads = 0usize;
        let mut upserted = 0usize;
        let mut removed = 0usize;
        for parsed_update in parsed {
            let inserted = tx
                .execute(
                    "INSERT OR IGNORE INTO raw_updates (
                        payload_sha256, poll_id, classification, nms_id,
                        last_updated_utc, action, referenced_human_identity,
                        raw_aixm
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        parsed_update.payload_sha256,
                        poll_id,
                        parsed_update.classification.api_name(),
                        parsed_update.update.record.nms_id,
                        parsed_update.last_updated.to_rfc3339(),
                        action_name(parsed_update.update.action),
                        parsed_update
                            .update
                            .referenced_notam
                            .as_ref()
                            .map(|reference| reference.human_identity()),
                        parsed_update.raw_aixm,
                    ],
                )
                .context("failed to retain raw NMS API update")?;
            if inserted == 0 {
                duplicate_payloads += 1;
                continue;
            }
            new_payloads += 1;
            let (changed, deleted) =
                apply_update(&tx, &parsed_update.update, parsed_update.last_updated)?;
            upserted += changed;
            removed += deleted;
        }
        let expired = prune_expired(&tx, completed_at)?;
        set_metadata(&tx, "poll_cursor_utc", &started_at.to_rfc3339())?;
        let current_records =
            tx.query_row("SELECT COUNT(*) FROM current_notams", [], |row| {
                row.get::<_, i64>(0)
            })
            .context("failed to count NMS API current records")? as usize;
        tx.commit().context("failed to commit NMS API poll")?;
        Ok(NmsApiPollSummary {
            started_at_utc: started_at.to_rfc3339(),
            query_since_utc: query_since.to_rfc3339(),
            domestic_received,
            fdc_received,
            new_payloads,
            duplicate_payloads,
            upserted,
            removed,
            expired,
            current_records,
        })
    }

    pub fn current_records(&self) -> anyhow::Result<Vec<StructuredNotamRecord>> {
        let connection = self.open_connection()?;
        let mut statement = connection
            .prepare("SELECT record_json FROM current_notams ORDER BY id")
            .context("failed to prepare NMS API current-state query")?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .context("failed to query NMS API current state")?;
        rows.map(|row| {
            let json = row.context("failed to read NMS API current record")?;
            serde_json::from_str(&json).context("failed to decode NMS API current record")
        })
        .collect()
    }

    pub fn current_fingerprint(&self) -> anyhow::Result<String> {
        let records = self
            .current_records()?
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect::<std::collections::BTreeMap<_, _>>();
        Ok(sha256_hex(&serde_json::to_vec(&records)?))
    }

    fn open_connection(&self) -> anyhow::Result<Connection> {
        let connection = Connection::open(self.root.join("state.sqlite"))
            .with_context(|| format!("failed to open NMS API state {}", self.root.display()))?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .context("failed to set NMS API sqlite busy timeout")?;
        connection
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = FULL;
                 CREATE TABLE IF NOT EXISTS metadata (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS current_notams (
                    id TEXT PRIMARY KEY,
                    human_identity TEXT NOT NULL,
                    last_updated_utc TEXT NOT NULL,
                    effective_end_utc TEXT,
                    record_json TEXT NOT NULL,
                    updated_at_utc TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS current_notams_human_identity_idx
                    ON current_notams(human_identity);
                 CREATE INDEX IF NOT EXISTS current_notams_effective_end_idx
                    ON current_notams(effective_end_utc);
                 CREATE TABLE IF NOT EXISTS poll_runs (
                    poll_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    started_at_utc TEXT NOT NULL,
                    query_since_utc TEXT NOT NULL,
                    completed_at_utc TEXT NOT NULL,
                    domestic_received INTEGER NOT NULL,
                    fdc_received INTEGER NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS raw_updates (
                    payload_sha256 TEXT PRIMARY KEY,
                    poll_id INTEGER NOT NULL REFERENCES poll_runs(poll_id),
                    classification TEXT NOT NULL,
                    nms_id TEXT NOT NULL,
                    last_updated_utc TEXT NOT NULL,
                    action TEXT NOT NULL,
                    referenced_human_identity TEXT,
                    raw_aixm TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS raw_updates_poll_idx
                    ON raw_updates(poll_id);
                 CREATE INDEX IF NOT EXISTS raw_updates_nms_id_idx
                    ON raw_updates(nms_id, last_updated_utc);",
            )
            .context("failed to initialize NMS API sqlite schema")?;
        Ok(connection)
    }
}

pub fn run_collector(
    store: &NmsApiCollectorStore,
    source: &mut impl NmsApiSource,
    options: &CollectorOptions,
) -> anyhow::Result<()> {
    run_collector_with_observer(store, source, options, |_| Ok(()))
}

pub fn run_collector_with_observer(
    store: &NmsApiCollectorStore,
    source: &mut impl NmsApiSource,
    options: &CollectorOptions,
    mut observer: impl FnMut(&NmsCollectorEvent) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    if options.poll_interval.is_zero() {
        bail!("NMS API poll interval must be greater than zero");
    }
    if options.overlap >= Duration::from_secs(24 * 60 * 60) {
        bail!("NMS API overlap must be less than 24 hours");
    }
    store.initialize()?;
    let _lock = store.acquire_lock()?;
    let mut installed_initial_load = false;
    if !store.is_baseline_installed()? {
        let capture_started_at = Utc::now();
        let capture_path = store.capture_root().join(format!(
            "initial-load-{}",
            capture_started_at.format("%Y%m%dT%H%M%SZ")
        ));
        capture_initial_load(
            &capture_path,
            &[
                NmsNotamClassification::Domestic,
                NmsNotamClassification::Fdc,
            ],
            source,
        )?;
        let baseline = load_initial_load_capture(&capture_path)?;
        store.install_baseline(
            &baseline.source.environment,
            baseline.source.api_base_url.as_deref(),
            baseline.captured_at_utc,
            &capture_path,
            &baseline.records,
        )?;
        installed_initial_load = true;
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "event": "nms_api_baseline_installed",
                "capture": capture_path,
                "records": baseline.records.len(),
                "poll_cursor_utc": baseline.captured_at_utc,
            }))?
        );
    }
    observer(&NmsCollectorEvent::StateReady {
        installed_initial_load,
        current_records: store.current_records()?.len(),
        cursor_utc: store.poll_cursor()?.to_rfc3339(),
    })?;

    let run_started = Instant::now();
    let mut attempted_polls = 0usize;
    loop {
        if options
            .max_polls
            .is_some_and(|maximum| attempted_polls >= maximum)
        {
            break;
        }
        if options
            .run_duration
            .is_some_and(|duration| run_started.elapsed() >= duration)
        {
            break;
        }
        let poll_started = Instant::now();
        attempted_polls += 1;
        match poll_once(store, source, options.overlap) {
            Ok(summary) => {
                println!("{}", serde_json::to_string(&summary)?);
                observer(&NmsCollectorEvent::PollApplied { summary })?;
            }
            Err(error) => {
                let event = NmsCollectorEvent::PollFailed {
                    failed_at_utc: Utc::now().to_rfc3339(),
                    attempt: attempted_polls,
                    error: format!("{error:#}"),
                    cursor_utc: store.poll_cursor()?.to_rfc3339(),
                };
                eprintln!("{}", serde_json::to_string(&event)?);
                observer(&event)?;
            }
        }

        if options
            .max_polls
            .is_some_and(|maximum| attempted_polls >= maximum)
        {
            break;
        }
        if options
            .run_duration
            .is_some_and(|duration| run_started.elapsed() >= duration)
        {
            break;
        }
        if let Some(remaining) = options.poll_interval.checked_sub(poll_started.elapsed()) {
            std::thread::sleep(remaining);
        }
    }
    Ok(())
}

fn poll_once(
    store: &NmsApiCollectorStore,
    source: &mut impl NmsApiSource,
    overlap: Duration,
) -> anyhow::Result<NmsApiPollSummary> {
    let started_at = Utc::now();
    let cursor = store.poll_cursor()?;
    let overlap =
        chrono::Duration::from_std(overlap).context("NMS API overlap exceeds chrono range")?;
    let query_since = cursor - overlap;
    let domestic = source
        .fetch_updates(NmsNotamClassification::Domestic, query_since)
        .context("failed to fetch DOMESTIC NMS API updates")?;
    let fdc = source
        .fetch_updates(NmsNotamClassification::Fdc, query_since)
        .context("failed to fetch FDC NMS API updates")?;
    store.apply_poll(started_at, query_since, domestic, fdc)
}

struct ParsedRawUpdate {
    classification: NmsNotamClassification,
    payload_sha256: String,
    raw_aixm: String,
    last_updated: DateTime<Utc>,
    update: NmsApiUpdate,
}

fn apply_update(
    tx: &Transaction<'_>,
    update: &NmsApiUpdate,
    source_updated_at: DateTime<Utc>,
) -> anyhow::Result<(usize, usize)> {
    match update.action {
        NmsApiUpdateAction::Upsert => {
            let mut removed = 0;
            if let Some(reference) = &update.referenced_notam {
                removed += remove_referenced(tx, &reference.human_identity(), source_updated_at)?;
            }
            let changed = upsert_current(tx, &update.record, Utc::now())?;
            Ok((changed, removed))
        }
        NmsApiUpdateAction::RemoveSelf => {
            let removed = delete_if_not_newer(tx, &update.record.id, source_updated_at)?;
            Ok((0, removed))
        }
        NmsApiUpdateAction::RemoveReferenced => {
            let reference = update
                .referenced_notam
                .as_ref()
                .context("NMS API cancellation has no referenced NOTAM")?;
            let removed = remove_referenced(tx, &reference.human_identity(), source_updated_at)?;
            Ok((0, removed))
        }
    }
}

fn upsert_current(
    tx: &Transaction<'_>,
    record: &StructuredNotamRecord,
    updated_at: DateTime<Utc>,
) -> anyhow::Result<usize> {
    let last_updated = record
        .last_updated_utc
        .as_deref()
        .context("NMS API record has no lastUpdated time")
        .and_then(|value| parse_timestamp(value, "NMS API record"))?;
    let existing = tx
        .query_row(
            "SELECT last_updated_utc, record_json FROM current_notams WHERE id = ?1",
            [&record.id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .with_context(|| format!("failed to query NMS API record {}", record.id))?;
    if let Some((existing_updated, existing_json)) = &existing {
        let existing_updated = parse_timestamp(existing_updated, "stored NMS API record")?;
        if existing_updated > last_updated {
            return Ok(0);
        }
        let incoming_json = serde_json::to_string(record)?;
        if existing_updated == last_updated && existing_json == &incoming_json {
            return Ok(0);
        }
    }
    let record_json = serde_json::to_string(record)
        .with_context(|| format!("failed to encode NMS API record {}", record.id))?;
    tx.execute(
        "INSERT INTO current_notams (
            id, human_identity, last_updated_utc, effective_end_utc,
            record_json, updated_at_utc
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET
            human_identity = excluded.human_identity,
            last_updated_utc = excluded.last_updated_utc,
            effective_end_utc = excluded.effective_end_utc,
            record_json = excluded.record_json,
            updated_at_utc = excluded.updated_at_utc",
        params![
            record.id,
            human_identity(record)?,
            last_updated.to_rfc3339(),
            record.effective_end_utc,
            record_json,
            updated_at.to_rfc3339(),
        ],
    )
    .with_context(|| format!("failed to upsert NMS API record {}", record.id))?;
    Ok(1)
}

fn delete_if_not_newer(
    tx: &Transaction<'_>,
    id: &str,
    cancellation_time: DateTime<Utc>,
) -> anyhow::Result<usize> {
    let existing = tx
        .query_row(
            "SELECT last_updated_utc FROM current_notams WHERE id = ?1",
            [id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .with_context(|| format!("failed to query canceled NMS API record {id}"))?;
    let Some(existing) = existing else {
        return Ok(0);
    };
    if parse_timestamp(&existing, "stored NMS API record")? > cancellation_time {
        return Ok(0);
    }
    tx.execute("DELETE FROM current_notams WHERE id = ?1", [id])
        .with_context(|| format!("failed to remove canceled NMS API record {id}"))
}

fn remove_referenced(
    tx: &Transaction<'_>,
    human_identity: &str,
    cancellation_time: DateTime<Utc>,
) -> anyhow::Result<usize> {
    let matches = {
        let mut statement = tx
            .prepare(
                "SELECT id, last_updated_utc
                 FROM current_notams
                 WHERE human_identity = ?1
                 ORDER BY id",
            )
            .context("failed to prepare referenced NOTAM query")?;
        let matches = statement
            .query_map([human_identity], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        matches
    };
    if matches.len() > 1 {
        bail!(
            "NMS API cancellation target {human_identity} matches {} active records",
            matches.len()
        );
    }
    let Some((id, existing_updated)) = matches.into_iter().next() else {
        return Ok(0);
    };
    if parse_timestamp(&existing_updated, "stored NMS API record")? > cancellation_time {
        return Ok(0);
    }
    tx.execute("DELETE FROM current_notams WHERE id = ?1", [&id])
        .with_context(|| format!("failed to remove referenced NMS API record {id}"))
}

fn prune_expired(tx: &Transaction<'_>, now: DateTime<Utc>) -> anyhow::Result<usize> {
    let expired_ids = {
        let mut statement = tx
            .prepare(
                "SELECT id, effective_end_utc
                 FROM current_notams
                 WHERE effective_end_utc IS NOT NULL",
            )
            .context("failed to prepare NMS API expiry query")?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut expired = Vec::new();
        for (id, end) in rows {
            if parse_timestamp(&end, "NMS API effective end")? <= now {
                expired.push(id);
            }
        }
        expired
    };
    for id in &expired_ids {
        tx.execute("DELETE FROM current_notams WHERE id = ?1", [id])?;
    }
    Ok(expired_ids.len())
}

fn human_identity(record: &StructuredNotamRecord) -> anyhow::Result<String> {
    Ok(format!(
        "{}:{}:{}:{}:{}",
        record
            .source_type
            .as_deref()
            .context("missing source type")?,
        record.location.as_deref().context("missing location")?,
        record.notam_year.as_deref().context("missing NOTAM year")?,
        record.notam_type.as_deref().context("missing NOTAM type")?,
        record
            .notam_number
            .as_deref()
            .context("missing NOTAM number")?,
    ))
}

fn action_name(action: NmsApiUpdateAction) -> &'static str {
    match action {
        NmsApiUpdateAction::Upsert => "upsert",
        NmsApiUpdateAction::RemoveSelf => "remove_self",
        NmsApiUpdateAction::RemoveReferenced => "remove_referenced",
    }
}

fn metadata(connection: &Connection, key: &str) -> anyhow::Result<Option<String>> {
    connection
        .query_row("SELECT value FROM metadata WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .optional()
        .with_context(|| format!("failed to read NMS API metadata {key}"))
}

fn set_metadata(connection: &Connection, key: &str, value: &str) -> anyhow::Result<()> {
    connection
        .execute(
            "INSERT INTO metadata(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .with_context(|| format!("failed to write NMS API metadata {key}"))?;
    Ok(())
}

fn parse_timestamp(value: &str, label: &str) -> anyhow::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("{label} has invalid timestamp {value}"))
        .map(|value| value.with_timezone(&Utc))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use anyhow::bail;
    use tempfile::tempdir;

    use super::*;
    use crate::{InitialLoadCaptureSource, InitialLoadSource};

    #[test]
    fn collector_state_allows_only_one_process_lock() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NmsApiCollectorStore::new(temp.path());
        store.initialize()?;
        let first = store.acquire_lock()?;
        let error = store
            .acquire_lock()
            .err()
            .context("second lock succeeded")?;
        assert!(format!("{error:#}").contains("already locked"));
        drop(first);
        store.acquire_lock()?;
        Ok(())
    }

    #[test]
    fn overlap_is_idempotent_and_referenced_cancellation_survives_restart() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NmsApiCollectorStore::new(temp.path());
        store.initialize()?;
        let baseline_xml = update_xml(
            "1000000000000001",
            "FDC",
            "MHK",
            "7893",
            Some("N"),
            "!FDC 6/7893 MHK TEST",
            "TEST V1",
            "2026-07-23T14:00:00Z",
            None,
            "209907241405",
        );
        let baseline = parse_nms_api_update(&baseline_xml, NmsNotamClassification::Fdc)?;
        store.install_baseline(
            "fixture",
            None,
            timestamp("2026-07-23T14:00:00Z"),
            Path::new("/fixture/initial-load"),
            &[baseline.record],
        )?;

        let updated_xml = update_xml(
            "1000000000000001",
            "FDC",
            "MHK",
            "7893",
            Some("N"),
            "!FDC 6/7893 MHK TEST",
            "TEST V2",
            "2026-07-23T14:03:00Z",
            None,
            "209907241405",
        );
        let summary = store.apply_poll(
            timestamp("2026-07-23T14:04:00Z"),
            timestamp("2026-07-23T13:50:00Z"),
            Vec::new(),
            vec![updated_xml.clone(), updated_xml.clone()],
        )?;
        assert_eq!(summary.new_payloads, 1);
        assert_eq!(summary.duplicate_payloads, 1);
        assert_eq!(summary.upserted, 1);
        assert_eq!(store.current_records()?[0].text.as_deref(), Some("TEST V2"));

        let restarted = NmsApiCollectorStore::new(temp.path());
        assert_eq!(restarted.poll_cursor()?, timestamp("2026-07-23T14:04:00Z"));
        let summary = restarted.apply_poll(
            timestamp("2026-07-23T14:07:00Z"),
            timestamp("2026-07-23T13:54:00Z"),
            Vec::new(),
            vec![updated_xml],
        )?;
        assert_eq!(summary.new_payloads, 0);
        assert_eq!(summary.duplicate_payloads, 1);
        assert_eq!(summary.upserted, 0);

        let cancellation = update_xml(
            "1000000000000099",
            "FDC",
            "FDC",
            "7893",
            Some("C"),
            "!FDC 6/7893 FDC CANCEL 6/7893 MHK",
            "FDC 6/7893 NOTAMC 6/7893 A) MHK",
            "2026-07-23T14:08:00Z",
            None,
            "202607261408",
        );
        let summary = restarted.apply_poll(
            timestamp("2026-07-23T14:10:00Z"),
            timestamp("2026-07-23T13:57:00Z"),
            Vec::new(),
            vec![cancellation],
        )?;
        assert_eq!(summary.removed, 1);
        assert!(restarted.current_records()?.is_empty());
        Ok(())
    }

    #[test]
    fn same_id_cancellation_and_expiry_remove_records() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NmsApiCollectorStore::new(temp.path());
        store.initialize()?;
        let active_xml = update_xml(
            "1000000000000002",
            "DOM",
            "OME",
            "103",
            Some("N"),
            "!OME 07/103 OME TWY TEST",
            "TWY TEST",
            "2026-07-23T14:00:00Z",
            None,
            "209907241405",
        );
        let active = parse_nms_api_update(&active_xml, NmsNotamClassification::Domestic)?;
        store.install_baseline(
            "fixture",
            None,
            timestamp("2026-07-23T14:00:00Z"),
            Path::new("/fixture/initial-load"),
            &[active.record],
        )?;
        let cancellation = update_xml(
            "1000000000000002",
            "DOM",
            "OME",
            "103",
            None,
            "!OME 07/103 OME TWY TEST",
            "TWY TEST",
            "2026-07-23T14:16:00Z",
            Some("2026-07-23T14:16:00Z"),
            "209907241405",
        );
        let summary = store.apply_poll(
            timestamp("2026-07-23T14:17:00Z"),
            timestamp("2026-07-23T14:00:00Z"),
            vec![cancellation],
            Vec::new(),
        )?;
        assert_eq!(summary.removed, 1);

        let expired = update_xml(
            "1000000000000003",
            "DOM",
            "OME",
            "104",
            Some("N"),
            "!OME 07/104 OME TWY TEST",
            "TWY TEST",
            "2026-07-23T14:18:00Z",
            None,
            "202607231418",
        );
        let summary = store.apply_poll(
            timestamp("2026-07-23T14:19:00Z"),
            timestamp("2026-07-23T14:07:00Z"),
            vec![expired],
            Vec::new(),
        )?;
        assert_eq!(summary.upserted, 1);
        assert_eq!(summary.expired, 1);
        assert!(store.current_records()?.is_empty());
        Ok(())
    }

    #[test]
    fn transient_poll_failure_keeps_cursor_and_later_attempt_recovers() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let store = NmsApiCollectorStore::new(temp.path());
        store.initialize()?;
        let baseline_xml = update_xml(
            "1000000000000004",
            "DOM",
            "OME",
            "105",
            Some("N"),
            "!OME 07/105 OME TWY TEST",
            "TEST V1",
            "2026-07-23T14:00:00Z",
            None,
            "209907241405",
        );
        let baseline = parse_nms_api_update(&baseline_xml, NmsNotamClassification::Domestic)?;
        let initial_cursor = timestamp("2026-07-23T14:00:00Z");
        store.install_baseline(
            "fixture",
            None,
            initial_cursor,
            Path::new("/fixture/initial-load"),
            &[baseline.record],
        )?;

        let updated_xml = update_xml(
            "1000000000000004",
            "DOM",
            "OME",
            "105",
            Some("N"),
            "!OME 07/105 OME TWY TEST",
            "TEST V2",
            "2026-07-23T14:03:00Z",
            None,
            "209907241405",
        );
        let mut source = FixtureSource {
            domestic_results: VecDeque::from([
                Err("temporary DOMESTIC failure".to_string()),
                Ok(vec![updated_xml]),
            ]),
        };
        let mut events = Vec::new();
        run_collector_with_observer(
            &store,
            &mut source,
            &CollectorOptions {
                poll_interval: Duration::from_millis(1),
                overlap: Duration::from_secs(600),
                run_duration: None,
                max_polls: Some(2),
            },
            |event| {
                events.push(event.clone());
                Ok(())
            },
        )?;

        assert!(matches!(
            events.first(),
            Some(NmsCollectorEvent::StateReady {
                installed_initial_load: false,
                current_records: 1,
                ..
            })
        ));
        assert!(matches!(
            events.get(1),
            Some(NmsCollectorEvent::PollFailed { attempt: 1, .. })
        ));
        assert!(matches!(
            events.get(2),
            Some(NmsCollectorEvent::PollApplied { summary })
                if summary.upserted == 1
        ));
        assert!(store.poll_cursor()? > initial_cursor);
        assert_eq!(store.current_records()?[0].text.as_deref(), Some("TEST V2"));
        let connection = store.open_connection()?;
        assert_eq!(
            connection.query_row("SELECT COUNT(*) FROM poll_runs", [], |row| {
                row.get::<_, i64>(0)
            })?,
            1
        );
        Ok(())
    }

    struct FixtureSource {
        domestic_results: VecDeque<Result<Vec<String>, String>>,
    }

    impl InitialLoadSource for FixtureSource {
        fn capture_source(&self) -> InitialLoadCaptureSource {
            InitialLoadCaptureSource {
                environment: "fixture".to_string(),
                api_base_url: None,
            }
        }

        fn fetch_classification(
            &mut self,
            _classification: NmsNotamClassification,
            _output_gzip_path: &Path,
        ) -> anyhow::Result<()> {
            bail!("fixture Initial Load fetch was not expected")
        }
    }

    impl NmsApiSource for FixtureSource {
        fn fetch_updates(
            &mut self,
            classification: NmsNotamClassification,
            _last_updated_since: DateTime<Utc>,
        ) -> anyhow::Result<Vec<String>> {
            match classification {
                NmsNotamClassification::Domestic => self
                    .domestic_results
                    .pop_front()
                    .context("fixture has no DOMESTIC result")?
                    .map_err(anyhow::Error::msg),
                NmsNotamClassification::Fdc => Ok(Vec::new()),
            }
        }
    }

    fn timestamp(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("test timestamp")
            .with_timezone(&Utc)
    }

    #[allow(clippy::too_many_arguments)]
    fn update_xml(
        nms_id: &str,
        classification: &str,
        location: &str,
        number: &str,
        notam_type: Option<&str>,
        local_text: &str,
        text: &str,
        last_updated: &str,
        canceled: Option<&str>,
        effective_end: &str,
    ) -> String {
        let notam_type = notam_type
            .map(|value| format!("<event:type>{value}</event:type>"))
            .unwrap_or_default();
        let canceled = canceled
            .map(|value| format!("<fnse:canceled>{value}</fnse:canceled>"))
            .unwrap_or_default();
        format!(
            r#"<AIXMBasicMessage xmlns="http://www.aixm.aero/schema/5.1/message"
                xmlns:event="http://www.aixm.aero/schema/5.1/event"
                xmlns:gml="http://www.opengis.net/gml/3.2"
                xmlns:fnse="http://www.aixm.aero/schema/5.1/extensions/FAA/FNSE"
                gml:id="NMS_ID_{nms_id}">
              <hasMember><event:Event><event:timeSlice><event:EventTimeSlice>
                <event:scenario>110</event:scenario>
                <event:textNOTAM><event:NOTAM>
                  <event:number>{number}</event:number>
                  <event:year>2026</event:year>
                  {notam_type}
                  <event:issued>2026-07-23T14:00:00Z</event:issued>
                  <event:location>{location}</event:location>
                  <event:effectiveStart>202607231400</event:effectiveStart>
                  <event:effectiveEnd>{effective_end}</event:effectiveEnd>
                  <event:text>{text}</event:text>
                  <event:translation><event:NOTAMTranslation>
                    <event:type>LOCAL_FORMAT</event:type>
                    <event:simpleText>{local_text}</event:simpleText>
                  </event:NOTAMTranslation></event:translation>
                </event:NOTAM></event:textNOTAM>
                <event:extension><fnse:EventExtension>
                  <fnse:classification>{classification}</fnse:classification>
                  <fnse:lastUpdated>{last_updated}</fnse:lastUpdated>
                  {canceled}
                </fnse:EventExtension></event:extension>
              </event:EventTimeSlice></event:timeSlice></event:Event></hasMember>
            </AIXMBasicMessage>"#
        )
    }
}
