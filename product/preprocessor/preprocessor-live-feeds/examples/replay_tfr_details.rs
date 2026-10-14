// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Rebuild captured FAA detail_records through the production publisher.
//! stdin: [{tfr_id, raw_xml}]. stdout: published TFR JSON. No network or live writes.
use anyhow::Context;
use preprocessor_live_feeds::{
    build_tfr_dataset, tfr_detail_backfill::parse_tfr_detail_xml, BuildTfrRequest,
};
use serde::Deserialize;
use std::{collections::BTreeMap, fs, io};

#[derive(Deserialize)]
struct Detail {
    tfr_id: String,
    raw_xml: String,
}

fn main() -> anyhow::Result<()> {
    let details: Vec<Detail> = serde_json::from_reader(io::stdin().lock())?;
    let root = tempfile::tempdir()?;
    let input_dir = root.path().join("input");
    fs::create_dir_all(input_dir.join("details"))?;
    let mut listing = Vec::new();
    for detail in details {
        parse_tfr_detail_xml(&detail.tfr_id, "capture", &detail.raw_xml)
            .with_context(|| format!("captured notice {}", detail.tfr_id))?;
        listing.push(serde_json::json!({"notam_id": detail.tfr_id}));
        fs::write(
            input_dir
                .join("details")
                .join(format!("{}.xml", detail.tfr_id.replace('/', "_"))),
            detail.raw_xml,
        )?;
    }
    fs::write(input_dir.join("list.json"), serde_json::to_vec(&listing)?)?;
    let result = build_tfr_dataset(&BuildTfrRequest {
        input_dir,
        output_dir: root.path().join("out"),
        version_label: "captured-audit".into(),
        generated_at_utc: chrono::DateTime::UNIX_EPOCH,
        notams_by_fdc_id: BTreeMap::new(),
    })?;
    io::copy(
        &mut fs::File::open(result.structured_json_path)?,
        &mut io::stdout().lock(),
    )?;
    Ok(())
}
