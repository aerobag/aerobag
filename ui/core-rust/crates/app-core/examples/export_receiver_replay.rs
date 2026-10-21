// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Export inbound application records, never setup blobs, into the shared UI replay.
//! Optional radar archive inputs are comparison publications, not phone reception.
use std::{collections::BTreeMap, error::Error, fs, path::Path};

use app_core::{
    receiver::{
        capture_replay::{replay_export, ReplayEvent},
        ApplicationRecord, Product,
    },
    receiver_replay::{Document, Event, Input, FORMAT},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::DateTime;
use serde_json::Value;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() == 2 && args[0] == "--validate" {
        app_core::receiver_replay::validate(&fs::read_to_string(&args[1])?)?;
        println!("Replay validated through core.");
        return Ok(());
    }
    if args.len() != 2 && args.len() != 4 {
        return Err("Usage: export_receiver_replay CAPTURE.zip OUTPUT.json [ARCHIVE_INDEX.sqlite EXTRACTED_ARCHIVE_ROOT]".into());
    }
    let mut events = Vec::new();
    let mut decode_error = None;
    let summary = replay_export(fs::File::open(&args[0])?, |clock, event| {
        let input = match event {
            ReplayEvent::Connected => Input::Connection { connected: true },
            ReplayEvent::Disconnected => Input::Connection { connected: false },
            ReplayEvent::Message(ApplicationRecord::Message {
                message_id,
                attributes: 0,
                data,
            }) => match app_core::receiver::decode_product(message_id, &data) {
                Ok(Product::Uninterpreted { .. }) => return,
                Ok(_) => Input::Receiver {
                    message_id,
                    bytes_base64: STANDARD.encode(data),
                },
                Err(error) => {
                    decode_error = Some(error.to_string());
                    return;
                }
            },
            _ => return,
        };
        events.push(Event {
            elapsed_ms: clock.wall_epoch_ms,
            input,
        });
    })?;
    if let Some(error) = decode_error {
        return Err(error.into());
    }
    events.sort_by_key(|e| e.elapsed_ms);
    let start = events
        .first()
        .ok_or("Capture has no input events")?
        .elapsed_ms;
    let end = events.last().unwrap().elapsed_ms;
    if args.len() == 4 {
        add_radar_archive(&mut events, start, end, &args[2], Path::new(&args[3]))?;
        events.sort_by_key(|e| e.elapsed_ms);
    }
    for event in &mut events {
        event.elapsed_ms -= start;
    }
    let document = Document {
        format: FORMAT.into(),
        schema_version: 1,
        title: if args.len() == 4 {
            "Receiver flight + archived Internet radar comparison"
        } else {
            "Receiver flight"
        }
        .into(),
        start_epoch_ms: start,
        duration_ms: end - start,
        events,
    };
    let json = serde_json::to_string(&document)?;
    app_core::receiver_replay::validate(&json)?;
    fs::write(&args[1], &json)?;
    println!(
        "{} events; {:.1} minutes; {:.2} MiB; {} captured connection(s). Validated through core.",
        document.events.len(),
        document.duration_ms as f64 / 60000.0,
        json.len() as f64 / 1048576.0,
        summary.connections
    );
    Ok(())
}

fn add_radar_archive(
    events: &mut Vec<Event>,
    start: i64,
    end: i64,
    index: &str,
    root: &Path,
) -> Result<(), Box<dyn Error>> {
    let db =
        rusqlite::Connection::open_with_flags(index, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut query = db.prepare("SELECT event FROM events ORDER BY sequence")?;
    for row in query.query_map([], |row| row.get::<_, String>(0))? {
        let event: Value = serde_json::from_str(&row?)?;
        if event["product"] != "nexrad" {
            continue;
        }
        let time = DateTime::parse_from_rfc3339(
            event["effective_at_utc"]
                .as_str()
                .ok_or("Missing publication time")?,
        )?
        .timestamp_millis();
        if time < start || time > end {
            continue;
        }
        let directory = event["directory"]
            .as_str()
            .ok_or("Missing event directory")?;
        let version = event["head"]["version"].as_str().ok_or("Missing version")?;
        if !version
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            || !Path::new(directory)
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
        {
            return Err("Unsafe archive path".into());
        }
        let state = root.join(directory).join("states/nexrad").join(version);
        if !state.exists() {
            continue;
        } // A deliberately selected archive subset.
        let manifest: Value = serde_json::from_slice(&fs::read(state.join("manifest.json"))?)?;
        let mut tiles = BTreeMap::new();
        collect_pngs(&state, &state.join("tiles"), version, &mut tiles)?;
        events.push(Event {
            elapsed_ms: time,
            input: Input::InternetRadar {
                version: version.into(),
                manifest,
                tiles_base64: tiles,
            },
        });
    }
    Ok(())
}

fn collect_pngs(
    base: &Path,
    dir: &Path,
    version: &str,
    result: &mut BTreeMap<String, String>,
) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            collect_pngs(base, &entry.path(), version, result)?;
        } else if entry.file_type()?.is_file()
            && entry.path().extension().is_some_and(|ext| ext == "png")
        {
            result.insert(
                format!(
                    "{}/states/nexrad/{version}/{}",
                    app_core::live_feeds::LIVE_FEEDS_BASE_PATH,
                    entry
                        .path()
                        .strip_prefix(base)?
                        .to_str()
                        .ok_or("Non-UTF8 archive path")?
                ),
                STANDARD.encode(fs::read(entry.path())?),
            );
        } else {
            return Err("Unexpected archive tile entry".into());
        }
    }
    Ok(())
}
