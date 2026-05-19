use super::*;

pub fn update_live_feeds(config: &ProductBuildConfig) -> anyhow::Result<LiveFeedUpdateResult> {
    fs::create_dir_all(&config.build_root)
        .with_context(|| format!("failed to create {}", config.build_root.display()))?;
    let live_root = live_feeds_root(config);
    fs::create_dir_all(&live_root)
        .with_context(|| format!("failed to create {}", live_root.display()))?;

    let tasks = vec![
        GraphScheduledTask {
            id: "live-feed-metars-build".to_string(),
            deps: vec![],
            weight: 1,
            kind: LiveFeedTaskKind::BuildMetars,
        },
        GraphScheduledTask {
            id: "live-feed-metars-publish".to_string(),
            deps: vec!["live-feed-metars-build".to_string()],
            weight: 1,
            kind: LiveFeedTaskKind::PublishMetars,
        },
        GraphScheduledTask {
            id: "live-feed-nexrad-build".to_string(),
            deps: vec![],
            weight: 3,
            kind: LiveFeedTaskKind::BuildNexrad,
        },
        GraphScheduledTask {
            id: "live-feed-nexrad-publish".to_string(),
            deps: vec!["live-feed-nexrad-build".to_string()],
            weight: 1,
            kind: LiveFeedTaskKind::PublishNexrad,
        },
        GraphScheduledTask {
            id: "live-feed-tfrs-build".to_string(),
            deps: vec![],
            weight: 1,
            kind: LiveFeedTaskKind::BuildTfrs,
        },
        GraphScheduledTask {
            id: "live-feed-tfrs-publish".to_string(),
            deps: vec!["live-feed-tfrs-build".to_string()],
            weight: 1,
            kind: LiveFeedTaskKind::PublishTfrs,
        },
        GraphScheduledTask {
            id: "live-feed-winds-aloft-build".to_string(),
            deps: vec![],
            weight: 1,
            kind: LiveFeedTaskKind::BuildWindsAloft,
        },
        GraphScheduledTask {
            id: "live-feed-winds-aloft-publish".to_string(),
            deps: vec!["live-feed-winds-aloft-build".to_string()],
            weight: 1,
            kind: LiveFeedTaskKind::PublishWindsAloft,
        },
        GraphScheduledTask {
            id: "live-feed-obstacles-build".to_string(),
            deps: vec![],
            weight: 1,
            kind: LiveFeedTaskKind::BuildObstacles,
        },
        GraphScheduledTask {
            id: "live-feed-obstacles-publish".to_string(),
            deps: vec!["live-feed-obstacles-build".to_string()],
            weight: 1,
            kind: LiveFeedTaskKind::PublishObstacles,
        },
    ];
    let config_for_tasks = config.clone();
    let live_root_for_tasks = live_root.clone();
    let (task_values, _) = run_weighted_task_graph(
        "live-feeds-scheduler",
        tasks,
        config.max_heavy_jobs.max(1),
        |message| {
            eprintln!("{message}");
            Ok(())
        },
        move |kind, task_values, _task_node_records| {
            let config = config_for_tasks.clone();
            let live_root = live_root_for_tasks.clone();
            match kind {
                LiveFeedTaskKind::BuildMetars => match build_live_metars_state(&config) {
                    Ok(built) => Ok(GraphTaskCompletion {
                        node_records: vec![],
                        value: LiveFeedTaskValue::BuiltMetars(built),
                        completion_detail: "built metars".to_string(),
                    }),
                    Err(error) => live_feed_failure_completion("metars", "build", error),
                },
                LiveFeedTaskKind::PublishMetars => {
                    let built = match task_values.get("live-feed-metars-build") {
                        Some(LiveFeedTaskValue::BuiltMetars(built)) => built.clone(),
                        Some(LiveFeedTaskValue::Failed(failure)) => {
                            return Ok(GraphTaskCompletion {
                                node_records: vec![],
                                value: LiveFeedTaskValue::Failed(failure.clone()),
                                completion_detail: format!(
                                    "skipped because {} {} failed",
                                    failure.product, failure.phase
                                ),
                            });
                        }
                        _ => bail!("missing built METAR live-feed state"),
                    };
                    match publish_live_feed_product(&config, &live_root, || {
                        publish_live_metars(&live_root, built)
                    }) {
                        Ok((published, current_path)) => Ok(GraphTaskCompletion {
                            node_records: vec![],
                            value: LiveFeedTaskValue::Published(published),
                            completion_detail: format!("current={}", current_path.display()),
                        }),
                        Err(error) => live_feed_failure_completion("metars", "publish", error),
                    }
                }
                LiveFeedTaskKind::BuildNexrad => match build_live_nexrad_state(&config) {
                    Ok(built) => Ok(GraphTaskCompletion {
                        node_records: vec![],
                        value: LiveFeedTaskValue::BuiltNexrad(built),
                        completion_detail: "built nexrad".to_string(),
                    }),
                    Err(error) => live_feed_failure_completion("nexrad", "build", error),
                },
                LiveFeedTaskKind::PublishNexrad => {
                    let built = match task_values.get("live-feed-nexrad-build") {
                        Some(LiveFeedTaskValue::BuiltNexrad(built)) => built.clone(),
                        Some(LiveFeedTaskValue::Failed(failure)) => {
                            return Ok(GraphTaskCompletion {
                                node_records: vec![],
                                value: LiveFeedTaskValue::Failed(failure.clone()),
                                completion_detail: format!(
                                    "skipped because {} {} failed",
                                    failure.product, failure.phase
                                ),
                            });
                        }
                        _ => bail!("missing built NEXRAD live-feed state"),
                    };
                    match publish_live_feed_product(&config, &live_root, || {
                        publish_live_nexrad(&live_root, built)
                    }) {
                        Ok((published, current_path)) => Ok(GraphTaskCompletion {
                            node_records: vec![],
                            value: LiveFeedTaskValue::Published(published),
                            completion_detail: format!("current={}", current_path.display()),
                        }),
                        Err(error) => live_feed_failure_completion("nexrad", "publish", error),
                    }
                }
                LiveFeedTaskKind::BuildTfrs => match build_live_tfrs_state(&config) {
                    Ok(built) => Ok(GraphTaskCompletion {
                        node_records: vec![],
                        value: LiveFeedTaskValue::BuiltTfrs(built),
                        completion_detail: "built tfrs".to_string(),
                    }),
                    Err(error) => live_feed_failure_completion("tfrs", "build", error),
                },
                LiveFeedTaskKind::PublishTfrs => {
                    let built = match task_values.get("live-feed-tfrs-build") {
                        Some(LiveFeedTaskValue::BuiltTfrs(built)) => built.clone(),
                        Some(LiveFeedTaskValue::Failed(failure)) => {
                            return Ok(GraphTaskCompletion {
                                node_records: vec![],
                                value: LiveFeedTaskValue::Failed(failure.clone()),
                                completion_detail: format!(
                                    "skipped because {} {} failed",
                                    failure.product, failure.phase
                                ),
                            });
                        }
                        _ => bail!("missing built TFR live-feed state"),
                    };
                    match publish_live_feed_product(&config, &live_root, || {
                        publish_live_tfrs(&live_root, built)
                    }) {
                        Ok((published, current_path)) => Ok(GraphTaskCompletion {
                            node_records: vec![],
                            value: LiveFeedTaskValue::Published(published),
                            completion_detail: format!("current={}", current_path.display()),
                        }),
                        Err(error) => live_feed_failure_completion("tfrs", "publish", error),
                    }
                }
                LiveFeedTaskKind::BuildWindsAloft => match build_live_winds_aloft_state(&config) {
                    Ok(built) => Ok(GraphTaskCompletion {
                        node_records: vec![],
                        value: LiveFeedTaskValue::BuiltWindsAloft(built),
                        completion_detail: "built winds-aloft".to_string(),
                    }),
                    Err(error) => live_feed_failure_completion("winds-aloft", "build", error),
                },
                LiveFeedTaskKind::PublishWindsAloft => {
                    let built = match task_values.get("live-feed-winds-aloft-build") {
                        Some(LiveFeedTaskValue::BuiltWindsAloft(built)) => built.clone(),
                        Some(LiveFeedTaskValue::Failed(failure)) => {
                            return Ok(GraphTaskCompletion {
                                node_records: vec![],
                                value: LiveFeedTaskValue::Failed(failure.clone()),
                                completion_detail: format!(
                                    "skipped because {} {} failed",
                                    failure.product, failure.phase
                                ),
                            });
                        }
                        _ => bail!("missing built winds-aloft live-feed state"),
                    };
                    match publish_live_feed_product(&config, &live_root, || {
                        publish_live_winds_aloft(&live_root, built)
                    }) {
                        Ok((published, current_path)) => Ok(GraphTaskCompletion {
                            node_records: vec![],
                            value: LiveFeedTaskValue::Published(published),
                            completion_detail: format!("current={}", current_path.display()),
                        }),
                        Err(error) => live_feed_failure_completion("winds-aloft", "publish", error),
                    }
                }
                LiveFeedTaskKind::BuildObstacles => match build_live_obstacles_state(&config) {
                    Ok(built) => Ok(GraphTaskCompletion {
                        node_records: vec![],
                        value: LiveFeedTaskValue::BuiltObstacles(built),
                        completion_detail: "built obstacles".to_string(),
                    }),
                    Err(error) => live_feed_failure_completion("obstacles", "build", error),
                },
                LiveFeedTaskKind::PublishObstacles => {
                    let built = match task_values.get("live-feed-obstacles-build") {
                        Some(LiveFeedTaskValue::BuiltObstacles(built)) => built.clone(),
                        Some(LiveFeedTaskValue::Failed(failure)) => {
                            return Ok(GraphTaskCompletion {
                                node_records: vec![],
                                value: LiveFeedTaskValue::Failed(failure.clone()),
                                completion_detail: format!(
                                    "skipped because {} {} failed",
                                    failure.product, failure.phase
                                ),
                            });
                        }
                        _ => bail!("missing built obstacles live-feed state"),
                    };
                    match publish_live_feed_product(&config, &live_root, || {
                        publish_live_obstacles(&live_root, built)
                    }) {
                        Ok((published, current_path)) => Ok(GraphTaskCompletion {
                            node_records: vec![],
                            value: LiveFeedTaskValue::Published(published),
                            completion_detail: format!("current={}", current_path.display()),
                        }),
                        Err(error) => live_feed_failure_completion("obstacles", "publish", error),
                    }
                }
            }
        },
    )?;
    let mut products = Vec::new();
    let mut failures = Vec::new();
    for task_id in [
        "live-feed-metars-publish",
        "live-feed-nexrad-publish",
        "live-feed-tfrs-publish",
        "live-feed-winds-aloft-publish",
        "live-feed-obstacles-publish",
    ] {
        match task_values.get(task_id) {
            Some(LiveFeedTaskValue::Published(result)) => products.push(result.clone()),
            Some(LiveFeedTaskValue::Failed(failure)) => failures.push(failure.clone()),
            _ => failures.push(FailedLiveFeedResult {
                product: task_id.to_string(),
                phase: "scheduler".to_string(),
                error: "missing live-feed task result".to_string(),
            }),
        }
    }
    let current_path = live_feeds_current_path(&live_root);
    write_live_feeds_current(&live_root)?;
    Ok(LiveFeedUpdateResult {
        root: live_root,
        current_path,
        products,
        failures,
    })
}

