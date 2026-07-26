// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use preprocessor_live_feeds::engine::sha256_hex;
use preprocessor_live_feeds::nms_initial_load::NmsNotamClassification;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

const FIXTURE_SCHEMA_VERSION: u32 = 2;
const FIXTURE_USAGE_NOTICE: &str = "TEST DATA ONLY - NOT FOR NAVIGATION";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NmsFixtureManifest {
    pub schema_version: u32,
    pub usage_notice: String,
    pub captured_by_commit: String,
    pub source_environment: String,
    pub initial_load_captured_at_utc: String,
    pub initial_load: Vec<NmsFixtureInitialLoad>,
    pub poll_trace: NmsFixturePollTrace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NmsFixtureInitialLoad {
    pub classification: NmsNotamClassification,
    pub feature_collection_timestamp: Option<String>,
    pub declared_record_count: Option<usize>,
    pub parsed_message_count: usize,
    pub canonical_record_count: usize,
    pub file: NmsFixtureFile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NmsFixturePollTrace {
    pub file: NmsFixtureFile,
    pub poll_count: usize,
    pub update_count: usize,
    pub first_started_at_utc: String,
    pub last_completed_at_utc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NmsFixtureFile {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NmsFixturePoll {
    pub started_at_utc: String,
    pub query_since_utc: String,
    pub completed_at_utc: String,
    pub source_domestic_received: usize,
    pub source_fdc_received: usize,
    pub updates: Vec<NmsFixtureUpdate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NmsFixtureUpdate {
    pub classification: NmsNotamClassification,
    pub payload_sha256: String,
    pub raw_aixm: String,
}

#[derive(Debug)]
pub struct LoadedNmsFixture {
    pub root: PathBuf,
    pub manifest: NmsFixtureManifest,
    pub polls: Vec<NmsFixturePoll>,
}

#[derive(Deserialize)]
struct InitialLoadSourceManifest {
    schema_version: u32,
    captured_at_utc: String,
    source: InitialLoadSource,
    classifications: Vec<InitialLoadSourceClassification>,
}

#[derive(Deserialize)]
struct InitialLoadSource {
    environment: String,
}

#[derive(Deserialize)]
struct InitialLoadSourceClassification {
    classification: NmsNotamClassification,
    feature_collection_timestamp: Option<String>,
    declared_record_count: Option<usize>,
    parsed_message_count: usize,
    canonical_record_count: usize,
    gzip: InitialLoadSourceFile,
}

#[derive(Deserialize)]
struct InitialLoadSourceFile {
    path: String,
    bytes: u64,
    sha256: String,
}

pub fn capture_nms_fixture(
    initial_load_dir: &Path,
    collector_state_root: &Path,
    output_dir: &Path,
    captured_by_commit: &str,
) -> anyhow::Result<PathBuf> {
    validate_commit(captured_by_commit)?;
    if output_dir.exists() {
        bail!("fixture output already exists: {}", output_dir.display());
    }
    let parent = output_dir
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create fixture parent {}", parent.display()))?;
    let output_name = output_dir
        .file_name()
        .context("fixture output must have a final path component")?
        .to_string_lossy();
    let temp_dir = parent.join(format!(
        ".{output_name}.tmp-{}-{}",
        std::process::id(),
        Utc::now().timestamp_millis()
    ));
    fs::create_dir(&temp_dir)
        .with_context(|| format!("failed to create temporary fixture {}", temp_dir.display()))?;

    let result = build_fixture(
        initial_load_dir,
        collector_state_root,
        &temp_dir,
        captured_by_commit,
    )
    .and_then(|manifest| {
        let manifest_path = temp_dir.join("capture_manifest.json");
        let mut file = BufWriter::new(
            File::create(&manifest_path)
                .with_context(|| format!("failed to create {}", manifest_path.display()))?,
        );
        serde_json::to_writer_pretty(&mut file, &manifest)
            .with_context(|| format!("failed to write {}", manifest_path.display()))?;
        file.write_all(b"\n")
            .with_context(|| format!("failed to finish {}", manifest_path.display()))?;
        file.flush()
            .with_context(|| format!("failed to flush {}", manifest_path.display()))?;
        fs::rename(&temp_dir, output_dir)
            .with_context(|| format!("failed to publish NMS fixture {}", output_dir.display()))?;
        Ok(output_dir.join("capture_manifest.json"))
    });
    result.with_context(|| {
        format!(
            "incomplete NMS fixture retained for diagnosis at {}",
            temp_dir.display()
        )
    })
}

fn build_fixture(
    initial_load_dir: &Path,
    collector_state_root: &Path,
    temp_dir: &Path,
    captured_by_commit: &str,
) -> anyhow::Result<NmsFixtureManifest> {
    let source_manifest_path = initial_load_dir.join("manifest.json");
    let source_manifest: InitialLoadSourceManifest = serde_json::from_slice(
        &fs::read(&source_manifest_path)
            .with_context(|| format!("failed to read {}", source_manifest_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", source_manifest_path.display()))?;
    if source_manifest.schema_version != 1 {
        bail!(
            "unsupported NMS Initial Load capture schema {}; expected 1",
            source_manifest.schema_version
        );
    }
    parse_timestamp(
        &source_manifest.captured_at_utc,
        "Initial Load capture timestamp",
    )?;
    if source_manifest.classifications.len() != 2 {
        bail!("NMS fixture requires exactly DOMESTIC and FDC Initial Load files");
    }

    let initial_output_dir = temp_dir.join("initial");
    fs::create_dir(&initial_output_dir)
        .with_context(|| format!("failed to create {}", initial_output_dir.display()))?;
    let mut initial_load = Vec::new();
    for classification in source_manifest.classifications {
        let source_path = initial_load_dir.join(&classification.gzip.path);
        verify_file(
            &source_path,
            classification.gzip.bytes,
            &classification.gzip.sha256,
        )?;
        let file_name = format!(
            "{}.xml.gz",
            classification
                .classification
                .api_name()
                .to_ascii_lowercase()
        );
        let relative_path = format!("initial/{file_name}");
        let destination = temp_dir.join(&relative_path);
        fs::copy(&source_path, &destination).with_context(|| {
            format!(
                "failed to copy {} to {}",
                source_path.display(),
                destination.display()
            )
        })?;
        initial_load.push(NmsFixtureInitialLoad {
            classification: classification.classification,
            feature_collection_timestamp: classification.feature_collection_timestamp,
            declared_record_count: classification.declared_record_count,
            parsed_message_count: classification.parsed_message_count,
            canonical_record_count: classification.canonical_record_count,
            file: describe_file(&destination, relative_path)?,
        });
    }
    initial_load.sort_by_key(|entry| entry.classification.api_name());
    let classifications = initial_load
        .iter()
        .map(|entry| entry.classification)
        .collect::<Vec<_>>();
    if classifications
        != vec![
            NmsNotamClassification::Domestic,
            NmsNotamClassification::Fdc,
        ]
    {
        bail!("NMS fixture requires one DOMESTIC and one FDC Initial Load file");
    }

    let state_path = collector_state_root.join("state.sqlite");
    let connection = Connection::open_with_flags(
        &state_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("failed to open {}", state_path.display()))?;
    let trace_relative_path = "polls.jsonl.gz".to_string();
    let trace_path = temp_dir.join(&trace_relative_path);
    let trace_file = File::create(&trace_path)
        .with_context(|| format!("failed to create {}", trace_path.display()))?;
    let mut writer = GzEncoder::new(BufWriter::new(trace_file), Compression::best());
    let mut poll_statement = connection.prepare(
        "SELECT poll_id, started_at_utc, query_since_utc, completed_at_utc,
                domestic_received, fdc_received
         FROM poll_runs ORDER BY poll_id",
    )?;
    let polls = poll_statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
        ))
    })?;
    let mut update_statement = connection.prepare(
        "SELECT classification, payload_sha256, raw_aixm
         FROM raw_updates
         WHERE poll_id = ?1
         ORDER BY last_updated_utc, nms_id, payload_sha256",
    )?;
    let mut poll_count = 0usize;
    let mut update_count = 0usize;
    let mut first_started_at_utc = None;
    let mut last_completed_at_utc = None;
    for poll in polls {
        let (
            poll_id,
            started_at_utc,
            query_since_utc,
            completed_at_utc,
            domestic_received,
            fdc_received,
        ) = poll.context("failed to read NMS poll trace row")?;
        parse_timestamp(&started_at_utc, "NMS poll start")?;
        parse_timestamp(&query_since_utc, "NMS poll query cursor")?;
        parse_timestamp(&completed_at_utc, "NMS poll completion")?;
        let updates = update_statement
            .query_map([poll_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .map(|row| {
                let (classification, payload_sha256, raw_aixm) =
                    row.context("failed to read NMS raw update")?;
                if sha256_hex(raw_aixm.as_bytes()) != payload_sha256 {
                    bail!("NMS raw update {payload_sha256} has an invalid payload hash");
                }
                Ok(NmsFixtureUpdate {
                    classification: parse_classification(&classification)?,
                    payload_sha256,
                    raw_aixm,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        update_count += updates.len();
        let fixture_poll = NmsFixturePoll {
            started_at_utc: started_at_utc.clone(),
            query_since_utc,
            completed_at_utc: completed_at_utc.clone(),
            source_domestic_received: usize::try_from(domestic_received)
                .context("negative DOMESTIC source receive count")?,
            source_fdc_received: usize::try_from(fdc_received)
                .context("negative FDC source receive count")?,
            updates,
        };
        serde_json::to_writer(&mut writer, &fixture_poll)
            .context("failed to write NMS fixture poll")?;
        writer
            .write_all(b"\n")
            .context("failed to delimit NMS fixture poll")?;
        first_started_at_utc.get_or_insert(started_at_utc);
        last_completed_at_utc = Some(completed_at_utc);
        poll_count += 1;
    }
    writer.finish().context("failed to finish NMS poll trace")?;
    if poll_count == 0 {
        bail!("NMS collector state contains no polls");
    }
    if update_count == 0 {
        bail!("NMS collector state contains no unique updates");
    }

    Ok(NmsFixtureManifest {
        schema_version: FIXTURE_SCHEMA_VERSION,
        usage_notice: FIXTURE_USAGE_NOTICE.to_string(),
        captured_by_commit: captured_by_commit.to_string(),
        source_environment: source_manifest.source.environment,
        initial_load_captured_at_utc: source_manifest.captured_at_utc,
        initial_load,
        poll_trace: NmsFixturePollTrace {
            file: describe_file(&trace_path, trace_relative_path)?,
            poll_count,
            update_count,
            first_started_at_utc: first_started_at_utc.context("NMS fixture has no first poll")?,
            last_completed_at_utc: last_completed_at_utc.context("NMS fixture has no last poll")?,
        },
    })
}

pub fn load_nms_fixture(root: &Path) -> anyhow::Result<LoadedNmsFixture> {
    let manifest_path = root.join("capture_manifest.json");
    let manifest: NmsFixtureManifest = serde_json::from_slice(
        &fs::read(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    if manifest.schema_version != FIXTURE_SCHEMA_VERSION {
        bail!(
            "unsupported NMS fixture schema {}; expected {FIXTURE_SCHEMA_VERSION}",
            manifest.schema_version
        );
    }
    if manifest.usage_notice != FIXTURE_USAGE_NOTICE {
        bail!("NMS fixture usage notice must be exactly {FIXTURE_USAGE_NOTICE:?}");
    }
    validate_commit(&manifest.captured_by_commit)?;
    parse_timestamp(
        &manifest.initial_load_captured_at_utc,
        "Initial Load capture timestamp",
    )?;
    for initial in &manifest.initial_load {
        verify_fixture_file(root, &initial.file)?;
    }
    verify_fixture_file(root, &manifest.poll_trace.file)?;
    let trace_path = root.join(&manifest.poll_trace.file.path);
    let reader = BufReader::new(GzDecoder::new(
        File::open(&trace_path)
            .with_context(|| format!("failed to open {}", trace_path.display()))?,
    ));
    let mut polls = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| {
            format!("failed to read {} line {}", trace_path.display(), index + 1)
        })?;
        if line.is_empty() {
            bail!(
                "{} contains an empty line at {}",
                trace_path.display(),
                index + 1
            );
        }
        let poll = serde_json::from_str::<NmsFixturePoll>(&line).with_context(|| {
            format!(
                "failed to parse {} line {}",
                trace_path.display(),
                index + 1
            )
        })?;
        parse_timestamp(&poll.started_at_utc, "NMS fixture poll start")?;
        parse_timestamp(&poll.query_since_utc, "NMS fixture poll query cursor")?;
        parse_timestamp(&poll.completed_at_utc, "NMS fixture poll completion")?;
        for update in &poll.updates {
            if sha256_hex(update.raw_aixm.as_bytes()) != update.payload_sha256 {
                bail!(
                    "{} line {} contains an invalid update payload hash",
                    trace_path.display(),
                    index + 1
                );
            }
        }
        polls.push(poll);
    }
    let update_count = polls.iter().map(|poll| poll.updates.len()).sum::<usize>();
    if polls.len() != manifest.poll_trace.poll_count
        || update_count != manifest.poll_trace.update_count
    {
        bail!(
            "NMS fixture trace contains {} polls and {} updates; manifest declares {} and {}",
            polls.len(),
            update_count,
            manifest.poll_trace.poll_count,
            manifest.poll_trace.update_count
        );
    }
    Ok(LoadedNmsFixture {
        root: root.to_path_buf(),
        manifest,
        polls,
    })
}

fn describe_file(path: &Path, relative_path: String) -> anyhow::Result<NmsFixtureFile> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(NmsFixtureFile {
        path: relative_path,
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
    })
}

fn verify_fixture_file(root: &Path, expected: &NmsFixtureFile) -> anyhow::Result<()> {
    if Path::new(&expected.path).is_absolute()
        || Path::new(&expected.path)
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        bail!("NMS fixture contains an unsafe path {}", expected.path);
    }
    verify_file(&root.join(&expected.path), expected.bytes, &expected.sha256)
}

fn verify_file(path: &Path, expected_bytes: u64, expected_sha256: &str) -> anyhow::Result<()> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    if bytes.len() as u64 != expected_bytes || sha256_hex(&bytes) != expected_sha256 {
        bail!("NMS fixture identity mismatch for {}", path.display());
    }
    Ok(())
}

fn parse_timestamp(value: &str, label: &str) -> anyhow::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("{label} is not RFC3339: {value}"))
        .map(|value| value.with_timezone(&Utc))
}

fn parse_classification(value: &str) -> anyhow::Result<NmsNotamClassification> {
    match value {
        "DOMESTIC" => Ok(NmsNotamClassification::Domestic),
        "FDC" => Ok(NmsNotamClassification::Fdc),
        _ => bail!("unsupported NMS fixture classification {value}"),
    }
}

fn validate_commit(value: &str) -> anyhow::Result<()> {
    if value.len() < 7 || value.len() > 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("captured-by commit must be a 7-64 character hexadecimal Git object ID");
    }
    Ok(())
}
