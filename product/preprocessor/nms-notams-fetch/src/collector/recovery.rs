// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use super::*;

struct ReplayUpdate {
    poll_id: i64,
    repaired: bool,
    parsed: ParsedRawUpdate,
}

fn parse_retained_update(
    raw: &str,
    classification: NmsNotamClassification,
) -> anyhow::Result<(NmsApiUpdate, DateTime<Utc>)> {
    let update = parse_nms_api_update(raw, classification)?;
    let last_updated = parse_timestamp(
        update
            .record
            .last_updated_utc
            .as_deref()
            .context("missing lastUpdated")?,
        "retained NOTAM update",
    )?;
    Ok((update, last_updated))
}

impl NmsApiCollectorStore {
    /// Explicit offline repair uses the same exclusion lock as the daemon.
    /// The daemon also runs this before announcing its initial source snapshot.
    pub fn repair_rejected_updates(&self) -> anyhow::Result<usize> {
        if !self.root.join("state.sqlite").is_file() {
            bail!("NOTAM repair requires an existing collector state");
        }
        let _lock = self.acquire_lock()?;
        if metadata(&self.open_connection()?, "schema_version")?.as_deref() != Some("9") {
            bail!("NOTAM repair requires collector schema 9");
        }
        self.recover_rejected_updates(Utc::now())
    }

    pub(super) fn recover_rejected_updates(&self, now: DateTime<Utc>) -> anyhow::Result<usize> {
        let connection = self.open_connection()?;
        // Still-invalid quarantine must not force expensive replay or make
        // ordinary startup depend on loading the historic baseline.
        let mut statement = connection.prepare(
            "SELECT classification, raw_aixm FROM rejected_updates r
             WHERE NOT EXISTS(SELECT 1 FROM raw_updates a WHERE a.payload_sha256=r.payload_sha256)",
        )?;
        let mut rows = statement.query([])?;
        let mut recoverable = false;
        while let Some(row) = rows.next()? {
            let classification = serde_json::from_value(serde_json::Value::String(row.get(0)?))?;
            if parse_retained_update(&row.get::<_, String>(1)?, classification).is_ok() {
                recoverable = true;
                break;
            }
        }
        if !recoverable {
            return Ok(0);
        }
        let path = metadata(&connection, "baseline_capture_path")?
            .context("NOTAM repair requires the retained Initial Load baseline")?;
        let baseline = load_initial_load_capture(Path::new(&path))?;
        self.recover_from_baseline(&baseline.records, baseline.captured_at_utc, now)
    }