pub(super) fn live_feeds_root(config: &ProductBuildConfig) -> PathBuf {
    artifact_root_from_build_root(&config.build_root).join("live-feeds")
}

pub(super) fn live_feeds_current_path(root: &Path) -> PathBuf {
    root.join("current.json")
}

pub(super) fn live_feed_failure_completion(
    product: &str,
    phase: &str,
    error: anyhow::Error,
) -> anyhow::Result<GraphTaskCompletion<LiveFeedTaskValue>> {
    let failure = FailedLiveFeedResult {
        product: product.to_string(),
        phase: phase.to_string(),
        error: format!("{error:#}"),
    };
    eprintln!(
        "live-feed {} {} failed: {}",
        failure.product, failure.phase, failure.error
    );
    Ok(GraphTaskCompletion {
        node_records: vec![],
        completion_detail: format!("{} {} failed", failure.product, failure.phase),
        value: LiveFeedTaskValue::Failed(failure),
    })
}

pub(super) fn publish_live_feed_product(
    config: &ProductBuildConfig,
    live_root: &Path,
    publish: impl FnOnce() -> anyhow::Result<UpdatedLiveFeedResult>,
) -> anyhow::Result<(UpdatedLiveFeedResult, PathBuf)> {
    let _publication_lock =
        acquire_named_publication_lock(&config.build_root, "live-feeds", |message| {
            eprintln!("{message}");
        })?;
    let published = publish()?;
    let current_path = write_live_feeds_current(live_root)?;
    Ok((published, current_path))
}

pub(super) fn build_live_metars_state(
    config: &ProductBuildConfig,
) -> anyhow::Result<BuiltLiveMetarState> {
    let (_source_zip_path, _source_generated_at_utc, record) = build_metars_product(config)?;
    let structured_json_relative = record
        .outputs
        .get("structured_json")
        .context("METAR node record missing structured_json output")?;
    let state_source_path =
        artifact_root_from_build_root(&config.build_root).join(structured_json_relative);
    if !state_source_path.is_file() {
        bail!(
            "METAR structured state does not exist: {}",
            state_source_path.display()
        );
    }

    let state_value = read_json_value(&state_source_path)?;
    let version = state_value
        .get("version_label")
        .and_then(serde_json::Value::as_str)
        .context("METAR state missing version_label")?
        .to_string();
    Ok(BuiltLiveMetarState {
        version,
        state_source_path,
        state_value,
    })
}

