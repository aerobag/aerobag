use super::*;

pub(super) fn build_source_urls_node(
    config: &ProductBuildConfig,
) -> anyhow::Result<(PathBuf, NodeRecord)> {
    if let Some(override_root) = env_path("AEROBAG_SOURCE_URLS_ROOT") {
        return build_overridden_source_urls_node(config, &override_root);
    }
    let resolved_cycle = match &config.target_cycle {
        Some(cycle) => cycle.clone(),
        None => discover_published_cycles(Some(&fetch_cache_config(config)?))?
            .into_iter()
            .last()
            .context("no published FAA cycles discovered")?,
    };
    let emit_source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/emit_source_urls.rs");
    let mut inputs = BTreeMap::from([("emit_source".to_string(), hash_file(&emit_source)?)]);
    inputs.insert("target_cycle".to_string(), hash_text(&resolved_cycle));
    let shared_root = build_shared_node_dir(config, "source-urls")?;
    let prepared = prepare_node_at(&shared_root, "source-urls", &inputs)?;
    let output_dir = prepared.dir.join("out");
    let expected = vec![
        output_dir.join("charts-sec/source_urls.jsonl"),
        output_dir.join("charts-tac/source_urls.jsonl"),
        output_dir.join("charts-enr-l/source_urls.jsonl"),
        output_dir.join("charts-enr-h/source_urls.jsonl"),
        output_dir.join("csup/source_urls.jsonl"),
        output_dir.join("tpp-ak/source_urls.jsonl"),
        output_dir.join("tpp-pac/source_urls.jsonl"),
        output_dir.join("tpp-sw/source_urls.jsonl"),
        output_dir.join("tpp-nc/source_urls.jsonl"),
        output_dir.join("tpp-ec/source_urls.jsonl"),
        output_dir.join("tpp-sc/source_urls.jsonl"),
        output_dir.join("tpp-ne/source_urls.jsonl"),
        output_dir.join("tpp-nw/source_urls.jsonl"),
        output_dir.join("tpp-se/source_urls.jsonl"),
        output_dir.join("data/source_urls.jsonl"),
    ];
    let record = run_cached_node(prepared, inputs, &expected, |_prepared| {
        fs::create_dir_all(&output_dir)?;
        emit_source_urls(
            &output_dir,
            Some(&resolved_cycle),
            Some(&fetch_cache_config(config)?),
        )?;
        Ok(BTreeMap::from([(
            "output_dir".to_string(),
            relative_artifact_path(&output_dir, &config.build_root),
        )]))
    })?;
    Ok((output_dir, record))
}

pub(super) fn fetch_cache_config(config: &ProductBuildConfig) -> anyhow::Result<FetchCacheConfig> {
    Ok(FetchCacheConfig {
        root: config.fetch_cache_root.clone(),
        mode: FetchCacheMode::parse(&config.fetch_cache_mode)?,
    })
}

pub(super) fn static_source_fetch_cache_config(
    config: &ProductBuildConfig,
) -> anyhow::Result<FetchCacheConfig> {
    let mode =
        env::var("STATIC_SOURCE_FETCH_CACHE_MODE").unwrap_or_else(|_| "cache-first".to_string());
    Ok(FetchCacheConfig {
        root: config.fetch_cache_root.clone(),
        mode: FetchCacheMode::parse(&mode)?,
    })
}

pub(super) fn terrain_fetch_cache_config(
    config: &ProductBuildConfig,
) -> anyhow::Result<FetchCacheConfig> {
    let mode = env::var("TERRAIN_FETCH_CACHE_MODE").unwrap_or_else(|_| "cache-first".to_string());
    Ok(FetchCacheConfig {
        root: config.fetch_cache_root.clone(),
        mode: FetchCacheMode::parse(&mode)?,
    })
}

pub(super) fn include_static_terrain_products() -> bool {
    env::var("AEROBAG_SKIP_STATIC_TERRAIN_PRODUCTS")
        .map(|value| value != "1" && !value.eq_ignore_ascii_case("true"))
        .unwrap_or(true)
}

pub(super) fn build_overridden_source_urls_node(
    config: &ProductBuildConfig,
    override_root: &Path,
) -> anyhow::Result<(PathBuf, NodeRecord)> {
    let inputs = BTreeMap::from([("source_urls_root".to_string(), hash_tree(override_root)?)]);
    let shared_root = build_shared_node_dir(config, "source-urls")?;
    let prepared = prepare_node_at(&shared_root, "source-urls", &inputs)?;
    let output_dir = prepared.dir.join("out");
    let expected = vec![
        output_dir.join("charts-sec/source_urls.jsonl"),
        output_dir.join("charts-tac/source_urls.jsonl"),
        output_dir.join("charts-enr-l/source_urls.jsonl"),
        output_dir.join("charts-enr-h/source_urls.jsonl"),
        output_dir.join("csup/source_urls.jsonl"),
        output_dir.join("tpp-ak/source_urls.jsonl"),
        output_dir.join("tpp-pac/source_urls.jsonl"),
        output_dir.join("tpp-sw/source_urls.jsonl"),
        output_dir.join("tpp-nc/source_urls.jsonl"),
        output_dir.join("tpp-ec/source_urls.jsonl"),
        output_dir.join("tpp-sc/source_urls.jsonl"),
        output_dir.join("tpp-ne/source_urls.jsonl"),
        output_dir.join("tpp-nw/source_urls.jsonl"),
        output_dir.join("tpp-se/source_urls.jsonl"),
        output_dir.join("data/source_urls.jsonl"),
    ];
    let record = run_cached_node(prepared, inputs, &expected, |_prepared| {
        if output_dir.exists() {
            fs::remove_dir_all(&output_dir)
                .with_context(|| format!("failed to remove {}", output_dir.display()))?;
        }
        copy_dir_recursive(override_root, &output_dir)?;
        Ok(BTreeMap::from([(
            "output_dir".to_string(),
            relative_artifact_path(&output_dir, &config.build_root),
        )]))
    })?;
    Ok((output_dir, record))
}

pub(super) fn build_chart_fetch_node(
    config: &ProductBuildConfig,
    family: ChartFamily,
    source_urls: &Path,
    fetch_jobs: usize,
) -> anyhow::Result<NodeRecord> {
    let family_id = family_slug(family).to_string();
    build_single_source_fetch_node(
        config,
        &format!("charts-{family_id}-fetch"),
        source_urls,
        fetch_jobs,
        family.capture_label(),
    )
}

pub(super) fn build_chart_process_node(
    config: &ProductBuildConfig,
    family: ChartFamily,
    source_repo: &Path,
    source_urls: &Path,
    source_fetch_record: &NodeRecord,
    cpu_jobs: usize,
) -> anyhow::Result<NodeRecord> {
    let family_id = family_slug(family).to_string();
    let node_name = format!("charts-{family_id}-process");
    let source_fetch_root =
        resolve_artifact_path(config, output_path(source_fetch_record, "source_root")?);
    let inputs = chart_process_inputs(
        family,
        source_repo,
        source_urls,
        source_content_fingerprint(source_fetch_record)?,
        cpu_jobs,
    )?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &node_name)?,
        &node_name,
        &inputs,
    )?;
    let work_dir = prepared.dir.join("work").join(family.capture_label());
    let tiles_root = work_dir.join("tiles");
    let legends_root = work_dir.join("legends");
    run_cached_node(
        prepared,
        inputs,
        &[tiles_root.clone(), legends_root.clone()],
        |prepared| {
            let work_dir = stage_work_dir(family, source_repo, &prepared.dir)?;
            seed_prefetched_source_tree(&source_fetch_root, &work_dir)?;
            build_family_vrts(family, &work_dir, cpu_jobs)?;
            build_family_legends(family, &work_dir)?;
            build_family_tiles(family, &work_dir, cpu_jobs)?;
            prune_chart_render_intermediates(&work_dir)?;
            Ok(BTreeMap::from([
                (
                    "work_dir".to_string(),
                    relative_artifact_path(&work_dir, &config.build_root),
                ),
                (
                    "tiles_root".to_string(),
                    relative_artifact_path(&tiles_root, &config.build_root),
                ),
                (
                    "legends_root".to_string(),
                    relative_artifact_path(&legends_root, &config.build_root),
                ),
            ]))
        },
    )
}

pub(super) fn prune_chart_render_intermediates(work_dir: &Path) -> anyhow::Result<()> {
    prune_chart_render_intermediates_dir(work_dir, false)
}

fn prune_chart_render_intermediates_dir(dir: &Path, in_output_dir: bool) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        let is_output_dir = matches!(
            entry.file_name().to_string_lossy().as_ref(),
            "tiles" | "legends"
        );
        let child_in_output = in_output_dir || is_output_dir;
        if file_type.is_dir() {
            prune_chart_render_intermediates_dir(&path, child_in_output)?;
            if !child_in_output {
                remove_empty_dir(&path)?;
            }
            continue;
        }
        if !child_in_output {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        }
    }
    Ok(())
}

fn remove_empty_dir(path: &Path) -> anyhow::Result<()> {
    match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::DirectoryNotEmpty => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("failed to remove {}", path.display())),
    }
}

fn build_single_source_fetch_node(
    config: &ProductBuildConfig,
    node_name: &str,
    source_urls: &Path,
    fetch_jobs: usize,
    label: &str,
) -> anyhow::Result<NodeRecord> {
    let (inputs, requests) = single_source_fetch_inputs(source_urls, fetch_jobs)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, node_name)?,
        node_name,
        &inputs,
    )?;
    let source_root = prepared.dir.join("source");
    let provenance_dir = prepared.dir.join("meta").join("provenance").join(node_name);
    let marker = prepared.dir.join(".fetch-complete");
    let mut expected = vec![marker.clone()];
    for request in &requests {
        expected.push(source_root.join(prefetch_request_file_name(request)?));
    }
    run_cached_node(prepared, inputs, &expected, |_prepared| {
        fs::create_dir_all(&source_root)
            .with_context(|| format!("failed to create {}", source_root.display()))?;
        fs::create_dir_all(&provenance_dir)
            .with_context(|| format!("failed to create {}", provenance_dir.display()))?;
        copy_source_urls_provenance(source_urls, &provenance_dir)?;
        prefetch_archives_with_provenance(
            &requests,
            &source_root,
            fetch_jobs,
            Some(&static_source_fetch_cache_config(config)?),
            &provenance_dir,
            label,
        )?;
        fs::write(&marker, b"ok")
            .with_context(|| format!("failed to write {}", marker.display()))?;
        let source_content_fingerprint = hash_tree(&source_root)?;
        Ok(BTreeMap::from([
            (
                "source_root".to_string(),
                relative_artifact_path(&source_root, &config.build_root),
            ),
            (
                "source_content_fingerprint".to_string(),
                source_content_fingerprint,
            ),
            (
                "provenance_dir".to_string(),
                relative_artifact_path(&provenance_dir, &config.build_root),
            ),
            (
                "marker".to_string(),
                relative_artifact_path(&marker, &config.build_root),
            ),
        ]))
    })
}