    // Rebuild from the authoritative baseline and ALL retained subsequent
    // updates, including accepted cancellations/replacements. Applying only an
    // old quarantined upsert to today's state can resurrect a canceled NOTAM.
    pub(super) fn recover_from_baseline(
        &self,
        baseline: &[StructuredNotamRecord],
        baseline_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> anyhow::Result<usize> {
        let mut source = self.open_connection()?;
        let cursor_before = canonical_source_cursor_from_connection(&source)?;
        if source_epoch_for_records(baseline)? != cursor_before.epoch {
            bail!("retained NOTAM baseline does not match the installed source epoch");
        }
        let poll_cursor_before = metadata(&source, "poll_cursor_utc")?;
        let mut updates = Vec::new();
        let mut repaired = Vec::new();
        // An accepted copy of a formerly rejected payload is the durable repair
        // marker. Keep rejected_updates unchanged as historical evidence.
        for rejected in [true, false] {
            let sql = if rejected {
                "SELECT r.first_poll_id, r.classification, r.payload_sha256, r.raw_aixm
                 FROM rejected_updates r JOIN poll_runs p ON p.poll_id=r.first_poll_id
                 WHERE julianday(p.started_at_utc) >= julianday(?1)
                 AND NOT EXISTS(SELECT 1 FROM raw_updates a WHERE a.payload_sha256=r.payload_sha256)"
            } else {
                "SELECT r.poll_id, r.classification, r.payload_sha256, r.raw_aixm
                 FROM raw_updates r JOIN poll_runs p ON p.poll_id=r.poll_id
                 WHERE julianday(p.started_at_utc) >= julianday(?1)"
            };
            let mut statement = source.prepare(sql)?;
            let mut rows = statement.query([baseline_at.to_rfc3339()])?;
            while let Some(row) = rows.next()? {
                let raw: String = row.get(3)?;
                let hash: String = row.get(2)?;
                if sha256_hex(raw.as_bytes()) != hash {
                    bail!("retained NOTAM payload checksum mismatch: {hash}");
                }
                let classification: NmsNotamClassification =
                    serde_json::from_value(serde_json::Value::String(row.get(1)?))?;
                let parsed = parse_retained_update(&raw, classification);
                let (update, last_updated) = match parsed {
                    Ok(parsed) => parsed,
                    Err(_) if rejected => continue, // Still quarantined; do not invent missing identity.
                    Err(error) => {
                        return Err(error).context("cannot replay previously accepted NOTAM")
                    }
                };
                let poll_id = row.get(0)?;
                if rejected {
                    repaired.push((
                        hash.clone(),
                        poll_id,
                        classification,
                        update.clone(),
                        last_updated,
                        raw.clone(),
                    ));
                }
                updates.push(ReplayUpdate {
                    poll_id,
                    repaired: rejected,
                    parsed: ParsedRawUpdate {
                        classification,
                        payload_sha256: hash,
                        update,
                        last_updated,
                        raw_aixm: String::new(), // Do not retain the entire XML history in memory.
                    },
                });
            }
            if rejected && repaired.is_empty() {
                return Ok(0);
            }
        }
        updates.sort_by(|left, right| {
            left.poll_id
                .cmp(&right.poll_id)
                .then_with(|| left.parsed.last_updated.cmp(&right.parsed.last_updated))
                .then_with(|| {
                    left.parsed
                        .update
                        .record
                        .id
                        .cmp(&right.parsed.update.record.id)
                })
                .then_with(|| left.parsed.payload_sha256.cmp(&right.parsed.payload_sha256))
        });

        let mut replay = Connection::open_in_memory()?;
        let mut original = Connection::open_in_memory()?;
        Self::initialize_connection(&replay)?;
        Self::initialize_connection(&original)?;
        let tx = replay.transaction()?;
        let original_tx = original.transaction()?;
        for record in baseline {
            upsert_current(&tx, record, baseline_at)?;
            upsert_current(&original_tx, record, baseline_at)?;
        }
        let mut sequence = 0;
        let mut original_sequence = 0;
        let mut updates = updates.into_iter().peekable();
        {
            // Empty polls still expire records. Reproduce those boundaries too,
            // especially before resolving a later human-identity cancellation.
            let mut statement = source.prepare(
                "SELECT poll_id, started_at_utc, query_since_utc, completed_at_utc,
                        domestic_received, fdc_received FROM poll_runs
                 WHERE julianday(started_at_utc) >= julianday(?1) ORDER BY poll_id",
            )?;
            let mut rows = statement.query([baseline_at.to_rfc3339()])?;
            while let Some(row) = rows.next()? {
                let poll_id: i64 = row.get(0)?;
                // Production mutation helpers journal against a real poll;
                // preserve their foreign-key invariants in the scratch replay.
                for scratch in [&tx, &original_tx] {
                    scratch.execute(
                        "INSERT INTO poll_runs(poll_id, started_at_utc, query_since_utc,
                        completed_at_utc, domestic_received, fdc_received)
                     VALUES (?1,?2,?3,?4,?5,?6)",
                        params![
                            poll_id,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?
                        ],
                    )?;
                }
                while updates.peek().is_some_and(|item| item.poll_id == poll_id) {
                    let item = updates.next().expect("peeked update");
                    apply_update(
                        &tx,
                        poll_id,
                        &mut sequence,
                        &item.parsed.update,
                        item.parsed.last_updated,
                    )?;
                    if !item.repaired {
                        apply_update(
                            &original_tx,
                            poll_id,
                            &mut original_sequence,
                            &item.parsed.update,
                            item.parsed.last_updated,
                        )?;
                    }
                }
                for scratch in [&tx, &original_tx] {
                    prune_expired(
                        scratch,
                        parse_timestamp(&row.get::<_, String>(3)?, "retained poll completion")?,
                    )?;
                    scratch.execute("DELETE FROM canonical_changes", [])?;
                    scratch.execute("DELETE FROM poll_runs", [])?;
                }
            }
        }
        prune_expired(&tx, now)?;
        prune_expired(&original_tx, now)?;
        let original_records: BTreeMap<_, _> = current_records_from_connection(&original_tx)?
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect();
        let recovered: BTreeMap<_, _> = current_records_from_connection(&tx)?
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect();
        tx.commit()?;

        let tx = source.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if canonical_source_cursor_from_connection(&tx)? != cursor_before
            || metadata(&tx, "poll_cursor_utc")? != poll_cursor_before
        {
            bail!("NOTAM source advanced during recovery; no repair was applied");
        }
        let previous: BTreeMap<_, _> = current_records_from_connection(&tx)?
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect();
        // Prove that the retained accepted history explains today's state
        // before trusting a repaired replay. Missing rows must not silently
        // delete good records. Ignore expiry since the last completed poll.
        let mut active_previous = previous.clone();
        for (id, record) in &previous {
            if record
                .effective_end_utc
                .as_deref()
                .map(|end| parse_timestamp(end, "current NOTAM effective end"))
                .transpose()?
                .is_some_and(|end| end <= now)
            {
                active_previous.remove(id);
            }
        }
        if original_records != active_previous {
            bail!("retained NOTAM history does not reproduce current state; no repair was applied");
        }
        let poll_id: i64 =
            tx.query_row("SELECT MAX(poll_id) FROM poll_runs", [], |row| row.get(0))?;
        let mut sequence = canonical_change_sequence(&tx)?;
        for id in previous.keys().filter(|id| !recovered.contains_key(*id)) {
            tx.execute("DELETE FROM current_notams WHERE id=?1", [id])?;
            append_canonical_change(
                &tx,
                poll_id,
                &mut sequence,
                CanonicalNotamSourceMutation::Remove {
                    notam_id: id.clone(),
                },
            )?;
        }
        for (id, record) in &recovered {
            if previous.get(id) != Some(record) {
                if upsert_current(&tx, record, now)? == 0 {
                    bail!(
                        "replayed NOTAM {} would overwrite a newer current record",
                        record.id
                    );
                }
                append_canonical_change(
                    &tx,
                    poll_id,
                    &mut sequence,
                    CanonicalNotamSourceMutation::Upsert(Box::new(record.clone())),
                )?;
            }
        }
        for (hash, poll_id, classification, update, at, raw) in &repaired {
            tx.execute(
                "INSERT INTO raw_updates(payload_sha256,poll_id,classification,nms_id,last_updated_utc,action,referenced_human_identity,raw_aixm)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![hash, poll_id, classification.api_name(), update.record.nms_id,
                    at.to_rfc3339(), action_name(update.action),
                    update.referenced_notam.as_ref().map(|r| r.human_identity()), raw],
            )?;
        }
        set_metadata(&tx, SOURCE_SEQUENCE_METADATA_KEY, &sequence.to_string())?;
        tx.commit()?;
        Ok(repaired.len())
    }
}