pub(super) fn publish_live_metars(
    live_root: &Path,
    built: BuiltLiveMetarState,
) -> anyhow::Result<UpdatedLiveFeedResult> {
    let BuiltLiveMetarState {
        version,
        state_source_path,
        state_value,
    } = built;
    let state_dir = live_root.join("states").join("metars");
    let delta_dir = live_root.join("deltas").join("metars");
    let version_dir = live_root.join("versions").join("metars");
    fs::create_dir_all(&state_dir)
        .with_context(|| format!("failed to create {}", state_dir.display()))?;
    fs::create_dir_all(&delta_dir)
        .with_context(|| format!("failed to create {}", delta_dir.display()))?;
    fs::create_dir_all(&version_dir)
        .with_context(|| format!("failed to create {}", version_dir.display()))?;

    let state_path = state_dir.join(format!("{version}.json"));
    if !state_path.is_file() {
        fs::copy(&state_source_path, &state_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                state_source_path.display(),
                state_path.display()
            )
        })?;
    }
    let state_bytes = fs::read(&state_path)
        .with_context(|| format!("failed to read {}", state_path.display()))?;
    let state_blob_sha256 = sha256_hex(&state_bytes);
    let state_sha256 = canonical_json_sha256(&state_value)?;

    let previous_entry = read_live_feeds_current(live_root)?
        .and_then(|current| current.products.get("metars").cloned());
    let mut previous_version = None;
    let mut delta_ref = None;
    let mut delta_path = None;
    let mut changed_count = 0;
    let mut removed_count = 0;
    if let Some(previous) = previous_entry.as_ref() {
        if previous.current == version {
            if previous.state_sha256 != state_sha256 {
                bail!(
                    "current METAR state hash mismatch for {}: expected {}, got {}",
                    previous.current,
                    previous.state_sha256,
                    state_sha256
                );
            }
            return Ok(UpdatedLiveFeedResult {
                product: "metars".to_string(),
                version,
                state_path,
                delta_path: None,
                changed_count,
                removed_count,
            });
        }
        if previous.current != version {
            let previous_state_path = live_root.join(&previous.state_url);
            let previous_state = read_json_value(&previous_state_path)?;
            let previous_sha256 = canonical_json_sha256(&previous_state)?;
            if previous_sha256 != previous.state_sha256 {
                bail!(
                    "previous METAR state hash mismatch for {}: expected {}, got {}",
                    previous.current,
                    previous.state_sha256,
                    previous_sha256
                );
            }
            let delta = build_metar_station_delta(&previous_state, &state_value)?;
            changed_count = delta.changed.len();
            removed_count = delta.removed.len();
            let path = delta_dir.join(format!("{}__{}.json", previous.current, version));
            write_json_pretty_file(&path, &delta)?;
            let bytes =
                fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
            delta_ref = Some(LiveDeltaRef {
                from_version: previous.current.clone(),
                from_state_sha256: previous.state_sha256.clone(),
                to_version: version.clone(),
                to_state_sha256: state_sha256.clone(),
                url: live_feeds_relative_url(live_root, &path)?,
                bytes: bytes.len() as u64,
                blob_sha256: sha256_hex(&bytes),
            });
            previous_version = Some(previous.current.clone());
            delta_path = Some(path);
        }
    }

    let state_ref = LivePayloadRef {
        url: live_feeds_relative_url(live_root, &state_path)?,
        bytes: state_bytes.len() as u64,
        blob_sha256: state_blob_sha256,
        state_sha256: state_sha256.clone(),
    };
    let version_manifest = LiveFeedVersionManifest {
        schema_version: 1,
        product: "metars".to_string(),
        version: version.clone(),
        previous: previous_version,
        state: state_ref,
        delta_from_previous: delta_ref,
    };
    let version_manifest_path = version_dir.join(format!("{version}.json"));
    write_json_pretty_file(&version_manifest_path, &version_manifest)?;
    let current = merge_live_feed_current(
        live_root,
        "metars",
        LiveFeedCurrentEntry {
            current: version.clone(),
            version_manifest_url: live_feeds_relative_url(live_root, &version_manifest_path)?,
            state_url: live_feeds_relative_url(live_root, &state_path)?,
            state_sha256,
        },
    )?;
    write_live_feeds_current_manifest(live_root, &current)?;

    Ok(UpdatedLiveFeedResult {
        product: "metars".to_string(),
        version,
        state_path,
        delta_path,
        changed_count,
        removed_count,
    })
}

pub(super) fn build_live_tfrs_state(
    config: &ProductBuildConfig,
) -> anyhow::Result<BuiltLiveTfrState> {
    let (_source_zip_path, _source_generated_at_utc, record) = build_tfrs_product(config)?;
    let structured_json_relative = record
        .outputs
        .get("structured_json")
        .context("TFR node record missing structured_json output")?;
    let state_source_path =
        artifact_root_from_build_root(&config.build_root).join(structured_json_relative);
    if !state_source_path.is_file() {
        bail!(
            "TFR structured state does not exist: {}",
            state_source_path.display()
        );
    }
    let state_value = read_json_value(&state_source_path)?;
    let version = state_value
        .get("version_label")
        .and_then(serde_json::Value::as_str)
        .context("TFR state missing version_label")?
        .to_string();
    let area_group_count = state_value
        .get("areas")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    Ok(BuiltLiveTfrState {
        version,
        state_source_path,
        state_value,
        area_group_count,
    })
}

pub(super) fn publish_live_tfrs(
    live_root: &Path,
    built: BuiltLiveTfrState,
) -> anyhow::Result<UpdatedLiveFeedResult> {
    let BuiltLiveTfrState {
        version,
        state_source_path,
        state_value,
        area_group_count,
    } = built;
    let state_dir = live_root.join("states").join("tfrs");
    let version_dir = live_root.join("versions").join("tfrs");
    fs::create_dir_all(&state_dir)
        .with_context(|| format!("failed to create {}", state_dir.display()))?;
    fs::create_dir_all(&version_dir)
        .with_context(|| format!("failed to create {}", version_dir.display()))?;

    let state_path = state_dir.join(format!("{version}.json"));
    if !state_path.is_file() {
        fs::copy(&state_source_path, &state_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                state_source_path.display(),
                state_path.display()
            )
        })?;
    }
    let state_bytes = fs::read(&state_path)
        .with_context(|| format!("failed to read {}", state_path.display()))?;
    let state_blob_sha256 = sha256_hex(&state_bytes);
    let state_sha256 = canonical_json_sha256(&state_value)?;

    let previous_entry = read_live_feeds_current(live_root)?
        .and_then(|current| current.products.get("tfrs").cloned());
    if let Some(previous) = previous_entry.as_ref() {
        if previous.current == version {
            if previous.state_sha256 != state_sha256 {
                bail!(
                    "current TFR state hash mismatch for {}: expected {}, got {}",
                    previous.current,
                    previous.state_sha256,
                    state_sha256
                );
            }
            return Ok(UpdatedLiveFeedResult {
                product: "tfrs".to_string(),
                version,
                state_path,
                delta_path: None,
                changed_count: 0,
                removed_count: 0,
            });
        }
    }

    let state_ref = LivePayloadRef {
        url: live_feeds_relative_url(live_root, &state_path)?,
        bytes: state_bytes.len() as u64,
        blob_sha256: state_blob_sha256,
        state_sha256: state_sha256.clone(),
    };
    let version_manifest = LiveFeedVersionManifest {
        schema_version: 1,
        product: "tfrs".to_string(),
        version: version.clone(),
        previous: previous_entry.map(|entry| entry.current),
        state: state_ref,
        delta_from_previous: None,
    };
    let version_manifest_path = version_dir.join(format!("{version}.json"));
    write_json_pretty_file(&version_manifest_path, &version_manifest)?;
    let current = merge_live_feed_current(
        live_root,
        "tfrs",
        LiveFeedCurrentEntry {
            current: version.clone(),
            version_manifest_url: live_feeds_relative_url(live_root, &version_manifest_path)?,
            state_url: live_feeds_relative_url(live_root, &state_path)?,
            state_sha256,
        },
    )?;
    write_live_feeds_current_manifest(live_root, &current)?;

    Ok(UpdatedLiveFeedResult {
        product: "tfrs".to_string(),
        version,
        state_path,
        delta_path: None,
        changed_count: area_group_count,
        removed_count: 0,
    })
}

pub(super) fn build_live_winds_aloft_state(
    config: &ProductBuildConfig,
) -> anyhow::Result<BuiltLiveWindsAloftState> {
    let (_source_zip_path, _source_generated_at_utc, record) = build_winds_aloft_product(config)?;
    let structured_json_relative = record
        .outputs
        .get("structured_json")
        .context("winds-aloft node record missing structured_json output")?;
    let state_source_path =
        artifact_root_from_build_root(&config.build_root).join(structured_json_relative);
    if !state_source_path.is_file() {
        bail!(
            "winds-aloft structured state does not exist: {}",
            state_source_path.display()
        );
    }
    let state_value = read_json_value(&state_source_path)?;
    let version = fast_product_version_label(&record.fingerprint);
    let file_count = state_value
        .get("files")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    Ok(BuiltLiveWindsAloftState {
        version,
        state_source_path,
        state_value,
        file_count,
    })
}