fn single_source_fetch_inputs(
    source_urls: &Path,
    fetch_jobs: usize,
) -> anyhow::Result<(BTreeMap<String, String>, Vec<PrefetchRequest>)> {
    let requests = dedup_prefetch_requests(read_source_prefetch_requests_jsonl(source_urls)?);
    let request_fingerprint = hash_text(
        &serde_json::to_string(&prefetch_request_manifest(&requests)?)
            .context("source prefetch request manifest json")?,
    );
    let inputs = BTreeMap::from([
        ("source_urls".to_string(), hash_file(source_urls)?),
        ("requests".to_string(), request_fingerprint),
        ("fetch_jobs".to_string(), fetch_jobs.to_string()),
        (
            "source_fetch_node_version".to_string(),
            STATIC_SOURCE_FETCH_NODE_VERSION.to_string(),
        ),
        (
            "fetch_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-fetch/src/lib.rs"),
            )?,
        ),
    ]);
    Ok((inputs, requests))
}

fn seed_prefetched_source_tree(source_root: &Path, work_dir: &Path) -> anyhow::Result<()> {
    if !source_root.is_dir() {
        anyhow::bail!("fetch node missing source root {}", source_root.display());
    }
    hard_link_or_copy_dir_recursive(source_root, work_dir)
        .with_context(|| format!("failed to seed source tree from {}", source_root.display()))
}

pub(super) fn build_chart_package_nodes(
    config: &ProductBuildConfig,
    family: ChartFamily,
    source_urls_dir: &Path,
    version_label: &str,
    source_fetch_record: &NodeRecord,
) -> anyhow::Result<(Vec<NodeRecord>, ChartSource)> {
    let family_id = family_slug(family).to_string();
    let contract_id = product_contract_id_for_family(&family_id)?;
    let artifact_version = contract_artifact_version(contract_id, version_label);
    let source_urls_path = source_urls_dir.join(format!("charts-{family_id}/source_urls.jsonl"));
    let process_node_name = format!("charts-{family_id}-process");
    let process_inputs = chart_process_inputs(
        family,
        &config.chart_metadata_root,
        &source_urls_path,
        source_content_fingerprint(source_fetch_record)?,
        config.cpu_jobs.min(8).max(1),
    )?;
    let process_prepared = prepare_node_at(
        &build_shared_node_dir(config, &process_node_name)?,
        &process_node_name,
        &process_inputs,
    )?;
    let process_record =
        load_existing_node_record(&process_prepared.record_path, &process_node_name)?;
    let work_dir = resolve_artifact_path(config, output_path(&process_record, "work_dir")?);
    let node_name = format!("charts-{family_id}-package");
    let inputs = BTreeMap::from([
        (
            "process_fingerprint".to_string(),
            process_record.fingerprint.clone(),
        ),
        (
            "package_node_contract".to_string(),
            "unpack-source-root-v1".to_string(),
        ),
        ("version_label".to_string(), version_label.to_string()),
        ("contract_id".to_string(), contract_id.to_string()),
        (
            "chart_package_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-charts/src/lib.rs"),
            )?,
        ),
    ]);
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &node_name)?,
        &node_name,
        &inputs,
    )?;
    let package_root = prepared.dir.join("output");
    let unpack_source_root = prepared.dir.join("unpack-source");
    let aggregate_path = package_root.join("package_outputs.jsonl");
    let wide_manifest_path = package_root.join(format!(
        "WIDE_{}_{}.manifest",
        manifest_chart_name(family),
        artifact_version
    ));
    let wide_zip_path = package_root.join(format!(
        "WIDE_{}_{}.zip",
        manifest_chart_name(family),
        artifact_version
    ));
    let mut expected_outputs = Vec::from([
        aggregate_path.clone(),
        unpack_source_root.clone(),
        wide_zip_path.clone(),
        wide_manifest_path.clone(),
    ]);
    for region in Region::ALL.iter() {
        expected_outputs.push(package_root.join(format!(
            "{}_{}_{}.zip",
            region.code(),
            manifest_chart_name(family),
            artifact_version
        )));
        expected_outputs.push(package_root.join(format!(
            "{}_{}_{}.manifest",
            region.code(),
            manifest_chart_name(family),
            artifact_version
        )));
    }
    let record = match claim_or_wait_for_node(&prepared, &expected_outputs)? {
        NodeCacheState::CacheHit(record) => record,
        NodeCacheState::Build(lock) => {
            let started_at_utc = utc_now_string();
            let started = Instant::now();
            if package_root.exists() {
                fs::remove_dir_all(&package_root)
                    .with_context(|| format!("failed to remove {}", package_root.display()))?;
            }
            if unpack_source_root.exists() {
                fs::remove_dir_all(&unpack_source_root).with_context(|| {
                    format!("failed to remove {}", unpack_source_root.display())
                })?;
            }
            fs::create_dir_all(&package_root)
                .with_context(|| format!("failed to create {}", package_root.display()))?;
            let mut package_records = Vec::new();
            for region in Region::ALL.iter() {
                let record = package_family_region_versioned_to(
                    family,
                    &work_dir,
                    &package_root,
                    *region,
                    version_label,
                    &artifact_version,
                )?;
                if chart_package_record_has_tiles(&record, &package_root)? {
                    package_records.push(record);
                }
            }
            package_records.push(package_family_wide_angle_versioned_to(
                family,
                &work_dir,
                &package_root,
                version_label,
                &artifact_version,
            )?);
            write_package_outputs_jsonl(&package_root, &package_records)?;
            let zip_paths = package_records
                .iter()
                .map(|record| package_root.join(&record.zip))
                .collect::<Vec<_>>();
            prepare_package_unpack_source_root(
                &zip_paths,
                &work_dir,
                &package_root,
                &unpack_source_root,
                &[],
            )?;
            let outputs = BTreeMap::from([
                (
                    "asset_root".to_string(),
                    relative_artifact_path(&work_dir, &config.build_root),
                ),
                (
                    "package_root".to_string(),
                    relative_artifact_path(&package_root, &config.build_root),
                ),
                (
                    "unpack_source_root".to_string(),
                    relative_artifact_path(&unpack_source_root, &config.build_root),
                ),
                (
                    "package_outputs".to_string(),
                    relative_artifact_path(&aggregate_path, &config.build_root),
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
            drop(lock);
            record
        }
    };
    Ok((
        vec![record],
        ChartSource {
            family_id,
            package_outputs_path: aggregate_path,
            asset_root: work_dir,
            package_root,
            unpack_source_root,
            source_urls_path: Some(source_urls_path),
        },
    ))
}

pub(super) fn chart_package_record_has_tiles(
    record: &PackageOutputRecord,
    package_root: &Path,
) -> anyhow::Result<bool> {
    if let Some(count) = record
        .metadata
        .get("tile_count")
        .and_then(|value| value.as_u64())
    {
        if count == 0 {
            return Ok(false);
        }
        return Ok(count_chart_zip_tile_entries(&package_root.join(&record.zip))? > 0);
    }
    Ok(true)
}

pub(super) fn count_chart_zip_tile_entries(path: &Path) -> anyhow::Result<u64> {
    let file = fs::File::open(path)
        .with_context(|| format!("failed to open chart zip {}", path.display()))?;
    let archive = zip::ZipArchive::new(file)
        .with_context(|| format!("failed to open chart zip {}", path.display()))?;
    let count = archive
        .file_names()
        .filter(|name| {
            if !name.ends_with(".webp") {
                return false;
            }
            let parts = name.split('/').collect::<Vec<_>>();
            parts.len() == 5 && parts[0] == "tiles"
        })
        .count();
    Ok(count as u64)
}

#[cfg(test)]
pub(super) fn chart_wide_angle_package_metadata(
    is_wide_angle: bool,
    tile_count: Option<u64>,
) -> BTreeMap<String, serde_json::Value> {
    let mut metadata = BTreeMap::from([
        (
            "wide_angle_region_id".to_string(),
            serde_json::Value::from(WIDE_ANGLE_REGION_ID),
        ),
        (
            "wide_angle_max_zoom".to_string(),
            serde_json::Value::from(FULL_COVERAGE_ZOOM),
        ),
        (
            "wide_angle".to_string(),
            serde_json::Value::from(is_wide_angle),
        ),
        (
            if is_wide_angle {
                "max_source_zoom".to_string()
            } else {
                "min_source_zoom".to_string()
            },
            serde_json::Value::from(if is_wide_angle {
                FULL_COVERAGE_ZOOM
            } else {
                FULL_COVERAGE_ZOOM + 1
            }),
        ),
    ]);
    if let Some(tile_count) = tile_count {
        metadata.insert(
            "tile_count".to_string(),
            serde_json::Value::from(tile_count),
        );
    }
    metadata
}

pub(super) fn build_csup_render_node(
    config: &ProductBuildConfig,
    region: Region,
    work_dir: &Path,
    process_fingerprint: &str,
    version_label: &str,
    render_jobs: usize,
) -> anyhow::Result<NodeRecord> {
    let node_name = format!("csup-render-{}", region.code().to_ascii_lowercase());
    let inputs = csup_render_inputs(process_fingerprint, region, render_jobs, version_label)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &node_name)?,
        &node_name,
        &inputs,
    )?;
    let marker = work_dir.join(format!(
        ".render-complete-{}",
        region.code().to_ascii_lowercase()
    ));
    run_cached_node(
        prepared,
        inputs,
        std::slice::from_ref(&marker),
        |_prepared| {
            render_csup_region(work_dir, region, render_jobs)?;
            fs::write(&marker, b"ok")
                .with_context(|| format!("failed to write {}", marker.display()))?;
            Ok(BTreeMap::from([
                (
                    "work_dir".to_string(),
                    relative_artifact_path(work_dir, &config.build_root),
                ),
                (
                    "marker".to_string(),
                    relative_artifact_path(&marker, &config.build_root),
                ),
            ]))
        },
    )
}

pub(super) fn build_csup_fetch_node(
    config: &ProductBuildConfig,
    source_urls: &Path,
    fetch_jobs: usize,
) -> anyhow::Result<NodeRecord> {
    build_single_source_fetch_node(config, "csup-fetch", source_urls, fetch_jobs, "csup")
}

pub(super) fn build_csup_process_node(
    config: &ProductBuildConfig,
    source_repo: &Path,
    source_urls: &Path,
    source_fetch_record: &NodeRecord,
) -> anyhow::Result<NodeRecord> {
    let source_fetch_root =
        resolve_artifact_path(config, output_path(source_fetch_record, "source_root")?);
    let inputs = csup_process_inputs(
        source_urls,
        source_content_fingerprint(source_fetch_record)?,
    )?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "csup-process")?,
        "csup-process",
        &inputs,
    )?;
    let work_root = prepared.dir.clone();
    let marker = work_root.join(".process-complete");
    run_cached_node(
        prepared,
        inputs,
        std::slice::from_ref(&marker),
        |_prepared| {
            let work_dir = stage_work_dir_for_product(source_repo, &work_root)?;
            seed_prefetched_source_tree(&source_fetch_root, &work_dir)?;
            prepare_csup_inputs(&work_dir)?;
            fs::write(&marker, b"ok")
                .with_context(|| format!("failed to write {}", marker.display()))?;
            Ok(BTreeMap::from([
                (
                    "work_dir".to_string(),
                    relative_artifact_path(&work_dir, &config.build_root),
                ),
                (
                    "marker".to_string(),
                    relative_artifact_path(&marker, &config.build_root),
                ),
            ]))
        },
    )
}

