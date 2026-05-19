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

pub(super) fn build_chart_render_node(
    config: &ProductBuildConfig,
    family: ChartFamily,
    source_repo: &Path,
    source_urls: &Path,
    fetch_jobs: usize,
    cpu_jobs: usize,
) -> anyhow::Result<NodeRecord> {
    let family_id = family_slug(family).to_string();
    let node_name = format!("charts-{family_id}-render");
    let inputs = chart_render_inputs(family, source_repo, source_urls, fetch_jobs, cpu_jobs)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &node_name)?,
        &node_name,
        &inputs,
    )?;
    let work_dir = prepared.dir.join("work").join(family.capture_label());
    let tiles_root = work_dir.join("tiles");
    run_cached_node(prepared, inputs, &[tiles_root.clone()], |prepared| {
        let work_dir = stage_work_dir(family, source_repo, &prepared.dir)?;
        let provenance_dir = prepared
            .dir
            .join("meta")
            .join("provenance")
            .join(format!("charts-{family_id}"));
        fs::create_dir_all(&provenance_dir)?;
        copy_source_urls_provenance(source_urls, &provenance_dir)?;
        let requests = read_source_prefetch_requests_jsonl(source_urls)?;
        prefetch_archives_with_provenance(
            &requests,
            &work_dir,
            fetch_jobs,
            Some(&static_source_fetch_cache_config(config)?),
            &provenance_dir,
            family.capture_label(),
        )?;
        build_family_vrts(family, &work_dir, cpu_jobs)?;
        build_family_tiles(family, &work_dir, cpu_jobs)?;
        Ok(BTreeMap::from([
            (
                "work_dir".to_string(),
                relative_artifact_path(&work_dir, &config.build_root),
            ),
            (
                "tiles_root".to_string(),
                relative_artifact_path(&tiles_root, &config.build_root),
            ),
        ]))
    })
}