pub(super) fn publish_live_winds_aloft(
    live_root: &Path,
    built: BuiltLiveWindsAloftState,
) -> anyhow::Result<UpdatedLiveFeedResult> {
    let BuiltLiveWindsAloftState {
        version,
        state_source_path,
        state_value,
        file_count,
    } = built;
    let state_dir = live_root.join("states").join("winds-aloft");
    let version_dir = live_root.join("versions").join("winds-aloft");
    fs::create_dir_all(&state_dir)
        .with_context(|| format!("failed to create {}", state_dir.display()))?;
    fs::create_dir_all(&version_dir)
        .with_context(|| format!("failed to create {}", version_dir.display()))?;

    let state_path = state_dir.join(format!("{version}.json"));
    if !state_path.is_file() {
        fs::copy(&state_source_path, &state_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                state_source_path.display(),
                state_path.display()
            )
        })?;
    }
    let state_bytes = fs::read(&state_path)
        .with_context(|| format!("failed to read {}", state_path.display()))?;
    let state_blob_sha256 = sha256_hex(&state_bytes);
    let state_sha256 = canonical_json_sha256(&state_value)?;

    let previous_entry = read_live_feeds_current(live_root)?
        .and_then(|current| current.products.get("winds-aloft").cloned());
    if let Some(previous) = previous_entry.as_ref() {
        if previous.current == version {
            if previous.state_sha256 != state_sha256 {
                bail!(
                    "current winds-aloft state hash mismatch for {}: expected {}, got {}",
                    previous.current,
                    previous.state_sha256,
                    state_sha256
                );
            }
            return Ok(UpdatedLiveFeedResult {
                product: "winds-aloft".to_string(),
                version,
                state_path,
                delta_path: None,
                changed_count: 0,
                removed_count: 0,
            });
        }
    }

    let state_ref = LivePayloadRef {
        url: live_feeds_relative_url(live_root, &state_path)?,
        bytes: state_bytes.len() as u64,
        blob_sha256: state_blob_sha256,
        state_sha256: state_sha256.clone(),
    };
    let version_manifest = LiveFeedVersionManifest {
        schema_version: 1,
        product: "winds-aloft".to_string(),
        version: version.clone(),
        previous: previous_entry.map(|entry| entry.current),
        state: state_ref,
        delta_from_previous: None,
    };
    let version_manifest_path = version_dir.join(format!("{version}.json"));
    write_json_pretty_file(&version_manifest_path, &version_manifest)?;
    let current = merge_live_feed_current(
        live_root,
        "winds-aloft",
        LiveFeedCurrentEntry {
            current: version.clone(),
            version_manifest_url: live_feeds_relative_url(live_root, &version_manifest_path)?,
            state_url: live_feeds_relative_url(live_root, &state_path)?,
            state_sha256,
        },
    )?;
    write_live_feeds_current_manifest(live_root, &current)?;

    Ok(UpdatedLiveFeedResult {
        product: "winds-aloft".to_string(),
        version,
        state_path,
        delta_path: None,
        changed_count: file_count,
        removed_count: 0,
    })
}

pub(super) fn build_live_obstacles_state(
    config: &ProductBuildConfig,
) -> anyhow::Result<BuiltLiveObstacleState> {
    let (_source_zip_path, _source_generated_at_utc, record) = build_obstacles_product(config)?;
    let structured_json_relative = record
        .outputs
        .get("structured_json")
        .context("obstacle node record missing structured_json output")?;
    let state_source_path =
        artifact_root_from_build_root(&config.build_root).join(structured_json_relative);
    if !state_source_path.is_file() {
        bail!(
            "obstacle structured state does not exist: {}",
            state_source_path.display()
        );
    }
    let state_value = read_json_value(&state_source_path)?;
    let version = state_value
        .get("version_label")
        .and_then(serde_json::Value::as_str)
        .context("obstacle state missing version_label")?
        .to_string();
    let obstacle_count = state_value
        .get("obstacles_by_id")
        .and_then(serde_json::Value::as_object)
        .map_or(0, serde_json::Map::len);
    Ok(BuiltLiveObstacleState {
        version,
        state_source_path,
        state_value,
        obstacle_count,
    })
}

pub(super) fn publish_live_obstacles(
    live_root: &Path,
    built: BuiltLiveObstacleState,
) -> anyhow::Result<UpdatedLiveFeedResult> {
    let BuiltLiveObstacleState {
        version,
        state_source_path,
        state_value,
        obstacle_count,
    } = built;
    let state_dir = live_root.join("states").join("obstacles");
    let delta_dir = live_root.join("deltas").join("obstacles");
    let version_dir = live_root.join("versions").join("obstacles");
    fs::create_dir_all(&state_dir)
        .with_context(|| format!("failed to create {}", state_dir.display()))?;
    fs::create_dir_all(&delta_dir)
        .with_context(|| format!("failed to create {}", delta_dir.display()))?;
    fs::create_dir_all(&version_dir)
        .with_context(|| format!("failed to create {}", version_dir.display()))?;

    let state_path = state_dir.join(format!("{version}.json"));
    if !state_path.is_file() {
        fs::copy(&state_source_path, &state_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                state_source_path.display(),
                state_path.display()
            )
        })?;
    }
    let state_bytes = fs::read(&state_path)
        .with_context(|| format!("failed to read {}", state_path.display()))?;
    let state_blob_sha256 = sha256_hex(&state_bytes);
    let state_sha256 = canonical_json_sha256(&state_value)?;

    let previous_entry = read_live_feeds_current(live_root)?
        .and_then(|current| current.products.get("obstacles").cloned());
    let mut previous_version = None;
    let mut delta_ref = None;
    let mut delta_path = None;
    let mut changed_count = obstacle_count;
    let mut removed_count = 0;
    if let Some(previous) = previous_entry.as_ref() {
        if previous.current == version {
            if previous.state_sha256 != state_sha256 {
                bail!(
                    "current obstacle state hash mismatch for {}: expected {}, got {}",
                    previous.current,
                    previous.state_sha256,
                    state_sha256
                );
            }
            return Ok(UpdatedLiveFeedResult {
                product: "obstacles".to_string(),
                version,
                state_path,
                delta_path: None,
                changed_count: 0,
                removed_count: 0,
            });
        }
        let previous_state_path = live_root.join(&previous.state_url);
        let previous_state = read_json_value(&previous_state_path)?;
        let previous_sha256 = canonical_json_sha256(&previous_state)?;
        if previous_sha256 != previous.state_sha256 {
            bail!(
                "previous obstacle state hash mismatch for {}: expected {}, got {}",
                previous.current,
                previous.state_sha256,
                previous_sha256
            );
        }
        let delta = build_live_feed_record_delta(
            "obstacles",
            "obstacles_by_id",
            &previous_state,
            &state_value,
        )?;
        changed_count = delta.changed.len();
        removed_count = delta.removed.len();
        let path = delta_dir.join(format!("{}__{}.json", previous.current, version));
        write_json_pretty_file(&path, &delta)?;
        let bytes =
            fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
        delta_ref = Some(LiveDeltaRef {
            from_version: previous.current.clone(),
            from_state_sha256: previous.state_sha256.clone(),
            to_version: version.clone(),
            to_state_sha256: state_sha256.clone(),
            url: live_feeds_relative_url(live_root, &path)?,
            bytes: bytes.len() as u64,
            blob_sha256: sha256_hex(&bytes),
        });
        previous_version = Some(previous.current.clone());
        delta_path = Some(path);
    }

    let state_ref = LivePayloadRef {
        url: live_feeds_relative_url(live_root, &state_path)?,
        bytes: state_bytes.len() as u64,
        blob_sha256: state_blob_sha256,
        state_sha256: state_sha256.clone(),
    };
    let version_manifest = LiveFeedVersionManifest {
        schema_version: 1,
        product: "obstacles".to_string(),
        version: version.clone(),
        previous: previous_version,
        state: state_ref,
        delta_from_previous: delta_ref,
    };
    let version_manifest_path = version_dir.join(format!("{version}.json"));
    write_json_pretty_file(&version_manifest_path, &version_manifest)?;
    let current = merge_live_feed_current(
        live_root,
        "obstacles",
        LiveFeedCurrentEntry {
            current: version.clone(),
            version_manifest_url: live_feeds_relative_url(live_root, &version_manifest_path)?,
            state_url: live_feeds_relative_url(live_root, &state_path)?,
            state_sha256,
        },
    )?;
    write_live_feeds_current_manifest(live_root, &current)?;

    Ok(UpdatedLiveFeedResult {
        product: "obstacles".to_string(),
        version,
        state_path,
        delta_path,
        changed_count,
        removed_count,
    })
}

