// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Fetch and publish PIREPs into a new local directory, without a daemon or deployment.
use std::{fs, path::PathBuf};

use anyhow::{ensure, Context};
use chrono::Utc;
use preprocessor_live_feeds::{
    engine::{
        FileLiveFeedPublisher, LiveFeedPublisher, LiveFeedStatePayload, ProductBuilder,
        SystemClock, UpstreamEvent,
    },
    products::{LiveFeedFetchConfig, PirepLiveFeedBuilder},
};
use serde_json::{json, Value};

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("usage: pirep_source_probe NEW_DIRECTORY")?,
    );
    // Refuse existing directories so this diagnostic cannot overwrite a daemon's state.
    fs::create_dir(&root).context("probe requires a new output directory")?;
    let root = root.canonicalize()?;
    let scratch = root.join("scratch");
    let builder = PirepLiveFeedBuilder::new(LiveFeedFetchConfig::new(1, None));
    let now = Utc::now();
    let built = builder.build_state(
        &UpstreamEvent {
            product: "pireps".into(),
            source_id: "source-probe".into(),
            previous_source_id: None,
            observed_at_utc: now,
            payload_path: None,
        },
        &scratch,
    )?;
    let LiveFeedStatePayload::JsonFile { value, .. } = &built.payload else {
        anyhow::bail!("unexpected PIREP payload");
    };
    let records = value["pireps_by_id"]
        .as_object()
        .context("missing PIREP records")?;
    let mut times = records
        .values()
        .filter_map(|record| record["observed_at_utc"].as_str())
        .collect::<Vec<_>>();
    times.sort_unstable();
    let summary = json!({"record_count": records.len(), "oldest_observation": times.first(),
        "newest_observation": times.last()});
    let publisher = FileLiveFeedPublisher::new(root.join("published"), SystemClock);
    let update = publisher.publish(built.clone())?;
    ensure!(
        publisher.publish(built)?.unchanged,
        "identical replay was not unchanged"
    );
    let selection: Value = serde_json::from_slice(&fs::read(
        scratch.join("meta/provenance/pireps/source-selection.json"),
    )?)?;
    let result = json!({"collected_at": now, "source": selection, "records": summary,
        "version": update.version, "state_sha256": update.state_sha256,
        "state_path": update.state_path, "identical_replay": "unchanged"});
    fs::write(
        root.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