pub(super) fn build_csup_package_nodes(
    config: &ProductBuildConfig,
    source_urls_dir: &Path,
    version_label: &str,
    source_fetch_record: &NodeRecord,
) -> anyhow::Result<(Vec<NodeRecord>, AssetSource)> {
    let contract_id = product_contract_id_for_family("csup")?;
    let artifact_version = contract_artifact_version(contract_id, version_label);
    let source_urls_path = source_urls_dir.join("csup/source_urls.jsonl");
    let process_inputs = csup_process_inputs(
        &source_urls_path,
        source_content_fingerprint(source_fetch_record)?,
    )?;
    let process_prepared = prepare_node_at(
        &build_shared_node_dir(config, "csup-process")?,
        "csup-process",
        &process_inputs,
    )?;
    let process_record = load_existing_node_record(&process_prepared.record_path, "csup-process")?;
    let work_dir = resolve_artifact_path(config, output_path(&process_record, "work_dir")?);
    let mut inputs = BTreeMap::from([
        (
            "process_fingerprint".to_string(),
            process_record.fingerprint.clone(),
        ),
        (
            "package_node_contract".to_string(),
            "unpack-source-root-v1".to_string(),
        ),
        ("version_label".to_string(), version_label.to_string()),
        ("contract_id".to_string(), contract_id.to_string()),
        (
            "csup_package".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-csup/src/package.rs"),
            )?,
        ),
    ]);
    for region in Region::ALL.iter() {
        let render_node_name = format!("csup-render-{}", region.code().to_ascii_lowercase());
        let render_inputs = csup_render_inputs(
            &process_record.fingerprint,
            *region,
            config.cpu_jobs.max(1),
            version_label,
        )?;
        let render_prepared = prepare_node_at(
            &build_shared_node_dir(config, &render_node_name)?,
            &render_node_name,
            &render_inputs,
        )?;
        let render_record =
            load_existing_node_record(&render_prepared.record_path, &render_node_name)?;
        inputs.insert(
            format!("render_{}_fingerprint", region.code().to_ascii_lowercase()),
            render_record.fingerprint,
        );
    }
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "csup-package")?,
        "csup-package",
        &inputs,
    )?;
    let package_root = prepared.dir.join("output");
    let unpack_source_root = prepared.dir.join("unpack-source");
    let aggregate_path = package_root.join("package_outputs.jsonl");
    let mut expected_outputs = vec![aggregate_path.clone(), unpack_source_root.clone()];
    for region in Region::ALL.iter() {
        expected_outputs.push(package_root.join(format!(
            "{}_CSUP_{}.zip",
            region.code(),
            artifact_version
        )));
        expected_outputs.push(package_root.join(format!(
            "{}_CSUP_{}.manifest",
            region.code(),
            artifact_version
        )));
    }
    let record = match claim_or_wait_for_node(&prepared, &expected_outputs)? {
        NodeCacheState::CacheHit(record) => record,
        NodeCacheState::Build(lock) => {
            let started_at_utc = utc_now_string();
            let started = Instant::now();
            if package_root.exists() {
                fs::remove_dir_all(&package_root)
                    .with_context(|| format!("failed to remove {}", package_root.display()))?;
            }
            if unpack_source_root.exists() {
                fs::remove_dir_all(&unpack_source_root).with_context(|| {
                    format!("failed to remove {}", unpack_source_root.display())
                })?;
            }
            fs::create_dir_all(&package_root)
                .with_context(|| format!("failed to create {}", package_root.display()))?;
            let mut package_records = Vec::new();
            for region in Region::ALL.iter() {
                package_records.push(package_csup_region_versioned_to(
                    &work_dir,
                    &package_root,
                    *region,
                    version_label,
                    &artifact_version,
                )?);
            }
            write_package_outputs_jsonl(&package_root, &package_records)?;
            let zip_paths = package_records
                .iter()
                .map(|record| package_root.join(&record.zip))
                .collect::<Vec<_>>();
            prepare_package_unpack_source_root(
                &zip_paths,
                &work_dir,
                &package_root,
                &unpack_source_root,
                &[],
            )?;
            let outputs = BTreeMap::from([
                (
                    "asset_root".to_string(),
                    relative_artifact_path(&work_dir, &config.build_root),
                ),
                (
                    "package_root".to_string(),
                    relative_artifact_path(&package_root, &config.build_root),
                ),
                (
                    "unpack_source_root".to_string(),
                    relative_artifact_path(&unpack_source_root, &config.build_root),
                ),
                (
                    "package_outputs".to_string(),
                    relative_artifact_path(&aggregate_path, &config.build_root),
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
            drop(lock);
            record
        }
    };
    Ok((
        vec![record],
        AssetSource {
            package_outputs_path: aggregate_path,
            asset_root: work_dir.clone(),
            package_root,
            unpack_source_root,
            source_urls_path: Some(source_urls_path),
        },
    ))
}

pub(super) fn tpp_render_unit_task_name(region: Region, unit: &TppRenderUnitPlan) -> String {
    format!(
        "tpp-{}-render-unit-{}",
        region.code().to_ascii_lowercase(),
        unit.id()
    )
}

pub(super) fn tpp_render_assemble_task_name(region: Region) -> String {
    format!("tpp-{}-render-assemble", region.code().to_ascii_lowercase())
}

pub(super) fn tpp_package_plan_task_name(region: Region) -> String {
    format!("tpp-{}-package-plan", region.code().to_ascii_lowercase())
}

pub(super) fn tpp_thumbnail_task_name(region: Region, thumbnail: &TppThumbnailPlan) -> String {
    format!(
        "tpp-{}-thumbnail-{}",
        region.code().to_ascii_lowercase(),
        thumbnail.id
    )
}

pub(super) fn tpp_render_unit_records_for_plan(
    region: Region,
    plan: &TppRegionRenderPlan,
    task_node_records: &BTreeMap<String, Vec<NodeRecord>>,
) -> anyhow::Result<Vec<NodeRecord>> {
    plan.units()
        .iter()
        .map(|unit| {
            let task_id = tpp_render_unit_task_name(region, unit);
            let records = task_node_records
                .get(&task_id)
                .with_context(|| format!("missing tpp render unit task record for {task_id}"))?;
            records
                .iter()
                .find(|record| record.name.ends_with("-render-unit"))
                .cloned()
                .with_context(|| format!("missing tpp render unit node record for {task_id}"))
        })
        .collect()
}

fn tpp_plate_sources_from_unit_records(
    config: &ProductBuildConfig,
    unit_records: &[NodeRecord],
) -> anyhow::Result<TppPlateSourceMap> {
    let mut sources = TppPlateSourceMap::new();
    for unit_record in unit_records {
        let unit_plates_root =
            resolve_artifact_path(config, output_path(unit_record, "plates_root")?);
        if unit_plates_root.is_dir() {
            collect_tpp_plate_sources(config, &unit_plates_root, &unit_plates_root, &mut sources)?;
        }
    }
    Ok(sources)
}

fn collect_tpp_plate_sources(
    config: &ProductBuildConfig,
    plates_root: &Path,
    dir: &Path,
    sources: &mut TppPlateSourceMap,
) -> anyhow::Result<()> {
    let mut entries = fs::read_dir(dir)
        .with_context(|| format!("failed to read {}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to iterate {}", dir.display()))?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to stat {}", path.display()))?;
        if file_type.is_dir() {
            collect_tpp_plate_sources(config, plates_root, &path, sources)?;
            continue;
        }
        if !file_type.is_file() || path.extension().and_then(|value| value.to_str()) != Some("png")
        {
            continue;
        }
        let member = Path::new("plates")
            .join(path.strip_prefix(plates_root).with_context(|| {
                format!(
                    "failed to relativize {} under {}",
                    path.display(),
                    plates_root.display()
                )
            })?)
            .to_string_lossy()
            .replace('\\', "/");
        insert_tpp_plate_source(sources, member, path)?;
    }
    Ok(())
}

fn insert_tpp_plate_source(
    sources: &mut TppPlateSourceMap,
    member: String,
    path: PathBuf,
) -> anyhow::Result<()> {
    if let Some(existing) = sources.get(&member) {
        let existing_hash = hash_file(existing)?;
        let new_hash = hash_file(&path)?;
        if existing_hash != new_hash {
            bail!(
                "duplicate TPP package member {member} has conflicting source files: {} and {}",
                existing.display(),
                path.display()
            );
        }
        return Ok(());
    }
    sources.insert(member, path);
    Ok(())
}

fn write_tpp_plate_source_manifest(
    config: &ProductBuildConfig,
    path: &Path,
    sources: &TppPlateSourceMap,
) -> anyhow::Result<()> {
    let relative_sources = sources
        .iter()
        .map(|(member, source)| {
            (
                member.clone(),
                relative_artifact_path(source, &config.build_root),
            )
        })
        .collect::<BTreeMap<_, _>>();
    fs::write(
        path,
        serde_json::to_vec_pretty(&relative_sources)
            .context("failed to encode tpp plate source manifest")?,
    )
    .with_context(|| format!("failed to write {}", path.display()))
}

fn load_tpp_plate_source_manifest(
    config: &ProductBuildConfig,
    render_record: &NodeRecord,
) -> anyhow::Result<TppPlateSourceMap> {
    let manifest_path = resolve_artifact_path(config, output_path(render_record, "plate_sources")?);
    let relative_sources: BTreeMap<String, String> = serde_json::from_slice(
        &fs::read(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    Ok(relative_sources
        .into_iter()
        .map(|(member, path)| (member, resolve_artifact_path(config, &path)))
        .collect())
}

pub(super) fn tpp_thumbnail_records_for_plan(
    region: Region,
    plan: &TppPackagePlan,
    task_node_records: &BTreeMap<String, Vec<NodeRecord>>,
) -> anyhow::Result<Vec<NodeRecord>> {
    plan.thumbnails
        .iter()
        .map(|thumbnail| {
            let task_id = tpp_thumbnail_task_name(region, thumbnail);
            let records = task_node_records
                .get(&task_id)
                .with_context(|| format!("missing tpp thumbnail task record for {task_id}"))?;
            records
                .iter()
                .find(|record| record.name.ends_with("-thumbnail"))
                .cloned()
                .with_context(|| format!("missing tpp thumbnail node record for {task_id}"))
        })
        .collect()
}

pub(super) fn build_tpp_plan_node(
    config: &ProductBuildConfig,
    region: Region,
    source_urls: &Path,
    fetch_jobs: usize,
    source_fetch_record: Option<&NodeRecord>,
) -> anyhow::Result<(NodeRecord, PathBuf, TppRegionRenderPlan, String)> {
    let region_id = region.code().to_ascii_lowercase();
    let node_name = format!("tpp-{region_id}-plan");
    let source_fetch_root = source_fetch_record
        .map(|record| {
            output_path(record, "source_root").map(|path| resolve_artifact_path(config, path))
        })
        .transpose()?;
    let source_content_fingerprint = source_fetch_record
        .map(tpp_source_content_fingerprint)
        .transpose()?;
    let inputs = tpp_plan_inputs(
        source_urls,
        &region_id,
        fetch_jobs,
        source_content_fingerprint,
    )?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &node_name)?,
        &node_name,
        &inputs,
    )?;
    let run_root = prepared.dir.clone();
    let work_dir = run_root.join(format!("work/tpp-{region_id}"));
    let plan_path = run_root.join("meta").join("tpp-render-plan.json");
    let marker = run_root.join(".plan-complete");
    let expected = vec![
        plan_path.clone(),
        work_dir.join("d-TPP_Metafile.xml"),
        marker.clone(),
    ];
    let _build_lock = match claim_or_wait_for_node(&prepared, &expected)? {
        NodeCacheState::CacheHit(record) => {
            let plan = load_tpp_region_render_plan(&plan_path)?;
            let source_fetch_root =
                source_fetch_root.context("tpp plan requires fetched source root")?;
            let source_content_fingerprint = source_content_fingerprint
                .context("tpp plan requires source content fingerprint")?;
            return Ok((
                record,
                source_fetch_root,
                plan,
                source_content_fingerprint.to_string(),
            ));
        }
        NodeCacheState::Build(lock) => lock,
    };

    let source_fetch_root = source_fetch_root.context("tpp render requires fetched source root")?;
    let source_content_fingerprint =
        source_content_fingerprint.context("tpp render requires source content fingerprint")?;
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    fs::create_dir_all(&work_dir)
        .with_context(|| format!("failed to create {}", work_dir.display()))?;
    hard_link_or_copy_file(
        &source_fetch_root.join("d-TPP_Metafile.xml"),
        &work_dir.join("d-TPP_Metafile.xml"),
    )?;
    let plan = plan_tpp_region_render(&work_dir, &source_fetch_root, region)?;
    if let Some(parent) = plan_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(
        &plan_path,
        serde_json::to_vec_pretty(&plan).context("failed to encode tpp render plan")?,
    )
    .with_context(|| format!("failed to write {}", plan_path.display()))?;
    fs::write(&marker, b"ok").with_context(|| format!("failed to write {}", marker.display()))?;
    let outputs = BTreeMap::from([
        (
            "work_dir".to_string(),
            relative_artifact_path(&work_dir, &config.build_root),
        ),
        (
            "source_content_fingerprint".to_string(),
            source_content_fingerprint.to_string(),
        ),
        (
            "plan".to_string(),
            relative_artifact_path(&plan_path, &config.build_root),
        ),
        (
            "marker".to_string(),
            relative_artifact_path(&marker, &config.build_root),
        ),
    ]);
    let region_record = write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )?;
    Ok((
        region_record,
        source_fetch_root,
        plan,
        source_content_fingerprint.to_string(),
    ))
}