pub(super) fn build_live_feed_record_delta(
    product: &str,
    records_key: &str,
    from_state: &serde_json::Value,
    to_state: &serde_json::Value,
) -> anyhow::Result<LiveFeedRecordDelta> {
    let from_version = state_version_label(from_state)?;
    let to_version = state_version_label(to_state)?;
    let from_records = state_record_map(from_state, records_key)?;
    let to_records = state_record_map(to_state, records_key)?;

    let mut changed = BTreeMap::new();
    for (record_id, to_record) in to_records {
        if from_records.get(record_id) != Some(to_record) {
            changed.insert(record_id.clone(), to_record.clone());
        }
    }
    let mut removed = from_records
        .keys()
        .filter(|record_id| !to_records.contains_key(*record_id))
        .cloned()
        .collect::<Vec<_>>();
    removed.sort();

    Ok(LiveFeedRecordDelta {
        schema_version: 1,
        product: product.to_string(),
        from_version: from_version.to_string(),
        to_version: to_version.to_string(),
        changed,
        removed,
    })
}

pub(super) fn apply_live_feed_record_delta(
    records_key: &str,
    count_key: &str,
    from_state: &serde_json::Value,
    delta: &LiveFeedRecordDelta,
) -> anyhow::Result<serde_json::Value> {
    let from_version = state_version_label(from_state)?;
    if from_version != delta.from_version {
        bail!(
            "delta starts at {}, but local state is {}",
            delta.from_version,
            from_version
        );
    }
    let mut result = from_state.clone();
    let record_count = {
        let records = result
            .get_mut(records_key)
            .and_then(serde_json::Value::as_object_mut)
            .with_context(|| format!("state missing {records_key} object"))?;
        for record_id in &delta.removed {
            records.remove(record_id);
        }
        for (record_id, record) in &delta.changed {
            records.insert(record_id.clone(), record.clone());
        }
        records.len()
    };
    let version = result
        .get_mut("version_label")
        .context("state missing version_label")?;
    *version = serde_json::Value::String(delta.to_version.clone());
    if let Some(count) = result.get_mut(count_key) {
        *count = serde_json::json!(record_count);
    }
    Ok(result)
}

pub(super) fn state_version_label(state: &serde_json::Value) -> anyhow::Result<&str> {
    state
        .get("version_label")
        .and_then(serde_json::Value::as_str)
        .context("state missing version_label")
}

pub(super) fn state_record_map<'a>(
    state: &'a serde_json::Value,
    records_key: &str,
) -> anyhow::Result<&'a serde_json::Map<String, serde_json::Value>> {
    state
        .get(records_key)
        .and_then(serde_json::Value::as_object)
        .with_context(|| format!("state missing {records_key} object"))
}

pub(super) fn build_live_nexrad_state(
    config: &ProductBuildConfig,
) -> anyhow::Result<BuiltLiveNexradState> {
    let debug_lat_lon_grid = env_flag("NEXRAD_DEBUG_LATLON_GRID");
    let artifact_root = artifact_root_from_build_root(&config.build_root).to_path_buf();
    let generated_at_utc = Utc::now()
        .with_second(0)
        .expect("zero seconds should be valid")
        .with_nanosecond(0)
        .expect("zero nanos should be valid");
    let scratch_root = artifact_root
        .join("private-work")
        .join("live-nexrad")
        .join(generated_at_utc.format("%Y%m%dT%H%MZ").to_string());
    let input_dir = scratch_root.join("input");
    if scratch_root.exists() {
        fs::remove_dir_all(&scratch_root)
            .with_context(|| format!("failed to clear {}", scratch_root.display()))?;
    }
    fs::create_dir_all(&input_dir)
        .with_context(|| format!("failed to create {}", input_dir.display()))?;

    let source_override = env_path("AEROBAG_LIVE_NEXRAD_SOURCE_GZ");
    let (source_gz_path, source_file_name) = if let Some(source_path) = source_override {
        let source_file_name = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .context("AEROBAG_LIVE_NEXRAD_SOURCE_GZ must name a source .tif.gz")?
            .to_string();
        (source_path, source_file_name)
    } else {
        let fetch_cache = FetchCacheConfig {
            root: config.fetch_cache_root.clone(),
            mode: FetchCacheMode::parse(&config.fetch_cache_mode)?,
        };
        let provenance_dir = scratch_root.join("meta").join("provenance").join("nexrad");
        fs::create_dir_all(&provenance_dir)
            .with_context(|| format!("failed to create {}", provenance_dir.display()))?;

        let index_url = "https://mrms.ncep.noaa.gov/data/RIDGEII/L2/CONUS/CREF_QCD/";
        let index_request = PrefetchRequest::new(index_url)
            .with_logical_file_name("index.html")
            .allow_html();
        prefetch_requests_with_provenance(
            std::slice::from_ref(&index_request),
            &input_dir,
            config.fetch_jobs,
            Some(&fetch_cache),
            &provenance_dir,
            "nexrad-index",
        )?;

        let listings = parse_nexrad_index_for_product(&input_dir.join("index.html"))?;
        let source_file_name = listings
            .first()
            .cloned()
            .context("NEXRAD index did not contain any source frames")?;
        let source_request = PrefetchRequest::new(format!("{index_url}{source_file_name}"))
            .with_logical_file_name(&source_file_name);
        prefetch_archives_with_provenance(
            std::slice::from_ref(&source_request),
            &input_dir,
            config.fetch_jobs,
            Some(&fetch_cache),
            &provenance_dir,
            "nexrad-frame",
        )?;
        (input_dir.join(&source_file_name), source_file_name)
    };
    let observed_at_utc = parse_nexrad_observed_at_utc(&source_file_name)?;
    let source_sha256 = hash_file(&source_gz_path)?;
    let palette_hash = hash_text(NEXRAD_FIXED_OPAQUE_PALETTE_JSON);
    let source_grid_script_hash = hash_text(&format!(
        "{}\n{}",
        NEXRAD_SOURCE_GRID_TILE_SCRIPT, NEXRAD_FIXED_OPAQUE_PALETTE_JSON
    ));
    let debug_version_suffix = if debug_lat_lon_grid {
        format!("_debuggrid{}", &source_grid_script_hash[..8])
    } else {
        String::new()
    };
    let version = format!(
        "{}_{}_png8{}{}",
        observed_at_utc.format("%Y%m%dT%H%M%SZ"),
        &source_sha256[..16],
        &palette_hash[..8],
        debug_version_suffix
    );
    let inputs = BTreeMap::from([
        ("product_id".to_string(), "nexrad-source-grid".to_string()),
        ("source_file".to_string(), source_file_name.clone()),
        ("source_sha256".to_string(), source_sha256.clone()),
        ("state_id".to_string(), version.clone()),
        ("res_levels".to_string(), "0,1,2,3".to_string()),
        ("tile_size".to_string(), "512".to_string()),
        (
            "debug_lat_lon_grid".to_string(),
            debug_lat_lon_grid.to_string(),
        ),
        ("source_grid_script".to_string(), source_grid_script_hash),
        ("fixed_palette_sha256".to_string(), palette_hash),
    ]);
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "live-nexrad-source-grid")?,
        "live-nexrad-source-grid",
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let manifest_path = output_dir.join("manifest.json");
    if try_load_node_record(&prepared, std::slice::from_ref(&manifest_path))?.is_some() {
        let manifest_value = read_json_value(&manifest_path)?;
        let tile_count = live_nexrad_tile_count(&manifest_value)?;
        return Ok(BuiltLiveNexradState {
            version,
            state_source_dir: output_dir,
            manifest_source_path: manifest_path,
            manifest_value,
            tile_count,
        });
    }
    let _build_lock = match claim_or_wait_for_node(&prepared, std::slice::from_ref(&manifest_path))?
    {
        NodeCacheState::CacheHit(_record) => {
            let manifest_value = read_json_value(&manifest_path)?;
            let tile_count = live_nexrad_tile_count(&manifest_value)?;
            return Ok(BuiltLiveNexradState {
                version,
                state_source_dir: output_dir,
                manifest_source_path: manifest_path,
                manifest_value,
                tile_count,
            });
        }
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    if output_dir.exists() {
        fs::remove_dir_all(&output_dir)
            .with_context(|| format!("failed to clear {}", output_dir.display()))?;
    }
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    build_nexrad_source_grid_tiles(
        &source_gz_path,
        &output_dir,
        &version,
        &observed_at_utc.and_utc().to_rfc3339(),
        &source_file_name,
        &source_sha256,
        debug_lat_lon_grid,
    )?;
    let manifest_value = read_json_value(&manifest_path)?;
    let tile_count = live_nexrad_tile_count(&manifest_value)?;
    let outputs = BTreeMap::from([(
        "manifest".to_string(),
        relative_artifact_path(&manifest_path, &config.build_root),
    )]);
    write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )?;
    Ok(BuiltLiveNexradState {
        version,
        state_source_dir: output_dir,
        manifest_source_path: manifest_path,
        manifest_value,
        tile_count,
    })
}