pub(super) fn build_chart_package_nodes(
    config: &ProductBuildConfig,
    family: ChartFamily,
    source_urls_dir: &Path,
    version_label: &str,
) -> anyhow::Result<(Vec<NodeRecord>, ChartSource)> {
    let family_id = family_slug(family).to_string();
    let source_urls_path = source_urls_dir.join(format!("charts-{family_id}/source_urls.jsonl"));
    let render_node_name = format!("charts-{family_id}-render");
    let render_inputs = chart_render_inputs(
        family,
        &config.chart_cutline_root,
        &source_urls_path,
        config.fetch_jobs,
        config.cpu_jobs.min(8).max(1),
    )?;
    let render_prepared = prepare_node_at(
        &build_shared_node_dir(config, &render_node_name)?,
        &render_node_name,
        &render_inputs,
    )?;
    let render_record = load_existing_node_record(&render_prepared.record_path, &render_node_name)?;
    let work_dir = resolve_artifact_path(config, output_path(&render_record, "work_dir")?);
    let node_name = format!("charts-{family_id}-package");
    let inputs = BTreeMap::from([
        (
            "render_fingerprint".to_string(),
            render_record.fingerprint.clone(),
        ),
        (
            "package_node_contract".to_string(),
            "unpack-source-root-v1".to_string(),
        ),
        ("version_label".to_string(), version_label.to_string()),
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
        version_label
    ));
    let wide_zip_path = package_root.join(format!(
        "WIDE_{}_{}.zip",
        manifest_chart_name(family),
        version_label
    ));
    let mut expected_outputs = Vec::from([
        aggregate_path.clone(),
        unpack_source_root.clone(),
        wide_zip_path.clone(),
        wide_manifest_path.clone(),
    ]);
    for region in Region::ALL {
        expected_outputs.push(package_root.join(format!(
            "{}_{}_{}.zip",
            region.code(),
            manifest_chart_name(family),
            version_label
        )));
        expected_outputs.push(package_root.join(format!(
            "{}_{}_{}.manifest",
            region.code(),
            manifest_chart_name(family),
            version_label
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
            for region in Region::ALL {
                let record = package_family_region_versioned_to(
                    family,
                    &work_dir,
                    &package_root,
                    region,
                    version_label,
                    version_label,
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
                version_label,
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
    stage_fingerprint: &str,
    version_label: &str,
    render_jobs: usize,
) -> anyhow::Result<NodeRecord> {
    let node_name = format!("csup-render-{}", region.code().to_ascii_lowercase());
    let inputs = csup_render_inputs(stage_fingerprint, region, render_jobs, version_label)?;
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

pub(super) fn build_csup_stage_node(
    config: &ProductBuildConfig,
    source_repo: &Path,
    source_urls: &Path,
    fetch_jobs: usize,
) -> anyhow::Result<NodeRecord> {
    let inputs = csup_stage_inputs(source_urls, fetch_jobs)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "csup-stage")?,
        "csup-stage",
        &inputs,
    )?;
    let work_root = prepared.dir.clone();
    let marker = work_root.join(".stage-complete");
    run_cached_node(
        prepared,
        inputs,
        std::slice::from_ref(&marker),
        |_prepared| {
            let work_dir = stage_work_dir_for_product(source_repo, &work_root)?;
            let provenance_dir = work_root.join("meta").join("provenance").join("csup");
            fs::create_dir_all(&provenance_dir)?;
            copy_source_urls_provenance(source_urls, &provenance_dir)?;
            let requests = read_source_prefetch_requests_jsonl(source_urls)?;
            prefetch_archives_with_provenance(
                &requests,
                &work_dir,
                fetch_jobs,
                Some(&static_source_fetch_cache_config(config)?),
                &provenance_dir,
                "csup",
            )?;
            prepare_csup_inputs(&work_dir)?;
            fs::write(&marker, b"ok")
                .with_context(|| format!("failed to write {}", marker.display()))?;
            Ok(BTreeMap::from([
                (
                    "work_dir".to_string(),
                    relative_artifact_path(&work_dir, &config.build_root),
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
        },
    )
}

pub(super) fn build_csup_package_nodes(
    config: &ProductBuildConfig,
    source_urls_dir: &Path,
    version_label: &str,
) -> anyhow::Result<(Vec<NodeRecord>, AssetSource)> {
    let source_urls_path = source_urls_dir.join("csup/source_urls.jsonl");
    let stage_inputs = csup_stage_inputs(&source_urls_path, config.fetch_jobs)?;
    let stage_prepared = prepare_node_at(
        &build_shared_node_dir(config, "csup-stage")?,
        "csup-stage",
        &stage_inputs,
    )?;
    let stage_record = load_existing_node_record(&stage_prepared.record_path, "csup-stage")?;
    let work_dir = resolve_artifact_path(config, output_path(&stage_record, "work_dir")?);
    let mut inputs = BTreeMap::from([
        (
            "stage_fingerprint".to_string(),
            stage_record.fingerprint.clone(),
        ),
        (
            "package_node_contract".to_string(),
            "unpack-source-root-v1".to_string(),
        ),
        ("version_label".to_string(), version_label.to_string()),
        (
            "csup_package".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-csup/src/package.rs"),
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
    for region in Region::ALL {
        let render_node_name = format!("csup-render-{}", region.code().to_ascii_lowercase());
        let render_inputs = csup_render_inputs(
            &stage_record.fingerprint,
            region,
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
    for region in Region::ALL {
        expected_outputs.push(package_root.join(format!(
            "{}_CSUP_{}.zip",
            region.code(),
            version_label
        )));
        expected_outputs.push(package_root.join(format!(
            "{}_CSUP_{}.manifest",
            region.code(),
            version_label
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
            for region in Region::ALL {
                package_records.push(package_csup_region_versioned_to(
                    &work_dir,
                    &package_root,
                    region,
                    version_label,
                    version_label,
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

pub(super) fn build_tpp_render_node(
    config: &ProductBuildConfig,
    request: &NativeTppRunRequest,
) -> anyhow::Result<NodeRecord> {
    let region_id = request.region.code().to_ascii_lowercase();
    let source_urls = request
        .prefetch_source_urls
        .as_ref()
        .context("tpp build requires source urls")?;
    let node_name = format!("tpp-{region_id}-render");
    let inputs = tpp_render_inputs(request, source_urls, &region_id)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &node_name)?,
        &node_name,
        &inputs,
    )?;
    let run_root = prepared.dir.clone();
    let plates_root = run_root.join(format!("work/tpp-{region_id}/plates"));
    run_cached_node(
        prepared,
        inputs,
        std::slice::from_ref(&plates_root),
        |_prepared| {
            let mut request = request.clone();
            request.run_root = run_root;
            let result = render_native_tpp(&request)?;
            Ok(BTreeMap::from([
                (
                    "work_dir".to_string(),
                    relative_artifact_path(&result.work_dir, &config.build_root),
                ),
                (
                    "provenance_dir".to_string(),
                    relative_artifact_path(&result.provenance_dir, &config.build_root),
                ),
                (
                    "plates_root".to_string(),
                    relative_artifact_path(&plates_root, &config.build_root),
                ),
            ]))
        },
    )
}

pub(super) fn build_tpp_package_node(
    config: &ProductBuildConfig,
    region: Region,
    source_urls_path: &Path,
    version_label: &str,
) -> anyhow::Result<(NodeRecord, AssetSource)> {
    let region_id = region.code().to_ascii_lowercase();
    let render_request = NativeTppRunRequest {
        region,
        source_repo: PathBuf::new(),
        run_root: PathBuf::new(),
        prefetch_source_urls: Some(source_urls_path.to_path_buf()),
        fetch_jobs: config.fetch_jobs,
        render_jobs: TPP_RENDER_JOBS_PER_RUN,
        fetch_cache: Some(static_source_fetch_cache_config(config)?),
    };
    let render_node_name = format!("tpp-{region_id}-render");
    let render_inputs = tpp_render_inputs(&render_request, source_urls_path, &region_id)?;
    let render_prepared = prepare_node_at(
        &build_shared_node_dir(config, &render_node_name)?,
        &render_node_name,
        &render_inputs,
    )?;
    let render_record = load_existing_node_record(&render_prepared.record_path, &render_node_name)?;
    let asset_root = resolve_artifact_path(config, output_path(&render_record, "work_dir")?);
    let inputs = BTreeMap::from([
        (
            "render_fingerprint".to_string(),
            render_record.fingerprint.clone(),
        ),
        (
            "package_node_contract".to_string(),
            "unpack-source-root-v1".to_string(),
        ),
        ("region".to_string(), region.code().to_string()),
        ("version_label".to_string(), version_label.to_string()),
        (
            "cache_layout_version".to_string(),
            TPP_CACHE_LAYOUT_VERSION.to_string(),
        ),
        (
            "tpp_package".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-tpp/src/package.rs"),
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
    let zip_path = package_root.join(format!("{}_TPP_{}.zip", region.code(), version_label));
    let manifest_path =
        package_root.join(format!("{}_TPP_{}.manifest", region.code(), version_label));
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
                    asset_root: asset_root.clone(),
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
    if unpack_source_root.exists() {
        fs::remove_dir_all(&unpack_source_root)
            .with_context(|| format!("failed to remove {}", unpack_source_root.display()))?;
    }
    let result = package_native_tpp_versioned(
        &asset_root,
        &package_root,
        &provenance_dir,
        region,
        version_label,
        version_label,
    )?;
    prepare_package_unpack_source_root(
        std::slice::from_ref(&zip_path),
        &asset_root,
        &package_root,
        &unpack_source_root,
        &["thumbnails/"],
    )?;
    let outputs = BTreeMap::from([
        (
            "asset_root".to_string(),
            relative_artifact_path(&asset_root, &config.build_root),
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
        (
            "package_count".to_string(),
            result.package_count.to_string(),
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
    Ok((
        record,
        AssetSource {
            package_outputs_path,
            asset_root,
            package_root,
            unpack_source_root,
            source_urls_path: Some(source_urls_path.to_path_buf()),
        },
    ))
}

pub(super) fn chart_render_inputs(
    family: ChartFamily,
    source_repo: &Path,
    source_urls: &Path,
    fetch_jobs: usize,
    cpu_jobs: usize,
) -> anyhow::Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        ("family".to_string(), family_slug(family).to_string()),
        ("source_repo".to_string(), hash_tree(source_repo)?),
        ("source_urls".to_string(), hash_file(source_urls)?),
        ("cpu_jobs".to_string(), cpu_jobs.to_string()),
        ("fetch_jobs".to_string(), fetch_jobs.to_string()),
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

pub(super) fn csup_stage_inputs(
    source_urls: &Path,
    fetch_jobs: usize,
) -> anyhow::Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        ("source_urls".to_string(), hash_file(source_urls)?),
        ("fetch_jobs".to_string(), fetch_jobs.to_string()),
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
            "tools_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-tools/src/lib.rs"),
            )?,
        ),
    ]))
}

pub(super) fn csup_render_inputs(
    stage_fingerprint: &str,
    region: Region,
    render_jobs: usize,
    version_label: &str,
) -> anyhow::Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        (
            "stage_fingerprint".to_string(),
            stage_fingerprint.to_string(),
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
            "tools_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-tools/src/lib.rs"),
            )?,
        ),
    ]))
}

pub(super) fn tpp_render_inputs(
    request: &NativeTppRunRequest,
    source_urls: &Path,
    region_id: &str,
) -> anyhow::Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        ("region".to_string(), region_id.to_string()),
        ("source_urls".to_string(), hash_file(source_urls)?),
        ("fetch_jobs".to_string(), request.fetch_jobs.to_string()),
        (
            "cache_layout_version".to_string(),
            TPP_CACHE_LAYOUT_VERSION.to_string(),
        ),
        (
            "tpp_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-tpp/src/lib.rs"),
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
    ]))
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
    let node_root = build_node_root(config, "resource-index")?;
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
