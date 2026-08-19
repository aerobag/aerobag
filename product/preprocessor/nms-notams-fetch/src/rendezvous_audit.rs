// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use had_nav_kv::NavKvRoot;
use preprocessor_live_feeds::nms_initial_load::{
    parse_nms_api_update, parse_nms_initial_load, NmsNotamClassification,
};
use preprocessor_live_feeds::StructuredNotamRecord;
use product_contracts::ProcedureRendezvousKey;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

const PROCEDURE_KEYWORDS: &[&str] = &["IAP", "ODP", "SID", "STAR"];

pub struct RendezvousAuditOptions {
    pub state_root: PathBuf,
    pub nav_db_dir: PathBuf,
    pub initial_load_path: Option<PathBuf>,
    pub keyword: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RendezvousAuditReport {
    pub generated_at_utc: String,
    pub elapsed_ms: u128,
    pub inputs: RendezvousAuditInputs,
    pub source: RendezvousAuditSourceSummary,
    pub nav_rendezvous_key_count: usize,
    pub procedure_records: usize,
    pub summaries: BTreeMap<String, RendezvousAuditSummary>,
    pub findings: Vec<RendezvousAuditFinding>,
    pub missing_active_source_versions: Vec<MissingActiveSourceVersion>,
}

#[derive(Debug, Serialize)]
pub struct RendezvousAuditInputs {
    pub collector_db: String,
    pub initial_load: String,
    pub nav_db_dir: String,
    pub keyword: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct RendezvousAuditSourceSummary {
    pub baseline_records: usize,
    pub retained_raw_updates: usize,
    pub reconstructed_source_versions: usize,
    pub active_fdc_versions: usize,
}

#[derive(Debug, Default, Serialize)]
pub struct RendezvousAuditSummary {
    pub records: usize,
    pub keyed_records: usize,
    pub matched_records: usize,
    pub unkeyed_records: usize,
    pub keyed_without_nav_match: usize,
    pub generated_keys: usize,
    pub matched_keys: usize,
}

#[derive(Debug, Serialize)]
pub struct RendezvousAuditFinding {
    pub id: String,
    pub keyword: String,
    pub airport_id: Option<String>,
    pub last_updated_utc: Option<String>,
    pub outcome: &'static str,
    pub generated_nav_keys: Vec<String>,
    pub available_nav_key_count: usize,
    pub available_nav_keys: Vec<String>,
    pub text_preview: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MissingActiveSourceVersion {
    pub id: String,
    pub last_updated_utc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SourceVersion {
    id: String,
    last_updated_utc: DateTime<Utc>,
}

struct CollectorSnapshot {
    baseline_capture_path: PathBuf,
    active_versions: BTreeSet<SourceVersion>,
    raw_updates: Vec<String>,
}

pub fn audit_rendezvous(options: &RendezvousAuditOptions) -> anyhow::Result<RendezvousAuditReport> {
    let started = Instant::now();
    let collector_db = options.state_root.join("state.sqlite");
    let collector = read_collector_snapshot(&collector_db)?;
    let initial_load_path = options
        .initial_load_path
        .clone()
        .unwrap_or_else(|| collector.baseline_capture_path.join("fdc.xml"));

    let input = File::open(&initial_load_path)
        .with_context(|| format!("failed to open {}", initial_load_path.display()))?;
    let baseline = parse_nms_initial_load(BufReader::new(input), NmsNotamClassification::Fdc)
        .with_context(|| format!("failed to parse {}", initial_load_path.display()))?;
    baseline.validate_complete()?;
    let baseline_records = baseline.records.len();

    let retained_raw_updates = collector.raw_updates.len();
    let mut candidates = BTreeMap::<SourceVersion, StructuredNotamRecord>::new();
    for record in baseline.records {
        insert_candidate(&mut candidates, record)?;
    }
    for xml in collector.raw_updates {
        let update = parse_nms_api_update(&xml, NmsNotamClassification::Fdc)
            .context("failed to reparse retained FDC update")?;
        insert_candidate(&mut candidates, update.record)?;
    }
    let reconstructed_source_versions = candidates.len();

    let mut active_records = Vec::new();
    let mut missing_active_source_versions = Vec::new();
    for version in &collector.active_versions {
        match candidates.remove(version) {
            Some(record) => active_records.push(record),
            None => missing_active_source_versions.push(MissingActiveSourceVersion {
                id: version.id.clone(),
                last_updated_utc: version.last_updated_utc.to_rfc3339(),
            }),
        }
    }

    let nav_keys = read_nav_rendezvous_keys(&options.nav_db_dir)?;
    let (summaries, findings, procedure_records) =
        audit_records(active_records, &nav_keys, options.keyword.as_deref())?;

    Ok(RendezvousAuditReport {
        generated_at_utc: Utc::now().to_rfc3339(),
        elapsed_ms: started.elapsed().as_millis(),
        inputs: RendezvousAuditInputs {
            collector_db: collector_db.display().to_string(),
            initial_load: initial_load_path.display().to_string(),
            nav_db_dir: options.nav_db_dir.display().to_string(),
            keyword: options.keyword.clone(),
        },
        source: RendezvousAuditSourceSummary {
            baseline_records,
            retained_raw_updates,
            reconstructed_source_versions,
            active_fdc_versions: collector.active_versions.len(),
        },
        nav_rendezvous_key_count: nav_keys.len(),
        procedure_records,
        summaries,
        findings,
        missing_active_source_versions,
    })
}

fn read_collector_snapshot(path: &Path) -> anyhow::Result<CollectorSnapshot> {
    let mut connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("failed to open collector database {}", path.display()))?;
    connection.busy_timeout(Duration::from_secs(5))?;
    let transaction = connection.transaction()?;

    let baseline_capture_path = transaction
        .query_row(
            "SELECT value FROM metadata WHERE key = 'baseline_capture_path'",
            [],
            |row| row.get::<_, String>(0),
        )
        .context("collector database has no baseline_capture_path metadata")?;

    let active_versions = {
        let mut statement = transaction.prepare(
            "SELECT id, last_updated_utc
             FROM current_notams
             WHERE json_extract(record_json, '$.source_type') = 'F'",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.map(|row| {
            let (id, last_updated_utc) = row?;
            source_version(id, &last_updated_utc)
        })
        .collect::<anyhow::Result<BTreeSet<_>>>()?
    };