pub(super) fn publish_live_nexrad(
    live_root: &Path,
    built: BuiltLiveNexradState,
) -> anyhow::Result<UpdatedLiveFeedResult> {
    let BuiltLiveNexradState {
        version,
        state_source_dir,
        manifest_source_path,
        manifest_value,
        tile_count,
    } = built;
    let state_dir = live_root.join("states").join("nexrad").join(&version);
    let version_dir = live_root.join("versions").join("nexrad");
    fs::create_dir_all(&version_dir)
        .with_context(|| format!("failed to create {}", version_dir.display()))?;

    let state_sha256 = canonical_json_sha256(&manifest_value)?;
    let manifest_bytes = fs::read(&manifest_source_path)
        .with_context(|| format!("failed to read {}", manifest_source_path.display()))?;
    let state_blob_sha256 = sha256_hex(&manifest_bytes);
    let current_manifest_path = state_dir.join("manifest.json");
    let needs_publish = if current_manifest_path.is_file() {
        canonical_json_sha256(&read_json_value(&current_manifest_path)?)? != state_sha256
    } else {
        true
    };
    if needs_publish {
        if state_dir.exists() {
            fs::remove_dir_all(&state_dir)
                .with_context(|| format!("failed to remove {}", state_dir.display()))?;
        }
        hardlink_dir_recursive(&state_source_dir, &state_dir)?;
    }

    let state_ref = LivePayloadRef {
        url: live_feeds_relative_url(live_root, &current_manifest_path)?,
        bytes: manifest_bytes.len() as u64,
        blob_sha256: state_blob_sha256,
        state_sha256: state_sha256.clone(),
    };
    let version_manifest = LiveFeedVersionManifest {
        schema_version: 1,
        product: "nexrad".to_string(),
        version: version.clone(),
        previous: None,
        state: state_ref,
        delta_from_previous: None,
    };
    let version_manifest_path = version_dir.join(format!("{version}.json"));
    write_json_pretty_file(&version_manifest_path, &version_manifest)?;
    let current = merge_live_feed_current(
        live_root,
        "nexrad",
        LiveFeedCurrentEntry {
            current: version.clone(),
            version_manifest_url: live_feeds_relative_url(live_root, &version_manifest_path)?,
            state_url: live_feeds_relative_url(live_root, &current_manifest_path)?,
            state_sha256,
        },
    )?;
    write_live_feeds_current_manifest(live_root, &current)?;

    Ok(UpdatedLiveFeedResult {
        product: "nexrad".to_string(),
        version,
        state_path: current_manifest_path,
        delta_path: None,
        changed_count: tile_count,
        removed_count: 0,
    })
}

pub(super) fn parse_nexrad_observed_at_utc(file_name: &str) -> anyhow::Result<NaiveDateTime> {
    let suffix = file_name
        .strip_prefix("CONUS_L2_CREF_QCD_")
        .with_context(|| format!("unexpected NEXRAD source filename: {file_name}"))?;
    let (date, time_with_ext) = suffix
        .split_once('_')
        .with_context(|| format!("unexpected NEXRAD source filename: {file_name}"))?;
    let time = time_with_ext
        .strip_suffix(".tif.gz")
        .with_context(|| format!("unexpected NEXRAD source filename: {file_name}"))?;
    NaiveDateTime::parse_from_str(&(date.to_string() + time), "%Y%m%d%H%M%S")
        .with_context(|| format!("failed to parse NEXRAD observed time from {file_name}"))
}

pub(super) fn live_nexrad_tile_count(manifest: &serde_json::Value) -> anyhow::Result<usize> {
    let levels = manifest
        .get("levels")
        .and_then(serde_json::Value::as_array)
        .context("NEXRAD source-grid manifest missing levels")?;
    Ok(levels
        .iter()
        .map(|level| {
            let cols = level
                .get("tile_cols")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let rows = level
                .get("tile_rows")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            (cols * rows) as usize
        })
        .sum())
}

