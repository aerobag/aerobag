// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use chrono::{SecondsFormat, Utc};
use preprocessor_core::xz_compress_bytes_with_system_xz;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MANIFEST_SCHEMA_VERSION: u32 = 2;
const MANIFEST_FILE: &str = "capture_manifest.json";
const START_FILE: &str = "start.sqlite";

#[derive(Debug, Clone)]
pub struct CaptureNotamTraceRequest {
    pub start_sqlite: PathBuf,
    pub source_sqlite: PathBuf,
    pub output_dir: PathBuf,
    pub source_commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CaptureManifest {
    schema_version: u32,
    start_ingest_seq: i64,
    end_ingest_seq: i64,
    captured_at_utc: String,
    starting_snapshot: CapturedFile,
    segments: Vec<CaptureSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CapturedFile {
    file: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CaptureSegment {
    file: String,
    source_schema_version: u32,
    source_commit: String,
    cursor_first: i64,
    cursor_last: i64,
    applied_message_count: u64,
    first_received_at_utc: Option<String>,
    last_received_at_utc: Option<String>,
    uncompressed_bytes: u64,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct CapturedRawMessage {
    ingest_seq: i64,
    dedupe_key: String,
    received_at_utc: String,
    applied_at_utc: String,
    message_json: String,
}

pub fn capture_notam_incremental_trace(
    request: &CaptureNotamTraceRequest,
) -> anyhow::Result<PathBuf> {
    if request.source_commit.trim().is_empty() {
        bail!("NOTAM trace source commit cannot be empty");
    }
    let start_ingest_seq = read_fixture_integer(&request.start_sqlite, "start_ingest_seq")?;
    let source_schema_version = read_source_schema_version(&request.source_sqlite)?;
    fs::create_dir_all(request.output_dir.join("segments"))
        .with_context(|| format!("failed to create {}", request.output_dir.display()))?;

    let start_output = request.output_dir.join(START_FILE);
    let starting_snapshot = install_start_snapshot(&request.start_sqlite, &start_output)?;
    let manifest_path = request.output_dir.join(MANIFEST_FILE);
    let mut manifest = if manifest_path.is_file() {
        let manifest: CaptureManifest = serde_json::from_slice(
            &fs::read(&manifest_path)
                .with_context(|| format!("failed to read {}", manifest_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
        validate_existing_manifest(&manifest, start_ingest_seq, &starting_snapshot)?;
        manifest
    } else {
        CaptureManifest {
            schema_version: MANIFEST_SCHEMA_VERSION,
            start_ingest_seq,
            end_ingest_seq: start_ingest_seq,
            captured_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            starting_snapshot,
            segments: Vec::new(),
        }
    };

    let (end_ingest_seq, rows) =
        read_applied_source_suffix(&request.source_sqlite, manifest.end_ingest_seq)?;
    if end_ingest_seq == manifest.end_ingest_seq {
        return Ok(manifest_path);
    }
    let cursor_first = manifest.end_ingest_seq + 1;
    let segment_name = format!("segments/{cursor_first:012}-{end_ingest_seq:012}.jsonl.xz");
    let mut jsonl = Vec::new();
    for row in &rows {
        serde_json::to_writer(&mut jsonl, row).context("failed to encode captured NOTAM row")?;
        jsonl.push(b'\n');
    }
    let encoded = xz_compress_bytes_with_system_xz(&jsonl)
        .map_err(|error| anyhow::anyhow!("failed to compress NOTAM trace segment: {error}"))?;
    let segment_path = request.output_dir.join(&segment_name);
    write_immutable(&segment_path, &encoded)?;
    manifest.segments.push(CaptureSegment {
        file: segment_name,
        source_schema_version,
        source_commit: request.source_commit.clone(),
        cursor_first,
        cursor_last: end_ingest_seq,
        applied_message_count: rows.len() as u64,
        first_received_at_utc: rows.first().map(|row| row.received_at_utc.clone()),
        last_received_at_utc: rows.last().map(|row| row.received_at_utc.clone()),
        uncompressed_bytes: jsonl.len() as u64,
        bytes: encoded.len() as u64,
        sha256: sha256_hex(&encoded),
    });
    manifest.end_ingest_seq = end_ingest_seq;
    manifest.captured_at_utc = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    atomic_write_json(&manifest_path, &manifest)?;
    Ok(manifest_path)
}

fn read_fixture_integer(path: &Path, key: &str) -> anyhow::Result<i64> {
    let connection = open_read_only(path)?;
    let value: String = connection
        .query_row(
            "SELECT value FROM fixture_metadata WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .with_context(|| format!("failed to read {key} from {}", path.display()))?;
    value
        .parse()
        .with_context(|| format!("invalid {key} in {}", path.display()))
}

fn read_source_schema_version(path: &Path) -> anyhow::Result<u32> {
    let connection = open_read_only(path)?;
    let value: String = connection
        .query_row(
            "SELECT value FROM metadata WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .with_context(|| format!("failed to read schema version from {}", path.display()))?;
    value
        .parse()
        .with_context(|| format!("invalid schema version in {}", path.display()))
}

fn read_applied_source_suffix(
    path: &Path,
    after_ingest_seq: i64,
) -> anyhow::Result<(i64, Vec<CapturedRawMessage>)> {
    let mut connection = open_read_only(path)?;
    let tx = connection
        .transaction()
        .with_context(|| format!("failed to start consistent read of {}", path.display()))?;
    let end_ingest_seq = tx
        .query_row(
            "SELECT value FROM metadata WHERE key = 'raw_ingest_cursor'",
            [],
            |row| row.get::<_, String>(0),
        )
        .with_context(|| format!("failed to read raw ingest cursor from {}", path.display()))?
        .parse::<i64>()
        .with_context(|| format!("invalid raw ingest cursor in {}", path.display()))?;
    if end_ingest_seq < after_ingest_seq {
        bail!("NOTAM source cursor moved backward from {after_ingest_seq} to {end_ingest_seq}");
    }
    let rows = {
        let mut statement = tx
            .prepare(
                "SELECT ingest_seq, dedupe_key, received_at_utc, applied_at_utc, message_json
                 FROM raw_notam_messages
                 WHERE ingest_seq > ?1 AND ingest_seq <= ?2 AND applied_at_utc IS NOT NULL
                 ORDER BY ingest_seq",
            )
            .context("failed to prepare NOTAM trace suffix query")?;
        let rows = statement
            .query_map([after_ingest_seq, end_ingest_seq], |row| {
                Ok(CapturedRawMessage {
                    ingest_seq: row.get(0)?,
                    dedupe_key: row.get(1)?,
                    received_at_utc: row.get(2)?,
                    applied_at_utc: row.get(3)?,
                    message_json: row.get(4)?,
                })
            })
            .context("failed to query NOTAM trace suffix")?
            .collect::<Result<Vec<_>, _>>()
            .context("failed to read NOTAM trace suffix")?;
        rows
    };
    tx.commit().context("failed to finish NOTAM trace read")?;
    Ok((end_ingest_seq, rows))
}

fn open_read_only(path: &Path) -> anyhow::Result<Connection> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("failed to open {}", path.display()))
}

fn install_start_snapshot(source: &Path, target: &Path) -> anyhow::Result<CapturedFile> {
    let source_bytes =
        fs::read(source).with_context(|| format!("failed to read {}", source.display()))?;
    if target.is_file() {
        let existing =
            fs::read(target).with_context(|| format!("failed to read {}", target.display()))?;
        if existing != source_bytes {
            bail!(
                "captured NOTAM starting snapshot changed at {}",
                target.display()
            );
        }
    } else {
        write_immutable(target, &source_bytes)?;
    }
    Ok(CapturedFile {
        file: START_FILE.to_string(),
        bytes: source_bytes.len() as u64,
        sha256: sha256_hex(&source_bytes),
    })
}

fn validate_existing_manifest(
    manifest: &CaptureManifest,
    start_ingest_seq: i64,
    starting_snapshot: &CapturedFile,
) -> anyhow::Result<()> {
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION
        || manifest.start_ingest_seq != start_ingest_seq
        || manifest.starting_snapshot.sha256 != starting_snapshot.sha256
        || manifest.starting_snapshot.bytes != starting_snapshot.bytes
    {
        bail!("existing NOTAM trace manifest does not match its inputs");
    }
    if manifest
        .segments
        .windows(2)
        .any(|pair| pair[0].cursor_last + 1 != pair[1].cursor_first)
        || manifest
            .segments
            .first()
            .is_some_and(|segment| segment.cursor_first != start_ingest_seq + 1)
        || manifest
            .segments
            .last()
            .is_some_and(|segment| segment.cursor_last != manifest.end_ingest_seq)
    {
        bail!("existing NOTAM trace segments do not form one contiguous cursor range");
    }
    Ok(())
}

fn write_immutable(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if path.is_file() {
        let existing =
            fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
        if existing != bytes {
            bail!("immutable NOTAM trace file changed at {}", path.display());
        }
        return Ok(());
    }
    atomic_write(path, bytes)
}

fn atomic_write_json(path: &Path, value: &impl Serialize) -> anyhow::Result<()> {
    let mut bytes =
        serde_json::to_vec_pretty(value).context("failed to encode capture manifest")?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let temp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .context("capture output has no UTF-8 file name")?,
        std::process::id()
    ));
    {
        let mut file =
            File::create(&temp).with_context(|| format!("failed to create {}", temp.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("failed to write {}", temp.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync {}", temp.display()))?;
    }
    fs::rename(&temp, path).with_context(|| format!("failed to promote {}", path.display()))?;
    File::open(parent)
        .with_context(|| format!("failed to open {}", parent.display()))?
        .sync_all()
        .with_context(|| format!("failed to sync {}", parent.display()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use tempfile::tempdir;

    #[test]
    fn capture_appends_contiguous_immutable_segments_idempotently() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let start = temp.path().join("fixture-start.sqlite");
        let source = temp.path().join("source.sqlite");
        let output = temp.path().join("capture");
        let start_connection = Connection::open(&start)?;
        start_connection.execute_batch(
            "CREATE TABLE fixture_metadata(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO fixture_metadata(key, value) VALUES ('start_ingest_seq', '0');",
        )?;
        drop(start_connection);

        let source_connection = Connection::open(&source)?;
        source_connection.execute_batch(
            "CREATE TABLE metadata(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO metadata(key, value) VALUES ('schema_version', '6');
             INSERT INTO metadata(key, value) VALUES ('raw_ingest_cursor', '3');
             CREATE TABLE raw_notam_messages(
                 ingest_seq INTEGER PRIMARY KEY,
                 dedupe_key TEXT NOT NULL,
                 received_at_utc TEXT NOT NULL,
                 applied_at_utc TEXT,
                 message_json TEXT NOT NULL
             );",
        )?;
        for (seq, applied) in [(1_i64, true), (2, false), (3, true)] {
            source_connection.execute(
                "INSERT INTO raw_notam_messages(
                    ingest_seq, dedupe_key, received_at_utc, applied_at_utc, message_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    seq,
                    format!("key-{seq}"),
                    format!("2026-07-23T00:00:0{seq}Z"),
                    applied.then_some("2026-07-23T00:01:00Z"),
                    format!(r#"{{"seq":{seq}}}"#)
                ],
            )?;
        }
        drop(source_connection);
        let request = CaptureNotamTraceRequest {
            start_sqlite: start,
            source_sqlite: source.clone(),
            output_dir: output.clone(),
            source_commit: "abc123".to_string(),
        };
        let manifest_path = capture_notam_incremental_trace(&request)?;
        let first: CaptureManifest = serde_json::from_slice(&fs::read(&manifest_path)?)?;
        assert_eq!(first.end_ingest_seq, 3);
        assert_eq!(first.segments.len(), 1);
        assert_eq!(first.segments[0].applied_message_count, 2);

        capture_notam_incremental_trace(&request)?;
        let repeated: CaptureManifest = serde_json::from_slice(&fs::read(&manifest_path)?)?;
        assert_eq!(repeated.segments.len(), 1);

        let source_connection = Connection::open(&source)?;
        source_connection.execute(
            "UPDATE metadata SET value = '7' WHERE key = 'schema_version'",
            [],
        )?;
        source_connection.execute(
            "INSERT INTO raw_notam_messages(
                ingest_seq, dedupe_key, received_at_utc, applied_at_utc, message_json
             ) VALUES (4, 'key-4', '2026-07-23T00:00:04Z',
                       '2026-07-23T00:01:00Z', '{\"seq\":4}')",
            [],
        )?;
        source_connection.execute(
            "UPDATE metadata SET value = '4' WHERE key = 'raw_ingest_cursor'",
            [],
        )?;
        drop(source_connection);
        let updated_request = CaptureNotamTraceRequest {
            source_commit: "def456".to_string(),
            ..request
        };
        capture_notam_incremental_trace(&updated_request)?;
        let appended: CaptureManifest = serde_json::from_slice(&fs::read(&manifest_path)?)?;
        assert_eq!(appended.end_ingest_seq, 4);
        assert_eq!(appended.segments.len(), 2);
        assert_eq!(appended.segments[1].cursor_first, 4);
        assert_eq!(appended.segments[1].cursor_last, 4);
        assert_eq!(appended.segments[0].source_schema_version, 6);
        assert_eq!(appended.segments[0].source_commit, "abc123");
        assert_eq!(appended.segments[1].source_schema_version, 7);
        assert_eq!(appended.segments[1].source_commit, "def456");
        Ok(())
    }
}
