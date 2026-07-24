// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use chrono::{Duration, NaiveDateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::StructuredTfrNotamMetadata;

const TFR_DETAIL_BACKFILL_SCHEMA_VERSION: u32 = 1;
const TFR_DETAIL_FETCH_RETRY_BASE_SECONDS: i64 = 5 * 60;
const TFR_DETAIL_FETCH_RETRY_MAX_SECONDS: i64 = 60 * 60;

#[derive(Debug, Clone)]
pub struct TfrDetailBackfillStore {
    root: PathBuf,
}

#[derive(Debug)]
pub struct TfrDetailBackfillLock {
    file: File,
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TfrDetailFetchTarget {
    pub tfr_id: String,
    pub source_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TfrDetailBackfillStoreSummary {
    pub desired: usize,
    pub stored: usize,
    pub failures: usize,
    pub remaining_unfetched: usize,
    pub due: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParsedTfrDetailBackfill {
    pub tfr_id: String,
    pub source_url: String,
    pub metadata: StructuredTfrNotamMetadata,
}

impl TfrDetailBackfillStore {
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

    pub fn acquire_lock(&self) -> anyhow::Result<TfrDetailBackfillLock> {
        fs::create_dir_all(&self.root)
            .with_context(|| format!("failed to create {}", self.root.display()))?;
        let path = self.root.join("lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)
            .with_context(|| {
                format!("failed to open TFR detail backfill lock {}", path.display())
            })?;
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc != 0 {
            let error = std::io::Error::last_os_error();
            if matches!(error.kind(), ErrorKind::WouldBlock) {
                bail!(
                    "TFR detail backfill store is already locked by another process: {}",
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
        Ok(TfrDetailBackfillLock { file, path })
    }

    pub fn record_desired_tfrs(&self, tfr_ids: &BTreeSet<String>) -> anyhow::Result<()> {
        let mut connection = self.open_connection()?;
        let tx = connection
            .transaction()
            .context("failed to start TFR detail desired transaction")?;
        let observed_at_utc = Utc::now().to_rfc3339();
        for tfr_id in tfr_ids {
            let source_url = detail_url_for_tfr_id(tfr_id)?;
            tx.execute(
                "INSERT INTO desired_tfrs (tfr_id, source_url, last_seen_at_utc)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(tfr_id) DO UPDATE SET
                    source_url = excluded.source_url,
                    last_seen_at_utc = excluded.last_seen_at_utc",
                params![tfr_id, source_url, observed_at_utc],
            )
            .with_context(|| format!("failed to record desired TFR detail {tfr_id}"))?;
        }
        tx.commit()
            .context("failed to commit TFR detail desired transaction")?;
        Ok(())
    }

    pub fn due_fetch_targets(&self, limit: usize) -> anyhow::Result<Vec<TfrDetailFetchTarget>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let connection = self.open_connection()?;
        let now = Utc::now().to_rfc3339();
        let mut statement = connection
            .prepare(
                "SELECT desired_tfrs.tfr_id, desired_tfrs.source_url
                 FROM desired_tfrs
                 LEFT JOIN detail_records ON detail_records.tfr_id = desired_tfrs.tfr_id
                 LEFT JOIN fetch_failures ON fetch_failures.tfr_id = desired_tfrs.tfr_id
                 WHERE detail_records.tfr_id IS NULL
                   AND (
                     fetch_failures.next_retry_after_utc IS NULL
                     OR fetch_failures.next_retry_after_utc <= ?1
                   )
                 ORDER BY desired_tfrs.last_seen_at_utc DESC, desired_tfrs.tfr_id
                 LIMIT ?2",
            )
            .context("failed to prepare TFR detail due query")?;
        let rows = statement
            .query_map(params![now, limit as i64], |row| {
                Ok(TfrDetailFetchTarget {
                    tfr_id: row.get(0)?,
                    source_url: row.get(1)?,
                })
            })
            .context("failed to query due TFR details")?;
        let mut targets = Vec::new();
        for row in rows {
            targets.push(row.context("failed to read due TFR detail row")?);
        }
        Ok(targets)
    }

    pub fn summary(&self) -> anyhow::Result<TfrDetailBackfillStoreSummary> {
        let connection = self.open_connection()?;
        let now = Utc::now().to_rfc3339();
        let desired = scalar_count(&connection, "SELECT COUNT(*) FROM desired_tfrs", [])?;
        let stored = scalar_count(&connection, "SELECT COUNT(*) FROM detail_records", [])?;
        let failures = scalar_count(&connection, "SELECT COUNT(*) FROM fetch_failures", [])?;
        let remaining_unfetched = scalar_count(
            &connection,
            "SELECT COUNT(*)
             FROM desired_tfrs
             LEFT JOIN detail_records ON detail_records.tfr_id = desired_tfrs.tfr_id
             WHERE detail_records.tfr_id IS NULL",
            [],
        )?;
        let due = scalar_count(
            &connection,
            "SELECT COUNT(*)
             FROM desired_tfrs
             LEFT JOIN detail_records ON detail_records.tfr_id = desired_tfrs.tfr_id
             LEFT JOIN fetch_failures ON fetch_failures.tfr_id = desired_tfrs.tfr_id
             WHERE detail_records.tfr_id IS NULL
               AND (
                 fetch_failures.next_retry_after_utc IS NULL
                 OR fetch_failures.next_retry_after_utc <= ?1
               )",
            [now],
        )?;
        Ok(TfrDetailBackfillStoreSummary {
            desired,
            stored,
            failures,
            remaining_unfetched,
            due,
        })
    }

    pub fn current_metadata_by_fdc_id(
        &self,
        tfr_ids: &BTreeSet<String>,
    ) -> anyhow::Result<Vec<(String, StructuredTfrNotamMetadata)>> {
        if tfr_ids.is_empty() {
            return Ok(Vec::new());
        }
        let connection = self.open_connection()?;
        let mut statement = connection
            .prepare("SELECT source_url, raw_xml FROM detail_records WHERE tfr_id = ?1")
            .context("failed to prepare TFR detail metadata query")?;
        let mut metadata = Vec::new();
        for tfr_id in tfr_ids {
            let rows = statement
                .query_map([tfr_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .with_context(|| format!("failed to query TFR detail metadata {tfr_id}"))?;
            for row in rows {
                let (source_url, raw_xml) =
                    row.with_context(|| format!("failed to read TFR detail metadata {tfr_id}"))?;
                let parsed = parse_tfr_detail_xml(tfr_id, &source_url, &raw_xml)
                    .with_context(|| format!("failed to parse stored TFR detail {tfr_id}"))?;
                metadata.push((tfr_id.clone(), parsed.metadata));
            }
        }
        Ok(metadata)
    }

    pub fn record_success(
        &self,
        target: &TfrDetailFetchTarget,
        xml: &str,
    ) -> anyhow::Result<ParsedTfrDetailBackfill> {
        let parsed = parse_tfr_detail_xml(&target.tfr_id, &target.source_url, xml)?;
        let metadata_json = serde_json::to_string(&parsed.metadata)
            .with_context(|| format!("failed to encode TFR detail {}", target.tfr_id))?;
        let hash = Sha256::digest(xml.as_bytes());
        let sha256 = format!("{hash:x}");
        let fetched_at_utc = Utc::now().to_rfc3339();
        let mut connection = self.open_connection()?;
        let tx = connection
            .transaction()
            .context("failed to start TFR detail success transaction")?;
        tx.execute(
            "INSERT INTO detail_records (
                tfr_id, source_url, fetched_at_utc, sha256, raw_xml, metadata_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(tfr_id) DO UPDATE SET
                source_url = excluded.source_url,
                fetched_at_utc = excluded.fetched_at_utc,
                sha256 = excluded.sha256,
                raw_xml = excluded.raw_xml,
                metadata_json = excluded.metadata_json",
            params![
                target.tfr_id,
                target.source_url,
                fetched_at_utc,
                sha256,
                xml,
                metadata_json,
            ],
        )
        .with_context(|| format!("failed to upsert TFR detail {}", target.tfr_id))?;
        tx.execute(
            "DELETE FROM fetch_failures WHERE tfr_id = ?1",
            [&target.tfr_id],
        )
        .with_context(|| format!("failed to clear TFR detail failure {}", target.tfr_id))?;
        tx.commit()
            .context("failed to commit TFR detail success transaction")?;
        Ok(parsed)
    }

    pub fn record_failure(
        &self,
        target: &TfrDetailFetchTarget,
        error: impl ToString,
    ) -> anyhow::Result<()> {
        let error = error.to_string();
        let mut connection = self.open_connection()?;
        let tx = connection
            .transaction()
            .context("failed to start TFR detail failure transaction")?;
        let consecutive_failures: i64 = tx
            .query_row(
                "SELECT consecutive_failures FROM fetch_failures WHERE tfr_id = ?1",
                [&target.tfr_id],
                |row| row.get(0),
            )
            .unwrap_or(0_i64)
            .saturating_add(1);
        let now = Utc::now();
        let delay_seconds = TFR_DETAIL_FETCH_RETRY_BASE_SECONDS
            .saturating_mul(1_i64 << consecutive_failures.saturating_sub(1).min(4))
            .min(TFR_DETAIL_FETCH_RETRY_MAX_SECONDS);
        let next_retry_after_utc = (now + Duration::seconds(delay_seconds)).to_rfc3339();
        tx.execute(
            "INSERT INTO fetch_failures (
                tfr_id, source_url, last_failure_at_utc, next_retry_after_utc,
                consecutive_failures, last_error
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(tfr_id) DO UPDATE SET
                source_url = excluded.source_url,
                last_failure_at_utc = excluded.last_failure_at_utc,
                next_retry_after_utc = excluded.next_retry_after_utc,
                consecutive_failures = excluded.consecutive_failures,
                last_error = excluded.last_error",
            params![
                target.tfr_id,
                target.source_url,
                now.to_rfc3339(),
                next_retry_after_utc,
                consecutive_failures,
                error,
            ],
        )
        .with_context(|| format!("failed to record TFR detail failure {}", target.tfr_id))?;
        tx.commit()
            .context("failed to commit TFR detail failure transaction")?;
        Ok(())
    }

    pub fn sqlite_path(&self) -> PathBuf {
        self.state_root().join("current.sqlite")
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
                CREATE TABLE IF NOT EXISTS desired_tfrs (
                    tfr_id TEXT PRIMARY KEY,
                    source_url TEXT NOT NULL,
                    last_seen_at_utc TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS detail_records (
                    tfr_id TEXT PRIMARY KEY,
                    source_url TEXT NOT NULL,
                    fetched_at_utc TEXT NOT NULL,
                    sha256 TEXT NOT NULL,
                    raw_xml TEXT NOT NULL,
                    metadata_json TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS fetch_failures (
                    tfr_id TEXT PRIMARY KEY,
                    source_url TEXT NOT NULL,
                    last_failure_at_utc TEXT NOT NULL,
                    next_retry_after_utc TEXT NOT NULL,
                    consecutive_failures INTEGER NOT NULL,
                    last_error TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS tfr_detail_failures_retry_idx
                    ON fetch_failures(next_retry_after_utc);
                ",
            )
            .context("failed to initialize TFR detail backfill sqlite schema")?;
        connection
            .execute(
                "INSERT INTO metadata(key, value) VALUES ('schema_version', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [TFR_DETAIL_BACKFILL_SCHEMA_VERSION.to_string()],
            )
            .context("failed to record TFR detail backfill sqlite schema version")?;
        Ok(connection)
    }

    fn state_root(&self) -> PathBuf {
        self.root.join("state")
    }
}

fn scalar_count<P>(connection: &Connection, sql: &str, params: P) -> anyhow::Result<usize>
where
    P: rusqlite::Params,
{
    let count = connection
        .query_row(sql, params, |row| row.get::<_, i64>(0))
        .context("failed to query TFR detail backfill count")?;
    Ok(count.max(0) as usize)
}

impl Drop for TfrDetailBackfillLock {
    fn drop(&mut self) {
        let _ = unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
        let _ = self.file.sync_all();
        let _ = &self.path;
    }
}

pub fn detail_url_for_tfr_id(tfr_id: &str) -> anyhow::Result<String> {
    validate_tfr_id(tfr_id)?;
    Ok(format!(
        "https://tfr.faa.gov/download/detail_{}.xml",
        tfr_id.replace('/', "_")
    ))
}

pub fn parse_tfr_detail_xml(
    tfr_id: &str,
    source_url: &str,
    xml: &str,
) -> anyhow::Result<ParsedTfrDetailBackfill> {
    validate_tfr_id(tfr_id)?;
    let xml = xml.trim_start_matches('\u{feff}');
    let document = roxmltree::Document::parse(xml)
        .with_context(|| format!("failed to parse TFR detail XML for {tfr_id}"))?;
    let number = child_text(&document, "noSeqNo").unwrap_or_else(|| {
        tfr_id
            .split_once('/')
            .map(|(_, number)| number.to_string())
            .unwrap()
    });
    let year = child_text(&document, "dateIndexYear").unwrap_or_else(|| "20??".to_string());
    let facility = child_text(&document, "codeFacility");
    let city = child_text(&document, "txtNameCity");
    let state = child_text(&document, "txtNameUSState");
    let issued_utc = child_text(&document, "dateIssued").map(normalize_detail_timestamp);
    let effective_start_utc =
        child_text(&document, "dateEffective").map(normalize_detail_timestamp);
    let effective_end_utc = child_text(&document, "dateExpire").map(normalize_detail_timestamp);
    let lower = detail_limit_text(
        child_text(&document, "valDistVerLower"),
        child_text(&document, "uomDistVerLower"),
        true,
    );
    let upper = detail_limit_text(
        child_text(&document, "valDistVerUpper"),
        child_text(&document, "uomDistVerUpper"),
        false,
    );
    let location = match (city.as_deref(), state.as_deref()) {
        (Some(city), Some(state)) => format!("{city}, {state}"),
        (Some(city), None) => city.to_string(),
        (None, Some(state)) => state.to_string(),
        (None, None) => tfr_id.to_string(),
    };
    let altitude_pair = match (&lower, &upper) {
        (Some(lower), Some(upper)) => format!("{lower}-{upper}"),
        _ => "altitudes unavailable".to_string(),
    };
    let summary_text = format!(
        "{}..AIRSPACE {}..TEMPORARY FLIGHT RESTRICTIONS. {}.{}{}",
        state.as_deref().unwrap_or("US"),
        location,
        altitude_pair,
        effective_start_utc
            .as_deref()
            .map(|value| format!(" Effective {value}."))
            .unwrap_or_default(),
        effective_end_utc
            .as_deref()
            .map(|value| format!(" Expires {value}."))
            .unwrap_or_default()
    );
    let text = child_text(&document, "txtDescrTraditional")
        .or_else(|| child_text(&document, "txtDescrUSNS"))
        .unwrap_or_else(|| match child_text(&document, "txtDescrPurpose") {
            Some(purpose) => format!("{summary_text} {purpose}"),
            None => summary_text.clone(),
        });
    let record_id = match (&facility, year.as_str()) {
        (Some(facility), year) if year.chars().all(|ch| ch.is_ascii_digit()) => {
            format!("F:{facility}:{year}:N:{number}")
        }
        _ => format!("FAA_TFR_DETAIL:{tfr_id}"),
    };
    Ok(ParsedTfrDetailBackfill {
        tfr_id: tfr_id.to_string(),
        source_url: source_url.to_string(),
        metadata: StructuredTfrNotamMetadata {
            record_id,
            source_type: Some("FAA_TFR_DETAIL".to_string()),
            status: Some("PUBLISHED".to_string()),
            function: Some("N".to_string()),
            keyword: Some("AIRSPACE".to_string()),
            facility,
            issued_utc,
            effective_start_utc,
            effective_end_utc,
            text: Some(text.clone()),
            local_text: Some(text),
            icao_text: None,
        },
    })
}

fn validate_tfr_id(tfr_id: &str) -> anyhow::Result<()> {
    let Some((year_digit, sequence)) = tfr_id.split_once('/') else {
        bail!("invalid TFR id {tfr_id}: expected D/NNNN");
    };
    if year_digit.len() != 1 || !year_digit.chars().all(|ch| ch.is_ascii_digit()) {
        bail!("invalid TFR id {tfr_id}: expected one year digit before slash");
    }
    if sequence.len() != 4 || !sequence.chars().all(|ch| ch.is_ascii_digit()) {
        bail!("invalid TFR id {tfr_id}: expected four sequence digits after slash");
    }
    Ok(())
}

fn child_text(document: &roxmltree::Document<'_>, local_name: &str) -> Option<String> {
    document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == local_name)
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn detail_limit_text(value: Option<String>, unit: Option<String>, lower: bool) -> Option<String> {
    let value = value?.trim().to_ascii_uppercase();
    let unit = unit.unwrap_or_default().trim().to_ascii_uppercase();
    if lower && (value == "0" || value == "SFC") {
        return Some("SFC".to_string());
    }
    if value.is_empty() {
        return None;
    }
    if unit.is_empty() {
        Some(value)
    } else {
        Some(format!("{value}{unit}"))
    }
}

fn normalize_detail_timestamp(value: String) -> String {
    let value = value.trim();
    if value.ends_with('Z') || value.contains('+') {
        return value.to_string();
    }
    if let Ok(timestamp) = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S") {
        return timestamp.and_utc().to_rfc3339();
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_url_uses_tfr_download_contract() {
        assert_eq!(
            detail_url_for_tfr_id("6/8212").unwrap(),
            "https://tfr.faa.gov/download/detail_6_8212.xml"
        );
        assert!(detail_url_for_tfr_id("bad").is_err());
    }

    #[test]
    fn parser_normalizes_tfr_detail_into_metadata() {
        let xml = r#"
        <XNOTAM-Update>
          <Group><Add><Not>
            <NotUid>
              <txtNameAcctFac>FDC</txtNameAcctFac>
              <dateIndexYear>2026</dateIndexYear>
              <noSeqNo>8212</noSeqNo>
              <dateIssued>2026-06-14T16:55:05</dateIssued>
            </NotUid>
            <dateEffective>2026-06-17T04:01:00</dateEffective>
            <dateExpire>2026-07-17T03:59:00</dateExpire>
            <AffLocGroup>
              <txtNameCity>Kennesaw</txtNameCity>
              <txtNameUSState>GEORGIA</txtNameUSState>
            </AffLocGroup>
            <codeFacility>ZTL</codeFacility>
            <txtDescrTraditional>!FDC 6/8212 ZTL GA..AIRSPACE KENNESAW, GA..TEMPORARY FLIGHT RESTRICTIONS. PURSUANT TO 14 CFR SECTION 91.145, ACFT OPS ARE PROHIBITED WI AN AREA DEFINED AS 2NM RADIUS SFC-400FT MSL EFFECTIVE 2606170401 UTC UNTIL 2607170359 UTC.</txtDescrTraditional>
            <TfrNot><TFRAreaGroup><aseTFRArea>
              <valDistVerUpper>400</valDistVerUpper>
              <uomDistVerUpper>FT</uomDistVerUpper>
              <valDistVerLower>0</valDistVerLower>
              <uomDistVerLower>FT</uomDistVerLower>
            </aseTFRArea></TFRAreaGroup></TfrNot>
          </Not></Add></Group>
        </XNOTAM-Update>
        "#;
        let parsed = parse_tfr_detail_xml("6/8212", "https://example.test/detail.xml", xml)
            .expect("parsed detail");
        assert_eq!(parsed.metadata.record_id, "F:ZTL:2026:N:8212");
        assert_eq!(parsed.metadata.facility.as_deref(), Some("ZTL"));
        assert_eq!(
            parsed.metadata.effective_start_utc.as_deref(),
            Some("2026-06-17T04:01:00+00:00")
        );
        let text = parsed.metadata.text.as_deref().expect("text");
        assert!(text.contains("TEMPORARY FLIGHT RESTRICTIONS"));
        assert!(text.contains("PURSUANT TO 14 CFR SECTION 91.145"));
        assert!(text.contains("SFC-400FT"));
    }

    #[test]
    fn parser_falls_back_to_synthesized_text_without_body() {
        let xml = r#"
        <XNOTAM-Update>
          <Not>
            <NotUid><dateIndexYear>2026</dateIndexYear><noSeqNo>8212</noSeqNo></NotUid>
            <codeFacility>ZTL</codeFacility>
            <dateEffective>2026-06-17T04:01:00</dateEffective>
            <dateExpire>2026-07-17T03:59:00</dateExpire>
            <txtNameCity>Kennesaw</txtNameCity>
            <txtNameUSState>GEORGIA</txtNameUSState>
            <valDistVerUpper>400</valDistVerUpper>
            <uomDistVerUpper>FT</uomDistVerUpper>
            <valDistVerLower>0</valDistVerLower>
            <uomDistVerLower>FT</uomDistVerLower>
          </Not>
        </XNOTAM-Update>
        "#;
        let parsed = parse_tfr_detail_xml("6/8212", "https://example.test/detail.xml", xml)
            .expect("parsed detail");
        let text = parsed.metadata.text.as_deref().expect("text");
        assert!(text.contains("TEMPORARY FLIGHT RESTRICTIONS"));
        assert!(text.contains("SFC-400FT"));
        assert!(text.contains("Expires 2026-07-17T03:59:00+00:00"));
    }

    #[test]
    fn store_records_desired_tfrs_and_successful_details() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = TfrDetailBackfillStore::new(temp.path());
        store.initialize().expect("initialize");
        store
            .record_desired_tfrs(&BTreeSet::from(["6/8212".to_string()]))
            .expect("desired");
        let targets = store.due_fetch_targets(10).expect("targets");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].tfr_id, "6/8212");
        store
            .record_success(
                &targets[0],
                r#"
                <XNOTAM-Update>
                  <Not>
                    <NotUid><dateIndexYear>2026</dateIndexYear><noSeqNo>8212</noSeqNo></NotUid>
                    <codeFacility>ZTL</codeFacility>
                    <dateEffective>2026-06-17T04:01:00</dateEffective>
                    <dateExpire>2026-07-17T03:59:00</dateExpire>
                    <txtNameCity>Kennesaw</txtNameCity>
                    <txtNameUSState>GEORGIA</txtNameUSState>
                    <txtDescrTraditional>!FDC 6/8212 ZTL GA..AIRSPACE KENNESAW, GA..TEMPORARY FLIGHT RESTRICTIONS. FULL BODY.</txtDescrTraditional>
                    <valDistVerUpper>400</valDistVerUpper>
                    <uomDistVerUpper>FT</uomDistVerUpper>
                    <valDistVerLower>0</valDistVerLower>
                    <uomDistVerLower>FT</uomDistVerLower>
                  </Not>
                </XNOTAM-Update>
                "#,
            )
            .expect("success");
        assert!(store.due_fetch_targets(10).expect("targets").is_empty());
        let records = store
            .current_metadata_by_fdc_id(&BTreeSet::from(["6/8212".to_string()]))
            .expect("metadata");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, "6/8212");
        assert!(records[0]
            .1
            .text
            .as_deref()
            .expect("text")
            .contains("FULL BODY"));
    }

    #[test]
    fn store_reparses_raw_xml_instead_of_trusting_stale_metadata() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = TfrDetailBackfillStore::new(temp.path());
        store.initialize().expect("initialize");
        let connection = store.open_connection().expect("connection");
        connection
            .execute(
                "INSERT INTO detail_records (
                    tfr_id, source_url, fetched_at_utc, sha256, raw_xml, metadata_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    "6/8212",
                    "https://example.test/detail.xml",
                    "2026-07-12T00:00:00Z",
                    "stale",
                    r#"
                    <XNOTAM-Update>
                      <Not>
                        <NotUid><dateIndexYear>2026</dateIndexYear><noSeqNo>8212</noSeqNo></NotUid>
                        <codeFacility>ZTL</codeFacility>
                        <txtDescrTraditional>!FDC 6/8212 ZTL GA..AIRSPACE KENNESAW FULL FAA BODY.</txtDescrTraditional>
                      </Not>
                    </XNOTAM-Update>
                    "#,
                    r#"{"record_id":"STALE","text":"stale terse text"}"#,
                ],
            )
            .expect("insert");
        let records = store
            .current_metadata_by_fdc_id(&BTreeSet::from(["6/8212".to_string()]))
            .expect("metadata");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].1.record_id, "F:ZTL:2026:N:8212");
        assert_eq!(
            records[0].1.text.as_deref(),
            Some("!FDC 6/8212 ZTL GA..AIRSPACE KENNESAW FULL FAA BODY.")
        );
    }
}