pub(super) fn build_nexrad_source_grid_tiles(
    source_gz_path: &Path,
    output_dir: &Path,
    version: &str,
    observed_at_utc: &str,
    source_file: &str,
    source_sha256: &str,
    debug_lat_lon_grid: bool,
) -> anyhow::Result<()> {
    let script_path = output_dir.join("build_nexrad_source_grid_tiles.py");
    let palette_path = output_dir.join("nexrad_fixed_palette.json");
    fs::write(&script_path, NEXRAD_SOURCE_GRID_TILE_SCRIPT)
        .with_context(|| format!("failed to write {}", script_path.display()))?;
    fs::write(&palette_path, NEXRAD_FIXED_OPAQUE_PALETTE_JSON)
        .with_context(|| format!("failed to write {}", palette_path.display()))?;
    let output = Command::new("python3")
        .arg(&script_path)
        .arg("--palette")
        .arg(&palette_path)
        .arg("--source-gz")
        .arg(source_gz_path)
        .arg("--output-dir")
        .arg(output_dir)
        .arg("--state-id")
        .arg(version)
        .arg("--observed-at-utc")
        .arg(observed_at_utc)
        .arg("--source-file")
        .arg(source_file)
        .arg("--source-sha256")
        .arg(source_sha256)
        .arg("--tile-size")
        .arg("512")
        .arg("--res-level")
        .arg("0")
        .arg("--res-level")
        .arg("1")
        .arg("--res-level")
        .arg("2")
        .arg("--res-level")
        .arg("3")
        .args(debug_lat_lon_grid.then_some("--debug-lat-lon-grid"))
        .output()
        .with_context(|| format!("failed to run {}", script_path.display()))?;
    if !output.status.success() {
        bail!(
            "NEXRAD source-grid tiler failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::remove_file(&script_path)
        .with_context(|| format!("failed to remove {}", script_path.display()))?;
    fs::remove_file(&palette_path)
        .with_context(|| format!("failed to remove {}", palette_path.display()))?;
    Ok(())
}

pub(super) fn read_live_feeds_current(
    root: &Path,
) -> anyhow::Result<Option<LiveFeedsCurrentManifest>> {
    let path = live_feeds_current_path(root);
    if !path.is_file() {
        return Ok(None);
    }
    let manifest = serde_json::from_slice(
        &fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(manifest))
}

pub(super) fn merge_live_feed_current(
    root: &Path,
    product: &str,
    entry: LiveFeedCurrentEntry,
) -> anyhow::Result<LiveFeedsCurrentManifest> {
    let mut current = read_live_feeds_current(root)?.unwrap_or(LiveFeedsCurrentManifest {
        schema_version: 1,
        generated_at_utc: utc_now_string(),
        products: BTreeMap::new(),
    });
    current.schema_version = 1;
    current.generated_at_utc = utc_now_string();
    current.products.insert(product.to_string(), entry);
    Ok(current)
}

pub(super) fn write_live_feeds_current(root: &Path) -> anyhow::Result<PathBuf> {
    let current = read_live_feeds_current(root)?.unwrap_or(LiveFeedsCurrentManifest {
        schema_version: 1,
        generated_at_utc: utc_now_string(),
        products: BTreeMap::new(),
    });
    write_live_feeds_current_manifest(root, &current)
}

pub(super) fn write_live_feeds_current_manifest(
    root: &Path,
    manifest: &LiveFeedsCurrentManifest,
) -> anyhow::Result<PathBuf> {
    let path = live_feeds_current_path(root);
    write_json_pretty_file(&path, manifest)?;
    Ok(path)
}

pub(super) fn live_feeds_relative_url(root: &Path, path: &Path) -> anyhow::Result<String> {
    Ok(path
        .strip_prefix(root)
        .with_context(|| {
            format!(
                "failed to make {} relative to {}",
                path.display(),
                root.display()
            )
        })?
        .display()
        .to_string())
}

pub(super) fn read_json_value(path: &Path) -> anyhow::Result<serde_json::Value> {
    serde_json::from_slice(
        &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))
}

pub(super) fn write_json_pretty_file(path: &Path, value: &impl Serialize) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let temp_path = path.with_extension("tmp");
    fs::write(
        &temp_path,
        serde_json::to_vec_pretty(value).context("failed to encode json")?,
    )
    .with_context(|| format!("failed to write {}", temp_path.display()))?;
    fs::rename(&temp_path, path).with_context(|| {
        format!(
            "failed to rename {} to {}",
            temp_path.display(),
            path.display()
        )
    })
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn canonical_json_sha256(value: &serde_json::Value) -> anyhow::Result<String> {
    Ok(sha256_hex(
        &serde_json::to_vec(value).context("failed to encode canonical json")?,
    ))
}

pub fn publish_discovery_manifest(
    config: &ProductBuildConfig,
    as_of_utc: DateTime<Utc>,
    bundle_filenames: &[String],
) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(&config.build_root)
        .with_context(|| format!("failed to create {}", config.build_root.display()))?;
    if bundle_filenames.is_empty() {
        bail!("publish-discovery-manifest requires at least one --bundle");
    }
    let latest_alias_path = publication_current_artifacts_path(&config.build_root);
    if !latest_alias_path.is_file() {
        bail!(
            "missing current artifacts alias {}; build-product first",
            latest_alias_path.display()
        );
    }
    let bundles = bundle_filenames
        .iter()
        .map(|filename| current_bundle_entry_from_path(&config.build_root.join(filename)))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let manifest = CurrentArtifactsManifest {
        schema_version: 1,
        artifact_roots: default_current_artifact_roots(),
        as_of_date: as_of_utc.date_naive().format("%Y-%m-%d").to_string(),
        as_of_utc: as_of_utc.to_rfc3339_opts(SecondsFormat::Secs, true),
        bundles,
        diagnostics: None,
    };
    write_current_artifacts_aliases(&config.build_root, as_of_utc, &manifest)?;
    let immutable_path = publication_root_for_packaged_root(&config.build_root)
        .join(current_artifacts_immutable_filename(as_of_utc));
    let unpacked_root = published_unpacked_root(config)?;
    fs::create_dir_all(&unpacked_root)
        .with_context(|| format!("failed to create {}", unpacked_root.display()))?;
    sync_unpacked_discovery_manifests(&config.build_root, &latest_alias_path, &unpacked_root)?;
    cleanup_published_packaged_root(&config.build_root, &latest_alias_path)?;
    cleanup_published_unpacked_root(&unpacked_root, &latest_alias_path)?;
    validate_packaged_contract(&config.build_root, &latest_alias_path)?;
    validate_unpacked_contract(&config.build_root, &unpacked_root, &latest_alias_path)?;
    Ok(immutable_path)
}

pub(super) fn obstacle_snapshot_label(value: &str) -> anyhow::Result<String> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .with_context(|| format!("failed to parse obstacle snapshot date {value}"))?;
    Ok(date.format("%Y.%m.%d").to_string())
}

pub(super) fn build_obstacles_product(
    config: &ProductBuildConfig,
) -> anyhow::Result<(PathBuf, String, NodeRecord)> {
    let snapshot_date = Utc::now().date_naive();
    let snapshot_label = obstacle_snapshot_label(&snapshot_date.format("%Y-%m-%d").to_string())?;
    let source_generated_at_utc = format!("{}T00:00:00Z", snapshot_date.format("%Y-%m-%d"));
    let obstacle_url = "https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP";
    let logical_file_name = format!("obstacle_{snapshot_label}.zip");
    let request = PrefetchRequest::new(obstacle_url)
        .with_logical_file_name(&logical_file_name)
        .with_cache_key(format!("{obstacle_url}#logical_name={logical_file_name}"));
    let inputs = BTreeMap::from([
        ("product_id".to_string(), "obstacles".to_string()),
        ("source_url".to_string(), request.cache_key.clone()),
        ("vectors_lib".to_string(), vectors_code_fingerprint()?),
    ]);
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "fast-obstacles")?,
        "fast-obstacles",
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let manifest_path = output_dir.join(format!("obstacles_{snapshot_label}.manifest"));
    let stats_path = output_dir.join("stats.json");
    let structured_json_path = output_dir.join("obstacles.json");
    let zip_path = output_dir.join(format!("obstacles_{snapshot_label}.zip"));
    let _build_lock = match claim_or_wait_for_node(
        &prepared,
        &[
            manifest_path.clone(),
            stats_path.clone(),
            structured_json_path.clone(),
            zip_path.clone(),
        ],
    )? {
        NodeCacheState::CacheHit(record) => return Ok((zip_path, source_generated_at_utc, record)),
        NodeCacheState::Build(lock) => lock,
    };

    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let work_dir = prepared.dir.join("work");
    let input_dir = work_dir.join("input");
    let provenance_dir = prepared
        .dir
        .join("meta")
        .join("provenance")
        .join("obstacles");
    fs::create_dir_all(&input_dir)
        .with_context(|| format!("failed to create {}", input_dir.display()))?;
    fs::create_dir_all(&provenance_dir)
        .with_context(|| format!("failed to create {}", provenance_dir.display()))?;
    fs::write(
        provenance_dir.join("source_urls.jsonl"),
        format!(
            "{{\"event\":\"source_url\",\"label\":\"obstacles\",\"url\":\"{}\",\"logical_file_name\":\"{}\",\"cache_key\":\"{}\"}}\n",
            request.url, logical_file_name, request.cache_key
        ),
    )
    .with_context(|| {
        format!(
            "failed to write {}",
            provenance_dir.join("source_urls.jsonl").display()
        )
    })?;
    let fetch_cache = FetchCacheConfig {
        root: config.fetch_cache_root.clone(),
        mode: FetchCacheMode::parse(&config.fetch_cache_mode)?,
    };
    prefetch_archives_with_provenance(
        std::slice::from_ref(&request),
        &input_dir,
        config.fetch_jobs,
        Some(&fetch_cache),
        &provenance_dir,
        "obstacles",
    )?;
    let result = build_obstacle_dataset(&BuildObstacleDatasetRequest {
        input_dir,
        output_dir,
        version_label: snapshot_label,
    })?;
    let outputs = BTreeMap::from([
        (
            "manifest".to_string(),
            relative_artifact_path(&result.manifest_path, &config.build_root),
        ),
        (
            "stats".to_string(),
            relative_artifact_path(&result.stats_path, &config.build_root),
        ),
        (
            "structured_json".to_string(),
            relative_artifact_path(&result.structured_json_path, &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&result.zip_path, &config.build_root),
        ),
    ]);
    let record = write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )?;
    Ok((result.zip_path, source_generated_at_utc, record))
}