pub(super) fn build_tpp_render_unit_node(
    config: &ProductBuildConfig,
    region_id: &str,
    source_content_fingerprint: &str,
    source_root: &Path,
    unit: &TppRenderUnitPlan,
) -> anyhow::Result<NodeRecord> {
    let node_name = format!("tpp-{region_id}-render-unit");
    let inputs = tpp_render_unit_inputs(region_id, source_content_fingerprint, unit)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &node_name)?,
        &node_name,
        &inputs,
    )?;
    let work_dir = prepared.dir.join("work");
    let plates_root = work_dir.join("plates");
    let marker = prepared.dir.join(".render-complete");
    run_cached_node(
        prepared,
        inputs,
        &[plates_root.clone(), marker.clone()],
        |_prepared| {
            if work_dir.exists() {
                fs::remove_dir_all(&work_dir)
                    .with_context(|| format!("failed to remove {}", work_dir.display()))?;
            }
            let rendered_plates_root = render_tpp_unit(source_root, &work_dir, unit)?;
            fs::write(&marker, b"ok")
                .with_context(|| format!("failed to write {}", marker.display()))?;
            Ok(BTreeMap::from([
                ("unit_id".to_string(), unit.id().to_string()),
                (
                    "work_dir".to_string(),
                    relative_artifact_path(&work_dir, &config.build_root),
                ),
                (
                    "plates_root".to_string(),
                    relative_artifact_path(&rendered_plates_root, &config.build_root),
                ),
                (
                    "marker".to_string(),
                    relative_artifact_path(&marker, &config.build_root),
                ),
            ]))
        },
    )
}

pub(super) fn build_tpp_render_assemble_node(
    config: &ProductBuildConfig,
    region: Region,
    plan_record: &NodeRecord,
    unit_records: &[NodeRecord],
) -> anyhow::Result<NodeRecord> {
    let region_id = region.code().to_ascii_lowercase();
    let node_name = format!("tpp-{region_id}-render");
    let inputs = tpp_render_assemble_inputs(region, plan_record, unit_records)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &node_name)?,
        &node_name,
        &inputs,
    )?;
    let run_root = prepared.dir.clone();
    let work_dir = run_root.join(format!("work/tpp-{region_id}"));
    let plan_work_dir = resolve_artifact_path(config, output_path(plan_record, "work_dir")?);
    let plan_path = resolve_artifact_path(config, output_path(plan_record, "plan")?);
    let child_records_path = run_root.join("meta").join("tpp-render-unit-records.json");
    let plate_sources_path = run_root.join("meta").join("tpp-plate-sources.json");
    let marker = run_root.join(".render-complete");
    let expected = vec![
        child_records_path.clone(),
        plate_sources_path.clone(),
        work_dir.join("d-TPP_Metafile.xml"),
        marker.clone(),
    ];
    let _build_lock = match claim_or_wait_for_node(&prepared, &expected)? {
        NodeCacheState::CacheHit(record) => return Ok(record),
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    if work_dir.exists() {
        fs::remove_dir_all(&work_dir)
            .with_context(|| format!("failed to remove {}", work_dir.display()))?;
    }
    if let Some(parent) = child_records_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::create_dir_all(&work_dir)
        .with_context(|| format!("failed to create {}", work_dir.display()))?;
    hard_link_or_copy_file(
        &plan_work_dir.join("d-TPP_Metafile.xml"),
        &work_dir.join("d-TPP_Metafile.xml"),
    )?;
    let plate_sources = tpp_plate_sources_from_unit_records(config, unit_records)?;
    fs::write(
        &child_records_path,
        serde_json::to_vec_pretty(unit_records)
            .context("failed to encode tpp render unit node records")?,
    )
    .with_context(|| format!("failed to write {}", child_records_path.display()))?;
    write_tpp_plate_source_manifest(config, &plate_sources_path, &plate_sources)?;
    fs::write(&marker, b"ok").with_context(|| format!("failed to write {}", marker.display()))?;
    let outputs = BTreeMap::from([
        (
            "work_dir".to_string(),
            relative_artifact_path(&work_dir, &config.build_root),
        ),
        (
            "plan".to_string(),
            relative_artifact_path(&plan_path, &config.build_root),
        ),
        (
            "render_unit_records".to_string(),
            relative_artifact_path(&child_records_path, &config.build_root),
        ),
        (
            "plate_sources".to_string(),
            relative_artifact_path(&plate_sources_path, &config.build_root),
        ),
        (
            "marker".to_string(),
            relative_artifact_path(&marker, &config.build_root),
        ),
    ]);
    write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )
}

fn load_tpp_region_render_plan(path: &Path) -> anyhow::Result<TppRegionRenderPlan> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&bytes).context("failed to parse tpp render plan")
}

pub(super) fn build_tpp_fetch_node(
    config: &ProductBuildConfig,
    source_urls_dir: &Path,
) -> anyhow::Result<NodeRecord> {
    let (inputs, requests) = tpp_fetch_inputs(config, source_urls_dir)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "tpp-fetch")?,
        "tpp-fetch",
        &inputs,
    )?;
    let source_root = prepared.dir.join("source");
    let provenance_dir = prepared
        .dir
        .join("meta")
        .join("provenance")
        .join("tpp-fetch");
    let marker = prepared.dir.join(".fetch-complete");
    let mut expected = vec![marker.clone()];
    for request in &requests {
        expected.push(source_root.join(prefetch_request_file_name(request)?));
    }
    run_cached_node(prepared, inputs, &expected, |_prepared| {
        fs::create_dir_all(&source_root)
            .with_context(|| format!("failed to create {}", source_root.display()))?;
        fs::create_dir_all(&provenance_dir)
            .with_context(|| format!("failed to create {}", provenance_dir.display()))?;
        for region in Region::ALL.iter() {
            let region_id = region.code().to_ascii_lowercase();
            let source_urls_path =
                source_urls_dir.join(format!("tpp-{region_id}/source_urls.jsonl"));
            fs::copy(
                &source_urls_path,
                provenance_dir.join(format!("source_urls-{region_id}.jsonl")),
            )
            .with_context(|| {
                format!(
                    "failed to copy {} into {}",
                    source_urls_path.display(),
                    provenance_dir.display()
                )
            })?;
        }
        prefetch_archives_with_provenance(
            &requests,
            &source_root,
            config.fetch_jobs,
            Some(&static_source_fetch_cache_config(config)?),
            &provenance_dir,
            "tpp-fetch",
        )?;
        fs::write(&marker, b"ok")
            .with_context(|| format!("failed to write {}", marker.display()))?;
        let source_content_fingerprint = hash_tree(&source_root)?;
        Ok(BTreeMap::from([
            (
                "source_root".to_string(),
                relative_artifact_path(&source_root, &config.build_root),
            ),
            (
                "source_content_fingerprint".to_string(),
                source_content_fingerprint,
            ),
            (
                "provenance_dir".to_string(),
                relative_artifact_path(&provenance_dir, &config.build_root),
            ),
            (
                "marker".to_string(),
                relative_artifact_path(&marker, &config.build_root),
            ),
        ]))
    })
}

fn tpp_fetch_inputs(
    config: &ProductBuildConfig,
    source_urls_dir: &Path,
) -> anyhow::Result<(BTreeMap<String, String>, Vec<PrefetchRequest>)> {
    let mut source_url_hashes = BTreeMap::new();
    let mut requests = Vec::new();
    for region in Region::ALL.iter() {
        let region_id = region.code().to_ascii_lowercase();
        let source_urls_path = source_urls_dir.join(format!("tpp-{region_id}/source_urls.jsonl"));
        source_url_hashes.insert(region_id, hash_file(&source_urls_path)?);
        requests.extend(tpp_prefetch_requests(&source_urls_path)?);
    }
    let requests = dedup_prefetch_requests(requests);
    let request_fingerprint = hash_text(
        &serde_json::to_string(&prefetch_request_manifest(&requests)?)
            .context("tpp prefetch request manifest json")?,
    );
    let inputs = BTreeMap::from([
        (
            "source_urls".to_string(),
            hash_text(
                &serde_json::to_string(&source_url_hashes).context("tpp source url hashes json")?,
            ),
        ),
        ("requests".to_string(), request_fingerprint),
        ("fetch_jobs".to_string(), config.fetch_jobs.to_string()),
        (
            "tpp_fetch_node_version".to_string(),
            TPP_FETCH_NODE_VERSION.to_string(),
        ),
        (
            "cache_layout_version".to_string(),
            TPP_CACHE_LAYOUT_VERSION.to_string(),
        ),
        (
            "fetch_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-fetch/src/lib.rs"),
            )?,
        ),
    ]);
    Ok((inputs, requests))
}