    let raw_updates = {
        let mut statement = transaction.prepare(
            "SELECT raw_aixm
             FROM raw_updates
             WHERE classification = 'FDC'
             ORDER BY poll_id, rowid",
        )?;
        let updates = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        updates
    };
    transaction.commit()?;

    Ok(CollectorSnapshot {
        baseline_capture_path: PathBuf::from(baseline_capture_path),
        active_versions,
        raw_updates,
    })
}

fn insert_candidate(
    candidates: &mut BTreeMap<SourceVersion, StructuredNotamRecord>,
    record: StructuredNotamRecord,
) -> anyhow::Result<()> {
    let last_updated_utc = record
        .last_updated_utc
        .as_deref()
        .context("normalized FDC record has no last_updated_utc")?;
    let version = source_version(record.id.clone(), last_updated_utc)?;
    candidates.insert(version, record);
    Ok(())
}

fn source_version(id: String, last_updated_utc: &str) -> anyhow::Result<SourceVersion> {
    Ok(SourceVersion {
        id,
        last_updated_utc: DateTime::parse_from_rfc3339(last_updated_utc)
            .with_context(|| format!("invalid NMS last_updated_utc {last_updated_utc:?}"))?
            .with_timezone(&Utc),
    })
}

fn read_nav_rendezvous_keys(nav_db_dir: &Path) -> anyhow::Result<BTreeSet<String>> {
    let root_path = nav_db_dir.join("root");
    let root_bytes =
        fs::read(&root_path).with_context(|| format!("failed to read {}", root_path.display()))?;
    let root = NavKvRoot::parse(&root_bytes).map_err(anyhow::Error::msg)?;
    let mut read_error = None;
    let keys = root.prefix_keys(ProcedureRendezvousKey::NAV_KV_PREFIX, |page_index| {
        if read_error.is_some() {
            return None;
        }
        match read_nav_page(nav_db_dir, page_index) {
            Ok(bytes) => Some(bytes),
            Err(error) => {
                read_error = Some(error);
                None
            }
        }
    });
    if let Some(error) = read_error {
        return Err(error);
    }
    keys.context("failed to scan procedure rendezvous keys from nav_db")
        .map(|keys| keys.into_iter().collect())
}

fn read_nav_page(nav_db_dir: &Path, page_index: u32) -> anyhow::Result<Vec<u8>> {
    let path = nav_db_dir.join(format!("page_{page_index:04}"));
    let encoded = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    nav_kv_package::decode_xz_if_needed(&encoded)
        .map(|bytes| bytes.into_owned())
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("failed to decode {}", path.display()))
}

fn audit_records(
    records: Vec<StructuredNotamRecord>,
    nav_keys: &BTreeSet<String>,
    keyword_filter: Option<&str>,
) -> anyhow::Result<(
    BTreeMap<String, RendezvousAuditSummary>,
    Vec<RendezvousAuditFinding>,
    usize,
)> {
    let keyword_filter = keyword_filter.map(str::to_ascii_uppercase);
    if let Some(keyword) = keyword_filter.as_deref() {
        if !PROCEDURE_KEYWORDS.contains(&keyword) {
            bail!("unsupported procedure keyword {keyword}; expected IAP, ODP, SID, or STAR");
        }
    }

    let mut summaries = BTreeMap::<String, RendezvousAuditSummary>::new();
    let mut findings = Vec::new();
    let mut procedure_records = 0;
    for record in records {
        let Some(keyword) = record.notam_keyword.as_deref() else {
            continue;
        };
        if !PROCEDURE_KEYWORDS.contains(&keyword)
            || keyword_filter
                .as_deref()
                .is_some_and(|filter| filter != keyword)
        {
            continue;
        }
        procedure_records += 1;
        let summary = summaries.entry(keyword.to_string()).or_default();
        summary.records += 1;

        let generated_nav_keys = record
            .procedure_rendezvous_keys
            .iter()
            .map(|key| key.nav_kv_key().map_err(anyhow::Error::msg))
            .collect::<anyhow::Result<Vec<_>>>()?;
        summary.generated_keys += generated_nav_keys.len();
        let matched_keys = generated_nav_keys
            .iter()
            .filter(|key| nav_keys.contains(*key))
            .count();
        summary.matched_keys += matched_keys;

        let outcome = if generated_nav_keys.is_empty() {
            summary.unkeyed_records += 1;
            Some("unkeyed")
        } else {
            summary.keyed_records += 1;
            if matched_keys == 0 {
                summary.keyed_without_nav_match += 1;
                Some("no-nav-match")
            } else {
                summary.matched_records += 1;
                None
            }
        };
        if let Some(outcome) = outcome {
            let scope = match keyword {
                "STAR" => "SHARED",
                _ => record.airport_id.as_deref().unwrap_or_default(),
            };
            let kind = match keyword {
                "IAP" => "APPROACH",
                "STAR" => "ARRIVAL",
                "ODP" | "SID" => "DEPARTURE",
                _ => unreachable!("procedure keyword was validated"),
            };
            let sibling_prefix = format!(
                "{}{kind}/{}/",
                ProcedureRendezvousKey::NAV_KV_PREFIX,
                had_key::upper_component(scope),
            );
            let available_nav_keys = nav_keys
                .range(sibling_prefix.clone()..)
                .take_while(|key| key.starts_with(&sibling_prefix))
                .cloned()
                .collect::<Vec<_>>();
            findings.push(RendezvousAuditFinding {
                id: record.id,
                keyword: keyword.to_string(),
                airport_id: record.airport_id,
                last_updated_utc: record.last_updated_utc,
                outcome,
                generated_nav_keys,
                available_nav_key_count: available_nav_keys.len(),
                available_nav_keys: available_nav_keys.into_iter().take(50).collect(),
                text_preview: record.text.as_deref().map(text_preview),
            });
        }
    }
    findings.sort_by(|left, right| {
        (&left.keyword, &left.outcome, &left.airport_id, &left.id).cmp(&(
            &right.keyword,
            &right.outcome,
            &right.airport_id,
            &right.id,
        ))
    });
    Ok((summaries, findings, procedure_records))
}

fn text_preview(text: &str) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = normalized.chars();
    let preview = chars.by_ref().take(240).collect::<String>();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use product_contracts::ProcedureRendezvousKind;