pub(super) fn publish_built_fast_product(
    config: &ProductBuildConfig,
    id: &str,
    built: (PathBuf, String, NodeRecord),
) -> anyhow::Result<PublishedFastProductResult> {
    let (source_zip_path, source_generated_at_utc, record) = built;
    let (zip_sha256, zip_size_bytes) = node_output_file_detail(&record, "zip");
    let (published_zip, checksum_sha256, size_bytes) = publish_content_addressed_fast_product_zip(
        &config.build_root,
        id,
        &source_zip_path,
        zip_sha256.as_deref(),
        zip_size_bytes,
    )?;
    Ok(PublishedFastProductResult {
        id: id.to_string(),
        source_zip_path,
        published_zip,
        checksum_sha256,
        size_bytes,
        source_generated_at_utc,
    })
}

pub(super) fn build_or_reuse_fast_product<F>(
    config: &ProductBuildConfig,
    id: &str,
    previous_fast_products_by_id: &BTreeMap<String, PublishedFastProductResult>,
    gc_records: &mut BTreeMap<String, Vec<NodeRecord>>,
    build_product: F,
) -> anyhow::Result<Option<PublishedFastProductResult>>
where
    F: FnOnce(&ProductBuildConfig) -> anyhow::Result<(PathBuf, String, NodeRecord)>,
{
    match build_product(config).and_then(|built| {
        gc_records.insert(format!("fast:{id}"), vec![built.2.clone()]);
        publish_built_fast_product(config, id, built)
    }) {
        Ok(product) => Ok(Some(product)),
        Err(error) => {
            if let Some(previous) = previous_fast_products_by_id.get(id) {
                eprintln!(
                    "WARNING fast product {id} failed; reusing previous package {}: {error:#}",
                    previous.published_zip.display()
                );
                Ok(Some(previous.clone()))
            } else {
                eprintln!(
                    "WARNING fast product {id} failed and no previous package exists; omitting it from fast bundle: {error:#}"
                );
                Ok(None)
            }
        }
    }
}

pub(super) fn current_artifacts_path_for_live_feeds(
    config: &ProductBuildConfig,
) -> anyhow::Result<PathBuf> {
    let publication_root = publication_root_for_packaged_root(&config.build_root);
    let latest_alias = publication_root.join(current_artifacts_latest_alias_filename());
    if latest_alias.is_file() {
        return Ok(latest_alias);
    }

    let mut candidates = fs::read_dir(&publication_root)
        .with_context(|| format!("failed to read {}", publication_root.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to iterate {}", publication_root.display()))?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with("current_artifacts_")
                        && name.ends_with(".json")
                        && name
                            .strip_prefix("current_artifacts_")
                            .is_some_and(|suffix| suffix.contains('T'))
                })
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.pop().with_context(|| {
        format!(
            "no current_artifacts discovery manifest exists in {}; run build-product first",
            publication_root.display()
        )
    })
}

pub(super) fn sync_fast_subset_unpacked(
    build_root: &Path,
    current_artifacts_path: &Path,
    previous_fast_products: &[PublishedFastProductResult],
    fast_products: &[PublishedFastProductResult],
) -> anyhow::Result<()> {
    let unpacked_root = published_unpacked_root_from_build_root(build_root)?;
    fs::create_dir_all(&unpacked_root)
        .with_context(|| format!("failed to create {}", unpacked_root.display()))?;
    sync_unpacked_discovery_manifests(build_root, current_artifacts_path, &unpacked_root)?;
    let current: CurrentArtifactsManifest = serde_json::from_slice(
        &fs::read(current_artifacts_path)
            .with_context(|| format!("failed to read {}", current_artifacts_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", current_artifacts_path.display()))?;
    if let Some(diagnostics) = &current.diagnostics {
        sync_unpacked_file(&build_root.join(&diagnostics.filename), &unpacked_root)?;
    }
    sync_referenced_fast_bundle_unpacked_zips(
        build_root,
        &unpacked_root,
        current_artifacts_path,
        previous_fast_products,
        fast_products,
    )?;
    cleanup_published_unpacked_root(&unpacked_root, current_artifacts_path)?;
    Ok(())
}

pub(super) fn sync_referenced_fast_bundle_unpacked_zips(
    build_root: &Path,
    unpacked_root: &Path,
    current_artifacts_path: &Path,
    previous_fast_products: &[PublishedFastProductResult],
    fast_products: &[PublishedFastProductResult],
) -> anyhow::Result<()> {
    let mut products_by_filename = BTreeMap::<String, PublishedFastProductResult>::new();
    for product in previous_fast_products.iter().chain(fast_products.iter()) {
        let Some(filename) = product
            .published_zip
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
        else {
            bail!("failed to determine published fast filename");
        };
        products_by_filename.insert(filename, product.clone());
    }
    for discovery_path in discovery_manifest_paths(build_root, current_artifacts_path)? {
        let is_current_discovery = same_path(&discovery_path, current_artifacts_path);
        let current: CurrentArtifactsManifest = match serde_json::from_slice(
            &fs::read(&discovery_path)
                .with_context(|| format!("failed to read {}", discovery_path.display()))?,
        ) {
            Ok(current) => current,
            Err(error) if !is_current_discovery => {
                eprintln!(
                    "WARNING skipping stale historical discovery {} during fast unpack sync: {error:#}",
                    discovery_path.display()
                );
                continue;
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to parse {}", discovery_path.display()));
            }
        };
        for bundle in current
            .bundles
            .iter()
            .filter(|bundle| bundle.bundle_type == "fast")
        {
            let bundle_path = build_root.join(&bundle.filename);
            let fast_bundle: FastBundleManifest = match serde_json::from_slice(
                &fs::read(&bundle_path)
                    .with_context(|| format!("failed to read {}", bundle_path.display()))?,
            ) {
                Ok(bundle) => bundle,
                Err(error) if !is_current_discovery => {
                    eprintln!(
                        "WARNING skipping stale historical fast bundle {} during unpack sync: {error:#}",
                        bundle_path.display()
                    );
                    continue;
                }
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("failed to parse {}", bundle_path.display()));
                }
            };
            for product in fast_bundle_products_from_manifest(&bundle_path, &fast_bundle)? {
                let Some(filename) = product
                    .published_zip
                    .file_name()
                    .and_then(|value| value.to_str())
                    .map(str::to_string)
                else {
                    bail!("failed to determine published fast filename");
                };
                products_by_filename.entry(filename).or_insert(product);
            }
            for package in &fast_bundle.packages {
                let unpack_dir = unpacked_target_dir(unpacked_root, &package.filename)?;
                let marker_path = unpacked_marker_path(unpacked_root, &package.filename)?;
                if unpack_dir.is_dir()
                    && unpacked_dir_has_files(&unpack_dir)?
                    && fs::read_to_string(&marker_path)
                        .ok()
                        .as_deref()
                        .map(str::trim)
                        == Some(package.checksum_sha256.as_str())
                {
                    continue;
                }
                let Some(product) = products_by_filename.get(&package.filename) else {
                    if !is_current_discovery {
                        eprintln!(
                            "WARNING skipping stale historical fast package {} from {}: no source mapping available",
                            package.filename,
                            discovery_path.display()
                        );
                        continue;
                    }
                    bail!(
                        "no source mapping available to mirror historical fast package {}",
                        package.filename
                    );
                };
                if product.source_zip_path == product.published_zip {
                    sync_unpacked_zip_by_extract(
                        &product.published_zip,
                        unpacked_root,
                        &package.filename,
                        Some(&package.checksum_sha256),
                    )?;
                } else {
                    sync_unpacked_zip_from_source(
                        &product.published_zip,
                        product
                            .source_zip_path
                            .parent()
                            .unwrap_or_else(|| Path::new("/")),
                        unpacked_root,
                        &package.filename,
                        Some(&package.checksum_sha256),
                    )?;
                }
            }
        }
    }
    Ok(())
}