fn dedup_prefetch_requests(mut requests: Vec<PrefetchRequest>) -> Vec<PrefetchRequest> {
    requests.sort_by_key(prefetch_request_key);
    requests.dedup_by_key(|request| prefetch_request_key(request));
    requests
}

fn prefetch_request_key(request: &PrefetchRequest) -> (String, String, Option<String>, bool, bool) {
    (
        request.cache_key.clone(),
        request.url.clone(),
        request.logical_file_name.clone(),
        request.force_http1,
        request.allow_html,
    )
}

fn prefetch_request_manifest(
    requests: &[PrefetchRequest],
) -> anyhow::Result<Vec<serde_json::Value>> {
    requests
        .iter()
        .map(|request| {
            Ok(serde_json::json!({
                "allow_html": request.allow_html,
                "cache_key": &request.cache_key,
                "file_name": prefetch_request_file_name(request)?,
                "force_http1": request.force_http1,
                "logical_file_name": &request.logical_file_name,
                "url": &request.url,
            }))
        })
        .collect()
}

fn prefetch_request_file_name(request: &PrefetchRequest) -> anyhow::Result<String> {
    if let Some(file_name) = &request.logical_file_name {
        return Ok(file_name.clone());
    }
    request
        .url
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .with_context(|| format!("prefetch request URL has no file name: {}", request.url))
}

fn tpp_source_content_fingerprint(record: &NodeRecord) -> anyhow::Result<&str> {
    source_content_fingerprint(record)
}

fn source_content_fingerprint(record: &NodeRecord) -> anyhow::Result<&str> {
    record
        .outputs
        .get("source_content_fingerprint")
        .map(String::as_str)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "node {} missing outputs.source_content_fingerprint",
                record.name
            )
        })
}

pub(super) fn build_tpp_package_plan_node(
    config: &ProductBuildConfig,
    region: Region,
    source_urls_path: &Path,
    version_label: &str,
    render_record: &NodeRecord,
) -> anyhow::Result<(NodeRecord, PathBuf, TppPlateSourceMap, TppPackagePlan)> {
    let contract_id = product_contract_id_for_family("tpp")?;
    let artifact_version = contract_artifact_version(contract_id, version_label);
    let metadata_root = resolve_artifact_path(config, output_path(render_record, "work_dir")?);
    let plate_sources = load_tpp_plate_source_manifest(config, render_record)?;
    let inputs = tpp_package_plan_inputs(
        region,
        source_urls_path,
        version_label,
        contract_id,
        render_record,
    )?;
    let node_name = tpp_package_plan_task_name(region);
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &node_name)?,
        &node_name,
        &inputs,
    )?;
    let plan_path = prepared.dir.join("meta").join("tpp-package-plan.json");
    let marker = prepared.dir.join(".package-plan-complete");
    let _build_lock = match claim_or_wait_for_node(&prepared, &[plan_path.clone(), marker.clone()])?
    {
        NodeCacheState::CacheHit(record) => {
            let plan = load_tpp_package_plan(&plan_path)?;
            return Ok((record, metadata_root, plate_sources, plan));
        }
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let plan = plan_package_region_from_members(
        region,
        version_label,
        &artifact_version,
        plate_sources.keys().cloned().collect(),
    )?;
    if let Some(parent) = plan_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(
        &plan_path,
        serde_json::to_vec_pretty(&plan).context("failed to encode tpp package plan")?,
    )
    .with_context(|| format!("failed to write {}", plan_path.display()))?;
    fs::write(&marker, b"ok").with_context(|| format!("failed to write {}", marker.display()))?;
    let outputs = BTreeMap::from([
        (
            "metadata_root".to_string(),
            relative_artifact_path(&metadata_root, &config.build_root),
        ),
        (
            "plate_sources".to_string(),
            output_path(render_record, "plate_sources")?.to_string(),
        ),
        (
            "plan".to_string(),
            relative_artifact_path(&plan_path, &config.build_root),
        ),
        ("package_id".to_string(), plan.package_id.clone()),
        ("manifest_name".to_string(), plan.manifest_name.clone()),
        ("zip_name".to_string(), plan.zip_name.clone()),
        (
            "marker".to_string(),
            relative_artifact_path(&marker, &config.build_root),
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
    Ok((record, metadata_root, plate_sources, plan))
}

pub(super) fn build_tpp_thumbnail_node(
    config: &ProductBuildConfig,
    region: Region,
    source_png: &Path,
    thumbnail: &TppThumbnailPlan,
) -> anyhow::Result<NodeRecord> {
    let region_id = region.code().to_ascii_lowercase();
    let node_name = format!("tpp-{region_id}-thumbnail");
    let inputs = tpp_thumbnail_inputs(region, source_png, thumbnail)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &node_name)?,
        &node_name,
        &inputs,
    )?;
    let output_root = prepared.dir.join("output");
    let thumbnail_path = output_root.join(&thumbnail.thumbnail_path);
    let marker = prepared.dir.join(".thumbnail-complete");
    run_cached_node(
        prepared,
        inputs,
        &[thumbnail_path.clone(), marker.clone()],
        |_prepared| {
            if output_root.exists() {
                fs::remove_dir_all(&output_root)
                    .with_context(|| format!("failed to remove {}", output_root.display()))?;
            }
            let written = write_tpp_thumbnail_from_source(source_png, &output_root, thumbnail)?;
            if written != thumbnail_path {
                bail!(
                    "tpp thumbnail writer returned {} but expected {}",
                    written.display(),
                    thumbnail_path.display()
                );
            }
            fs::write(&marker, b"ok")
                .with_context(|| format!("failed to write {}", marker.display()))?;
            Ok(BTreeMap::from([
                ("thumbnail_id".to_string(), thumbnail.id.clone()),
                ("asset_path".to_string(), thumbnail.asset_path.clone()),
                (
                    "thumbnail_path".to_string(),
                    thumbnail.thumbnail_path.clone(),
                ),
                (
                    "thumbnail".to_string(),
                    relative_artifact_path(&thumbnail_path, &config.build_root),
                ),
                (
                    "marker".to_string(),
                    relative_artifact_path(&marker, &config.build_root),
                ),
            ]))
        },
    )
}

pub(super) fn build_tpp_package_assemble_node(
    config: &ProductBuildConfig,
    region: Region,
    source_urls_path: &Path,
    plan_record: &NodeRecord,
    metadata_root: &Path,
    plate_sources: &TppPlateSourceMap,
    plan: &TppPackagePlan,
    thumbnail_records: &[NodeRecord],
) -> anyhow::Result<(NodeRecord, AssetSource)> {
    let region_id = region.code().to_ascii_lowercase();
    let inputs = tpp_package_assemble_inputs(region, plan_record, thumbnail_records)?;
    let node_name = format!("tpp-{region_id}-package");
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &node_name)?,
        &node_name,
        &inputs,
    )?;
    let package_root = prepared.dir.join("output");
    let unpack_source_root = prepared.dir.join("unpack-source");
    let provenance_dir = prepared
        .dir
        .join("meta")
        .join("provenance")
        .join(format!("tpp-{region_id}"));
    let package_outputs_path = provenance_dir.join("package_outputs.jsonl");
    let zip_path = package_root.join(&plan.zip_name);
    let manifest_path = package_root.join(&plan.manifest_name);
    let _build_lock = match claim_or_wait_for_node(
        &prepared,
        &[
            package_outputs_path.clone(),
            unpack_source_root.clone(),
            zip_path.clone(),
            manifest_path.clone(),
        ],
    )? {
        NodeCacheState::CacheHit(record) => {
            return Ok((
                record,
                AssetSource {
                    package_outputs_path,
                    asset_root: metadata_root.to_path_buf(),
                    package_root: package_root.clone(),
                    unpack_source_root: unpack_source_root.clone(),
                    source_urls_path: Some(source_urls_path.to_path_buf()),
                },
            ));
        }
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    if package_root.exists() {
        fs::remove_dir_all(&package_root)
            .with_context(|| format!("failed to remove {}", package_root.display()))?;
    }
    if unpack_source_root.exists() {
        fs::remove_dir_all(&unpack_source_root)
            .with_context(|| format!("failed to remove {}", unpack_source_root.display()))?;
    }
    let thumbnail_sources = thumbnail_sources_from_records(config, plan, thumbnail_records)?;
    let package_count = assemble_package_region_from_sources(
        metadata_root,
        &package_root,
        &provenance_dir,
        region,
        plan,
        plate_sources,
        &thumbnail_sources,
    )?;
    prepare_package_unpack_source_root_with_member_sources(
        std::slice::from_ref(&zip_path),
        metadata_root,
        &package_root,
        &unpack_source_root,
        &["thumbnails/"],
        Some(plate_sources),
    )?;
    let outputs = BTreeMap::from([
        (
            "metadata_root".to_string(),
            relative_artifact_path(metadata_root, &config.build_root),
        ),
        (
            "package_root".to_string(),
            relative_artifact_path(&package_root, &config.build_root),
        ),
        (
            "unpack_source_root".to_string(),
            relative_artifact_path(&unpack_source_root, &config.build_root),
        ),
        (
            "package_outputs".to_string(),
            relative_artifact_path(&package_outputs_path, &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&zip_path, &config.build_root),
        ),
        (
            "manifest".to_string(),
            relative_artifact_path(&manifest_path, &config.build_root),
        ),
        ("package_count".to_string(), package_count.to_string()),
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
    Ok((
        record,
        AssetSource {
            package_outputs_path,
            asset_root: metadata_root.to_path_buf(),
            package_root,
            unpack_source_root,
            source_urls_path: Some(source_urls_path.to_path_buf()),
        },
    ))
}

fn load_tpp_package_plan(path: &Path) -> anyhow::Result<TppPackagePlan> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&bytes).context("failed to parse tpp package plan")
}

fn thumbnail_sources_from_records(
    config: &ProductBuildConfig,
    plan: &TppPackagePlan,
    thumbnail_records: &[NodeRecord],
) -> anyhow::Result<BTreeMap<String, PathBuf>> {
    if thumbnail_records.len() != plan.thumbnails.len() {
        bail!(
            "tpp package plan expected {} thumbnails but received {} node records",
            plan.thumbnails.len(),
            thumbnail_records.len()
        );
    }
    let mut sources = BTreeMap::new();
    for (thumbnail, record) in plan.thumbnails.iter().zip(thumbnail_records.iter()) {
        let record_thumbnail_path = record
            .outputs
            .get("thumbnail_path")
            .with_context(|| format!("node {} missing outputs.thumbnail_path", record.name))?;
        if record_thumbnail_path != &thumbnail.thumbnail_path {
            bail!(
                "thumbnail node path mismatch for {}: record has {}, plan has {}",
                thumbnail.id,
                record_thumbnail_path,
                thumbnail.thumbnail_path
            );
        }
        let thumbnail_file = resolve_artifact_path(config, output_path(record, "thumbnail")?);
        sources.insert(thumbnail.thumbnail_path.clone(), thumbnail_file);
    }
    Ok(sources)
}