    fn iap_record(id: &str, procedure_id: Option<&str>) -> StructuredNotamRecord {
        let mut record: StructuredNotamRecord = serde_json::from_value(serde_json::json!({
            "id": id,
            "nms_id": null,
            "source_type": "F",
            "notam_status": "ACTIVE",
            "notam_function": "NOTAMN",
            "notam_keyword": "IAP",
            "last_updated_utc": "2026-08-18T00:00:00Z",
            "location_designator": "SEA",
            "icao_id": "KSEA",
            "airport_id": "KSEA",
            "airport_effects": [],
            "procedure_rendezvous_keys": [],
            "airport_name": null,
            "airport_position": null,
            "location": null,
            "classification": null,
            "account_id": null,
            "xover_account_id": null,
            "xover_notam_id": null,
            "notam_number": null,
            "notam_year": null,
            "notam_type": null,
            "issued_utc": null,
            "effective_start_utc": null,
            "effective_end_utc": null,
            "text": "IAP TEST PROCEDURE",
            "local_text": null,
            "icao_text": null,
            "scenario": null
        }))
        .unwrap();
        if let Some(procedure_id) = procedure_id {
            record.procedure_rendezvous_keys.insert(
                ProcedureRendezvousKey::airport_scoped(
                    ProcedureRendezvousKind::Approach,
                    "KSEA",
                    procedure_id,
                )
                .unwrap(),
            );
        }
        record
    }

    #[test]
    fn audit_distinguishes_unkeyed_and_missing_nav_matches() {
        let matched = iap_record("matched", Some("I16R"));
        let matched_key = matched
            .procedure_rendezvous_keys
            .iter()
            .next()
            .unwrap()
            .nav_kv_key()
            .unwrap();
        let records = vec![
            matched,
            iap_record("missing", Some("I34L")),
            iap_record("unkeyed", None),
        ];

        let (summaries, findings, count) =
            audit_records(records, &BTreeSet::from([matched_key]), Some("iap")).unwrap();

        assert_eq!(count, 3);
        let summary = &summaries["IAP"];
        assert_eq!(summary.records, 3);
        assert_eq!(summary.keyed_records, 2);
        assert_eq!(summary.matched_records, 1);
        assert_eq!(summary.unkeyed_records, 1);
        assert_eq!(summary.keyed_without_nav_match, 1);
        assert_eq!(
            findings
                .iter()
                .map(|finding| (finding.id.as_str(), finding.outcome))
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([("missing", "no-nav-match"), ("unkeyed", "unkeyed")])
        );
    }
}