pub(super) fn chart_process_inputs(
    family: ChartFamily,
    source_repo: &Path,
    source_urls: &Path,
    source_content_fingerprint: &str,
    cpu_jobs: usize,
) -> anyhow::Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        ("family".to_string(), family_slug(family).to_string()),
        ("source_repo".to_string(), hash_tree(source_repo)?),
        ("source_urls".to_string(), hash_file(source_urls)?),
        ("cpu_jobs".to_string(), cpu_jobs.to_string()),
        (
            "source_content_fingerprint".to_string(),
            source_content_fingerprint.to_string(),
        ),
        (
            "chart_render_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-charts/src/lib.rs"),
            )?,
        ),
    ]))
}

pub(super) fn csup_process_inputs(
    source_urls: &Path,
    source_content_fingerprint: &str,
) -> anyhow::Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        ("source_urls".to_string(), hash_file(source_urls)?),
        (
            "source_content_fingerprint".to_string(),
            source_content_fingerprint.to_string(),
        ),
        (
            "csup_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-csup/src/lib.rs"),
            )?,
        ),
    ]))
}

pub(super) fn csup_render_inputs(
    process_fingerprint: &str,
    region: Region,
    render_jobs: usize,
    version_label: &str,
) -> anyhow::Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        (
            "process_fingerprint".to_string(),
            process_fingerprint.to_string(),
        ),
        ("region".to_string(), region.code().to_string()),
        ("render_jobs".to_string(), render_jobs.to_string()),
        ("version_label".to_string(), version_label.to_string()),
        (
            "csup_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-csup/src/lib.rs"),
            )?,
        ),
        (
            "png_tools".to_string(),
            hash_file(preprocessor_tools_src_path("png.rs"))?,
        ),
        (
            "tool_invocation".to_string(),
            hash_file(preprocessor_tools_src_path("tool_invocation.rs"))?,
        ),
    ]))
}

pub(super) fn tpp_plan_inputs(
    source_urls: &Path,
    region_id: &str,
    fetch_jobs: usize,
    source_content_fingerprint: Option<&str>,
) -> anyhow::Result<BTreeMap<String, String>> {
    let mut inputs = BTreeMap::from([
        ("region".to_string(), region_id.to_string()),
        ("source_urls".to_string(), hash_file(source_urls)?),
        ("fetch_jobs".to_string(), fetch_jobs.to_string()),
        (
            "tpp_render_node_version".to_string(),
            TPP_RENDER_NODE_VERSION.to_string(),
        ),
        (
            "cache_layout_version".to_string(),
            TPP_CACHE_LAYOUT_VERSION.to_string(),
        ),
        (
            "find_plate_pages_script".to_string(),
            hash_file(tpp_crate_path().join("scripts/find_plate_pages.py"))?,
        ),
        (
            "detect_landscape_rotation_script".to_string(),
            hash_file(tpp_crate_path().join("scripts/detect_landscape_rotation.py"))?,
        ),
    ]);
    if let Some(fingerprint) = source_content_fingerprint {
        inputs.insert(
            "source_content_fingerprint".to_string(),
            fingerprint.to_string(),
        );
    }
    Ok(inputs)
}

fn tpp_render_unit_inputs(
    region_id: &str,
    source_content_fingerprint: &str,
    unit: &TppRenderUnitPlan,
) -> anyhow::Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        ("region".to_string(), region_id.to_string()),
        ("unit_id".to_string(), unit.id().to_string()),
        (
            "source_content_fingerprint".to_string(),
            source_content_fingerprint.to_string(),
        ),
        (
            "unit_plan_hash".to_string(),
            hash_text(&serde_json::to_string(unit).context("tpp render unit plan json")?),
        ),
        (
            "tpp_render_node_version".to_string(),
            TPP_RENDER_NODE_VERSION.to_string(),
        ),
    ]))
}

fn tpp_render_assemble_inputs(
    region: Region,
    plan_record: &NodeRecord,
    unit_records: &[NodeRecord],
) -> anyhow::Result<BTreeMap<String, String>> {
    let unit_fingerprints = unit_records
        .iter()
        .map(|record| {
            serde_json::json!({
                "name": record.name,
                "fingerprint": record.fingerprint,
            })
        })
        .collect::<Vec<_>>();
    Ok(BTreeMap::from([
        ("region".to_string(), region.code().to_ascii_lowercase()),
        (
            "plan_fingerprint".to_string(),
            plan_record.fingerprint.clone(),
        ),
        (
            "render_unit_fingerprints".to_string(),
            hash_text(
                &serde_json::to_string(&unit_fingerprints)
                    .context("tpp render unit fingerprint json")?,
            ),
        ),
        (
            "tpp_render_assemble_node_version".to_string(),
            TPP_RENDER_ASSEMBLE_NODE_VERSION.to_string(),
        ),
    ]))
}

fn tpp_package_plan_inputs(
    region: Region,
    source_urls: &Path,
    version_label: &str,
    contract_id: &str,
    render_record: &NodeRecord,
) -> anyhow::Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        ("region".to_string(), region.code().to_ascii_lowercase()),
        ("source_urls".to_string(), hash_file(source_urls)?),
        ("version_label".to_string(), version_label.to_string()),
        ("contract_id".to_string(), contract_id.to_string()),
        (
            "render_fingerprint".to_string(),
            render_record.fingerprint.clone(),
        ),
        (
            "package_node_contract".to_string(),
            "unpack-source-root-v1".to_string(),
        ),
        (
            "tpp_package_node_version".to_string(),
            TPP_PACKAGE_NODE_VERSION.to_string(),
        ),
        (
            "cache_layout_version".to_string(),
            TPP_CACHE_LAYOUT_VERSION.to_string(),
        ),
        (
            "tpp_package".to_string(),
            hash_file(tpp_crate_path().join("src/package.rs"))?,
        ),
    ]))
}

fn tpp_thumbnail_inputs(
    region: Region,
    source_png: &Path,
    thumbnail: &TppThumbnailPlan,
) -> anyhow::Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        ("region".to_string(), region.code().to_ascii_lowercase()),
        ("thumbnail_id".to_string(), thumbnail.id.clone()),
        ("asset_path".to_string(), thumbnail.asset_path.clone()),
        (
            "thumbnail_path".to_string(),
            thumbnail.thumbnail_path.clone(),
        ),
        ("source_png".to_string(), hash_file(source_png)?),
        (
            "tpp_thumbnail_node_version".to_string(),
            TPP_THUMBNAIL_NODE_VERSION.to_string(),
        ),
        (
            "tpp_thumbnail".to_string(),
            hash_file(tpp_crate_path().join("src/thumbnail.rs"))?,
        ),
    ]))
}

fn tpp_package_assemble_inputs(
    region: Region,
    plan_record: &NodeRecord,
    thumbnail_records: &[NodeRecord],
) -> anyhow::Result<BTreeMap<String, String>> {
    let thumbnail_fingerprints = thumbnail_records
        .iter()
        .map(|record| {
            let thumbnail_path = record
                .outputs
                .get("thumbnail_path")
                .with_context(|| format!("node {} missing outputs.thumbnail_path", record.name))?;
            Ok(serde_json::json!({
                "name": record.name,
                "fingerprint": record.fingerprint,
                "thumbnail_path": thumbnail_path,
            }))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(BTreeMap::from([
        ("region".to_string(), region.code().to_ascii_lowercase()),
        (
            "package_plan_fingerprint".to_string(),
            plan_record.fingerprint.clone(),
        ),
        (
            "thumbnail_fingerprints".to_string(),
            hash_text(
                &serde_json::to_string(&thumbnail_fingerprints)
                    .context("tpp thumbnail fingerprint json")?,
            ),
        ),
        (
            "package_node_contract".to_string(),
            "unpack-source-root-v1".to_string(),
        ),
        (
            "tpp_package_node_version".to_string(),
            TPP_PACKAGE_NODE_VERSION.to_string(),
        ),
        (
            "cache_layout_version".to_string(),
            TPP_CACHE_LAYOUT_VERSION.to_string(),
        ),
        (
            "tpp_package".to_string(),
            hash_file(tpp_crate_path().join("src/package.rs"))?,
        ),
    ]))
}

fn workspace_preprocessor_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("preprocessor-cli should live under workspace root")
        .to_path_buf()
}

fn preprocessor_tools_src_path(file_name: &str) -> PathBuf {
    workspace_preprocessor_path()
        .join("preprocessor-tools/src")
        .join(file_name)
}

fn tpp_crate_path() -> PathBuf {
    workspace_preprocessor_path().join("preprocessor-tpp")
}

pub(super) fn build_data_nodes(
    config: &ProductBuildConfig,
    source_urls_dir: &Path,
    node_name: &str,
) -> anyhow::Result<Vec<NodeRecord>> {
    let source_urls = source_urls_dir.join("data/source_urls.jsonl");
    let data_version = data_version_label(source_urls_dir)?;
    let data_manifest_version = data_manifest_cycle(source_urls_dir)?;
    let (staged_input_dir, staging_record) = build_data_input_node(config, &source_urls)?;

    let artifact_stem = data_version.clone();
    let inputs = BTreeMap::from([
        (
            "staged_input_dir".to_string(),
            relative_artifact_path(&staged_input_dir, &config.build_root),
        ),
        (
            "staged_input_fingerprint".to_string(),
            staging_record.fingerprint.clone(),
        ),
        ("source_urls".to_string(), hash_file(&source_urls)?),
        (
            "manifest_version".to_string(),
            data_manifest_version.clone(),
        ),
        ("artifact_stem".to_string(), artifact_stem.clone()),
        (
            "data_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-data/src/lib.rs"),
            )?,
        ),
    ]);
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, node_name)?,
        node_name,
        &inputs,
    )?;
    let provenance_dir = prepared.dir.join(format!("meta/provenance/{node_name}"));
    fs::create_dir_all(&provenance_dir)?;
    copy_source_urls_provenance(&source_urls, &provenance_dir)?;

    let request = DataBuildRequest {
        input_dir: staged_input_dir.clone(),
        output_dir: prepared.dir.join("output"),
        manifest_version: data_manifest_version.clone(),
        artifact_stem: Some(artifact_stem),
    };
    let manifest_path = request.output_dir.join(format!(
        "{}.manifest",
        request.artifact_stem.as_deref().unwrap_or("databases")
    ));
    let zip_path = request.output_dir.join(format!(
        "{}.zip",
        request.artifact_stem.as_deref().unwrap_or("databases")
    ));
    let _build_lock =
        match claim_or_wait_for_node(&prepared, &[manifest_path.clone(), zip_path.clone()])? {
            NodeCacheState::CacheHit(record) => return Ok(vec![staging_record, record]),
            NodeCacheState::Build(lock) => lock,
        };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let result = build_data_package(&request)?;
    let outputs = BTreeMap::from([
        (
            "intermediate_sqlite_db".to_string(),
            relative_artifact_path(&result.main_db, &config.build_root),
        ),
        (
            "manifest".to_string(),
            relative_artifact_path(&result.manifest_path, &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&result.zip_path, &config.build_root),
        ),
    ]);
    let build_record = write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )?;
    Ok(vec![staging_record, build_record])
}

pub(super) fn build_data_match_node(
    config: &ProductBuildConfig,
    raw_intermediate_sqlite_db: &Path,
    raw_zip: &Path,
    artifact_stem: &str,
    raw_data_fingerprint: &str,
    tpp_sources: &[(Region, AssetSource, String)],
) -> anyhow::Result<NodeRecord> {
    let mut inputs = BTreeMap::from([
        (
            "raw_data_fingerprint".to_string(),
            raw_data_fingerprint.to_string(),
        ),
        ("artifact_stem".to_string(), artifact_stem.to_string()),
        (
            "matching_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-data/src/tpp_cifp_matching.rs"),
            )?,
        ),
        (
            "data_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-data/src/lib.rs"),
            )?,
        ),
        (
            "core_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-core/src/lib.rs"),
            )?,
        ),
    ]);
    let mut tpp_zips = Vec::new();
    for (region, source, fingerprint) in tpp_sources {
        let package = package_record_for_region(&source.package_outputs_path, *region)?;
        let zip_path = source.package_root.join(&package.zip);
        inputs.insert(
            format!("tpp_{}_fingerprint", region.code().to_ascii_lowercase()),
            fingerprint.clone(),
        );
        tpp_zips.push(zip_path);
    }
    let prepared = prepare_node_at(&build_shared_node_dir(config, "data")?, "data", &inputs)?;
    let output_dir = prepared.dir.join("output");
    let manifest_path = output_dir.join(format!("{artifact_stem}.manifest"));
    let zip_path = output_dir.join(format!("{artifact_stem}.zip"));
    let intermediate_sqlite_db_path = output_dir.join("intermediate-sqlite.db");
    let _build_lock = match claim_or_wait_for_node(
        &prepared,
        &[
            intermediate_sqlite_db_path.clone(),
            manifest_path.clone(),
            zip_path.clone(),
        ],
    )? {
        NodeCacheState::CacheHit(record) => return Ok(record),
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let result = build_data_package_with_tpp_matches(&DataTppMatchRequest {
        input_main_db: raw_intermediate_sqlite_db.to_path_buf(),
        input_zip: raw_zip.to_path_buf(),
        output_dir: output_dir.clone(),
        artifact_stem: artifact_stem.to_string(),
        tpp_package_zips: tpp_zips,
    })?;
    let outputs = BTreeMap::from([
        (
            "intermediate_sqlite_db".to_string(),
            relative_artifact_path(&result.main_db, &config.build_root),
        ),
        (
            "manifest".to_string(),
            relative_artifact_path(&result.manifest_path, &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&result.zip_path, &config.build_root),
        ),
    ]);
    write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )
}

pub(super) fn build_vectors_node(
    config: &ProductBuildConfig,
    intermediate_sqlite_db: &Path,
    source_input_dir: &Path,
    data_fingerprint: &str,
    version_label: &str,
) -> anyhow::Result<NodeRecord> {
    let inputs = BTreeMap::from([
        ("data_fingerprint".to_string(), data_fingerprint.to_string()),
        ("include_class_e_airspace".to_string(), "false".to_string()),
        (
            "source_input_dir".to_string(),
            relative_artifact_path(source_input_dir, &config.build_root),
        ),
        ("version_label".to_string(), version_label.to_string()),
        ("vectors_lib".to_string(), vectors_code_fingerprint()?),
    ]);
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "vectors")?,
        "vectors",
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let request = BuildVectorsRequest {
        main_db: intermediate_sqlite_db.to_path_buf(),
        data_input_dir: Some(source_input_dir.to_path_buf()),
        output_dir: output_dir.clone(),
        version_label: version_label.to_string(),
        include_class_e_airspace: false,
    };
    let had_pairs_path = output_dir.join(format!("vectors_{version_label}.had-pairs.jsonl"));
    let stats_path = output_dir.join("stats.json");
    let errors_path = output_dir.join("errors.json");
    let _build_lock = match claim_or_wait_for_node(
        &prepared,
        &[
            had_pairs_path.clone(),
            stats_path.clone(),
            errors_path.clone(),
        ],
    )? {
        NodeCacheState::CacheHit(record) => return Ok(record),
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let result = build_vectors_dataset(&request)?;
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
            "errors".to_string(),
            relative_artifact_path(&result.errors_path, &config.build_root),
        ),
        (
            "had_pairs".to_string(),
            relative_artifact_path(&result.had_pairs_path, &config.build_root),
        ),
    ]);
    write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )
}

pub(super) fn build_data_input_node(
    config: &ProductBuildConfig,
    source_urls: &Path,
) -> anyhow::Result<(PathBuf, NodeRecord)> {
    let requests = cycle_data_requests(read_source_prefetch_requests_jsonl(source_urls)?);
    let inputs = BTreeMap::from([
        ("source_urls".to_string(), hash_file(source_urls)?),
        ("fetch_jobs".to_string(), config.fetch_jobs.to_string()),
        (
            "cycle_urls".to_string(),
            hash_text(
                &requests
                    .iter()
                    .map(|request| request.cache_key.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        ),
        (
            "fetch_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-fetch/src/lib.rs"),
            )?,
        ),
    ]);
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "data-input-staging")?,
        "data-input-staging",
        &inputs,
    )?;
    let staged_root = prepared.dir.join("out");
    let marker = staged_root.join(".staged-complete");
    let _build_lock = match claim_or_wait_for_node(&prepared, std::slice::from_ref(&marker))? {
        NodeCacheState::CacheHit(record) => return Ok((staged_root, record)),
        NodeCacheState::Build(lock) => lock,
    };

    if staged_root.exists() {
        fs::remove_dir_all(&staged_root)
            .with_context(|| format!("failed to remove {}", staged_root.display()))?;
    }
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    fs::create_dir_all(&staged_root)
        .with_context(|| format!("failed to create {}", staged_root.display()))?;
    let provenance_dir = prepared.dir.join("meta/provenance/data-input-staging");
    fs::create_dir_all(&provenance_dir)?;
    copy_source_urls_provenance(source_urls, &provenance_dir)?;
    prefetch_archives_with_provenance(
        &requests,
        &staged_root,
        config.fetch_jobs,
        Some(&static_source_fetch_cache_config(config)?),
        &provenance_dir,
        "data",
    )?;
    fs::write(&marker, b"ok").with_context(|| format!("failed to write {}", marker.display()))?;
    let outputs = BTreeMap::from([
        (
            "staged_input_dir".to_string(),
            relative_artifact_path(&staged_root, &config.build_root),
        ),
        (
            "provenance_dir".to_string(),
            relative_artifact_path(&provenance_dir, &config.build_root),
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
    Ok((staged_root, record))
}

pub(super) fn build_resource_index_node(
    config: &ProductBuildConfig,
    nav_db_zip: &Path,
    chart_sources: Vec<ChartSource>,
    tpp_sources: Vec<AssetSource>,
    csup_sources: Vec<AssetSource>,
) -> anyhow::Result<NodeRecord> {
    let node_root = build_shared_node_dir(config, "resource-index")?;
    let chart_json = chart_sources
        .iter()
        .map(|source| {
            Ok(format!(
                "{}:{}:{}:{}:{}",
                source.family_id,
                source.package_outputs_path.display(),
                hash_file(&source.package_outputs_path)?,
                source.package_root.display(),
                source
                    .source_urls_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default()
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?
        .join("\n");
    let tpp_json = tpp_sources
        .iter()
        .map(|source| {
            Ok(format!(
                "{}:{}:{}:{}:{}",
                source.package_outputs_path.display(),
                hash_file(&source.package_outputs_path)?,
                source.asset_root.display(),
                source.package_root.display(),
                source
                    .source_urls_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default()
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?
        .join("\n");
    let csup_json = csup_sources
        .iter()
        .map(|source| {
            Ok(format!(
                "{}:{}:{}:{}:{}",
                source.package_outputs_path.display(),
                hash_file(&source.package_outputs_path)?,
                source.asset_root.display(),
                source.package_root.display(),
                source
                    .source_urls_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default()
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?
        .join("\n");
    let inputs = BTreeMap::from([
        ("nav_db_zip".to_string(), hash_file(nav_db_zip)?),
        ("chart_sources".to_string(), hash_text(&chart_json)),
        ("tpp_sources".to_string(), hash_text(&tpp_json)),
        ("csup_sources".to_string(), hash_text(&csup_json)),
        (
            "resource_index_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-resource-index/src/lib.rs"),
            )?,
        ),
        (
            "tools_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-tools/src/lib.rs"),
            )?,
        ),
    ]);
    let prepared = prepare_node_at(&node_root, "resource-index", &inputs)?;
    let output_path = prepared.dir.join("resource-index.json");
    let catalog_output_path = prepared.dir.join("catalog.json");
    let thumbnail_root = prepared.dir.join("thumbnails");
    if let Some(record) =
        try_load_node_record(&prepared, &[output_path.clone(), thumbnail_root.clone()])?
    {
        return Ok(record);
    }
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let request = BuildResourceIndexRequest {
        nav_db_zip: nav_db_zip.to_path_buf(),
        output_path: output_path.clone(),
        catalog_output_path: Some(catalog_output_path.clone()),
        chart_sources,
        tpp_sources,
        csup_sources,
    };
    write_resource_index(&request)?;
    let outputs = BTreeMap::from([
        (
            "resource_index".to_string(),
            relative_artifact_path(&output_path, &config.build_root),
        ),
        (
            "catalog".to_string(),
            relative_artifact_path(&catalog_output_path, &config.build_root),
        ),
    ]);
    write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )
}

pub(super) fn prepare_node_at(
    root: &Path,
    name: &str,
    inputs: &BTreeMap<String, String>,
) -> anyhow::Result<PreparedNode> {
    let fingerprint = fingerprint_for_node(name, inputs)?;
    let dir = root.join(&fingerprint);
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    Ok(PreparedNode {
        name: name.to_string(),
        fingerprint,
        record_path: dir.join("build-record.json"),
        lock_path: dir.join(".build-lock"),
        dir,
    })
}

pub(super) fn summarize_package_records(records: &[NodeRecord]) -> PackageSummary {
    let total = records.len();
    let cache_hits = records.iter().filter(|record| record.cache_hit).count();
    PackageSummary {
        total,
        cache_hits,
        rebuilt: total.saturating_sub(cache_hits),
    }
}

pub(super) fn read_package_outputs_by_region(
    path: &Path,
) -> anyhow::Result<BTreeMap<String, PackageOutputRecord>> {
    if !path.is_file() {
        return Ok(BTreeMap::new());
    }
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut records = BTreeMap::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let value: serde_json::Value =
            serde_json::from_str(line).context("failed to parse package output json")?;
        if value.get("event").and_then(|v| v.as_str()) != Some("package_output") {
            continue;
        }
        let record = PackageOutputRecord {
            label: value
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            chart: value
                .get("chart")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned),
            region: value
                .get("region")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            manifest: value
                .get("manifest")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            manifest_sha256: value
                .get("manifest_sha256")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            zip: value
                .get("zip")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            zip_sha256: value
                .get("zip_sha256")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            metadata: value
                .get("metadata")
                .and_then(|v| v.as_object())
                .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default(),
        };
        records.insert(record.region.clone(), record);
    }
    Ok(records)
}

pub(super) fn package_record_for_region(
    path: &Path,
    region: Region,
) -> anyhow::Result<PackageOutputRecord> {
    read_package_outputs_by_region(path)?
        .remove(&region.code().to_ascii_lowercase())
        .ok_or_else(|| anyhow::anyhow!("missing package output for region {}", region.code()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_config(root: &Path) -> ProductBuildConfig {
        let build_root = root.join("build");
        let publish_dir = build_root
            .join("published")
            .join("test")
            .join("20260602T000000Z");
        ProductBuildConfig {
            chart_metadata_root: root.join("chart-metadata"),
            build_root,
            publish_dir: publish_dir.clone(),
            packaged_dir: publish_dir.join("packaged"),
            publish_label: "test".to_string(),
            publish_timestamp: "20260602T000000Z".to_string(),
            target_cycle: Some("2605".to_string()),
            fetch_jobs: 4,
            cpu_jobs: 4,
            max_heavy_jobs: 1,
            fetch_cache_root: root.join("fetch-cache"),
            fetch_cache_mode: "cache-first".to_string(),
        }
    }

    fn write_tpp_source_urls(root: &Path) {
        for region in Region::ALL.iter() {
            let region_id = region.code().to_ascii_lowercase();
            let dir = root.join(format!("tpp-{region_id}"));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("source_urls.jsonl"), b"").unwrap();
        }
    }

    fn write_source_fetch_record(source_content_fingerprint: &str) -> NodeRecord {
        NodeRecord {
            name: "charts-enr-h-fetch".to_string(),
            fingerprint: "fetch-node-fingerprint".to_string(),
            inputs: BTreeMap::new(),
            outputs: BTreeMap::from([
                (
                    "source_root".to_string(),
                    "cache/nodes/charts-enr-h-fetch/test/source".to_string(),
                ),
                (
                    "source_content_fingerprint".to_string(),
                    source_content_fingerprint.to_string(),
                ),
            ]),
            output_details: BTreeMap::new(),
            cache_hit: true,
            started_at_utc: "2026-06-02T00:00:00Z".to_string(),
            finished_at_utc: "2026-06-02T00:00:00Z".to_string(),
            elapsed_ms: 0,
            fetch_cache_refs: Vec::new(),
        }
    }

    #[test]
    fn tpp_fetch_inputs_do_not_depend_on_tpp_render_library() {
        let temp = tempdir().unwrap();
        let config = test_config(temp.path());
        let source_urls_root = temp.path().join("source-urls");
        write_tpp_source_urls(&source_urls_root);

        let (inputs, _requests) = tpp_fetch_inputs(&config, &source_urls_root).unwrap();

        assert!(inputs.contains_key("requests"));
        assert!(inputs.contains_key("fetch_lib"));
        assert!(inputs.contains_key("tpp_fetch_node_version"));
        assert!(!inputs.contains_key("tpp_lib"));
    }

    #[test]
    fn tpp_plan_inputs_use_source_content_fingerprint() {
        let temp = tempdir().unwrap();
        let source_urls = temp.path().join("source_urls.jsonl");
        fs::write(&source_urls, b"").unwrap();

        let inputs = tpp_plan_inputs(&source_urls, "nw", 4, Some("source-content")).unwrap();

        assert_eq!(
            inputs.get("source_content_fingerprint").map(String::as_str),
            Some("source-content")
        );
        assert!(!inputs.contains_key("source_fetch_fingerprint"));
        assert!(inputs.contains_key("tpp_render_node_version"));
        assert!(!inputs.contains_key("tpp_lib"));
        assert!(!inputs.contains_key("tools_lib"));
    }

    #[test]
    fn tpp_package_inputs_do_not_depend_on_shared_tools_hash() {
        let temp = tempdir().unwrap();
        let source_urls = temp.path().join("source_urls.jsonl");
        fs::write(&source_urls, b"").unwrap();
        let asset_root = temp.path().join("assets");
        fs::create_dir_all(asset_root.join("plates")).unwrap();
        fs::write(asset_root.join("plates/test.png"), b"png").unwrap();
        let render_record = NodeRecord {
            name: "tpp-nw-render-assemble".to_string(),
            fingerprint: "render-fingerprint".to_string(),
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            output_details: BTreeMap::new(),
            cache_hit: true,
            started_at_utc: "2026-06-02T00:00:00Z".to_string(),
            finished_at_utc: "2026-06-02T00:00:00Z".to_string(),
            elapsed_ms: 0,
            fetch_cache_refs: Vec::new(),
        };
        let thumbnail = TppThumbnailPlan {
            id: "thumb".to_string(),
            asset_path: "plates/test.png".to_string(),
            thumbnail_path: "thumbnails/test.png".to_string(),
        };
        let thumbnail_record = NodeRecord {
            name: "tpp-nw-thumbnail-thumb".to_string(),
            fingerprint: "thumbnail-fingerprint".to_string(),
            inputs: BTreeMap::new(),
            outputs: BTreeMap::from([(
                "thumbnail_path".to_string(),
                "thumbnails/test.png".to_string(),
            )]),
            output_details: BTreeMap::new(),
            cache_hit: true,
            started_at_utc: "2026-06-02T00:00:00Z".to_string(),
            finished_at_utc: "2026-06-02T00:00:00Z".to_string(),
            elapsed_ms: 0,
            fetch_cache_refs: Vec::new(),
        };

        let package_plan_inputs =
            tpp_package_plan_inputs(Region::Nw, &source_urls, "2606_01", "TPP1", &render_record)
                .unwrap();
        let thumbnail_inputs =
            tpp_thumbnail_inputs(Region::Nw, &asset_root.join("plates/test.png"), &thumbnail)
                .unwrap();
        let assemble_inputs =
            tpp_package_assemble_inputs(Region::Nw, &render_record, &[thumbnail_record]).unwrap();
        let render_assemble_inputs =
            tpp_render_assemble_inputs(Region::Nw, &render_record, &[]).unwrap();

        assert!(!package_plan_inputs.contains_key("tools_lib"));
        assert!(!thumbnail_inputs.contains_key("tools_lib"));
        assert!(!assemble_inputs.contains_key("tools_lib"));
        assert!(thumbnail_inputs.contains_key("tpp_thumbnail"));
        assert!(thumbnail_inputs.contains_key("tpp_thumbnail_node_version"));
        assert!(!thumbnail_inputs.contains_key("tpp_package"));
        assert!(!thumbnail_inputs.contains_key("tpp_package_node_version"));
        assert!(render_assemble_inputs.contains_key("tpp_render_assemble_node_version"));
        assert!(!render_assemble_inputs.contains_key("tpp_render_node_version"));
    }

    #[test]
    fn chart_process_inputs_use_source_content_fingerprint() {
        let temp = tempdir().unwrap();
        let source_repo = temp.path().join("cutlines");
        let source_urls = temp.path().join("source_urls.jsonl");
        fs::create_dir_all(&source_repo).unwrap();
        fs::write(source_repo.join("cutline.geojson"), b"{}").unwrap();
        fs::write(&source_urls, b"").unwrap();

        let inputs = chart_process_inputs(
            ChartFamily::EnrH,
            &source_repo,
            &source_urls,
            "source-content",
            8,
        )
        .unwrap();

        assert_eq!(
            inputs.get("source_content_fingerprint").map(String::as_str),
            Some("source-content")
        );
        assert!(!inputs.contains_key("source_fetch_fingerprint"));
        assert!(!inputs.contains_key("tools_lib"));
    }

    #[test]
    fn chart_render_cleanup_keeps_tiles_and_legends_and_removes_source_work_files() {
        let temp = tempdir().unwrap();
        let work_dir = temp.path().join("charts-sec");
        let tile_path = work_dir.join("tiles").join("0").join("1").join("2.webp");
        let legend_path = work_dir.join("legends").join("Seattle SEC.png");
        fs::create_dir_all(tile_path.parent().unwrap()).unwrap();
        fs::create_dir_all(legend_path.parent().unwrap()).unwrap();
        fs::write(&tile_path, b"tile").unwrap();
        fs::write(&legend_path, b"legend").unwrap();
        fs::write(work_dir.join("Seattle SEC.tif"), b"tiff").unwrap();
        fs::write(work_dir.join("Seattle.zip"), b"zip").unwrap();
        fs::write(work_dir.join("Seattle.vrt"), b"vrt").unwrap();
        fs::create_dir_all(work_dir.join(".rust-logs")).unwrap();
        fs::write(work_dir.join(".rust-logs").join("gdal2tiles.log"), b"log").unwrap();

        prune_chart_render_intermediates(&work_dir).unwrap();

        assert!(tile_path.exists());
        assert!(work_dir.join("tiles").exists());
        assert!(legend_path.exists());
        assert!(work_dir.join("legends").exists());
        assert!(!work_dir.join("Seattle SEC.tif").exists());
        assert!(!work_dir.join("Seattle.zip").exists());
        assert!(!work_dir.join("Seattle.vrt").exists());
        assert!(!work_dir.join(".rust-logs").exists());
    }

    #[test]
    fn csup_render_inputs_use_narrow_tool_hashes() {
        let inputs = csup_render_inputs("process", Region::Nw, 4, "2607").unwrap();

        assert_eq!(
            inputs.get("process_fingerprint").map(String::as_str),
            Some("process")
        );
        assert!(inputs.contains_key("png_tools"));
        assert!(inputs.contains_key("tool_invocation"));
        assert!(!inputs.contains_key("tools_lib"));
    }

    #[test]
    fn csup_process_inputs_use_source_content_fingerprint() {
        let temp = tempdir().unwrap();
        let source_urls = temp.path().join("source_urls.jsonl");
        fs::write(&source_urls, b"").unwrap();

        let inputs = csup_process_inputs(&source_urls, "source-content").unwrap();

        assert_eq!(
            inputs.get("source_content_fingerprint").map(String::as_str),
            Some("source-content")
        );
        assert!(!inputs.contains_key("source_fetch_fingerprint"));
    }

    #[test]
    fn source_content_fingerprint_reads_fetch_output() {
        let record = write_source_fetch_record("source-content");

        assert_eq!(
            source_content_fingerprint(&record).unwrap(),
            "source-content"
        );
    }
}
