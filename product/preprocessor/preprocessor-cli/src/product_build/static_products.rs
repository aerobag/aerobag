use super::*;

pub(super) fn build_terrain_product(
    config: &ProductBuildConfig,
    region: Region,
    terrain_index_path: &Path,
    source_fetched_at_utc: Option<String>,
    geoid_csv_path: &Path,
    geoid_metadata_path: &Path,
    geoid_source_fetched_at_utc: Option<String>,
) -> anyhow::Result<(PathBuf, String, Option<String>, NodeRecord)> {
    let region_id = region.code().to_ascii_lowercase();
    let input_dir = config
        .build_root
        .join("private-work")
        .join("terrain")
        .join(&region_id)
        .join("input");
    let dem_dir = input_dir.join("dems");
    fs::create_dir_all(&dem_dir)
        .with_context(|| format!("failed to create {}", dem_dir.display()))?;

    let provenance_dir = config
        .build_root
        .join("meta")
        .join("provenance")
        .join(format!("terrain-{region_id}"));
    let fetch_cache = terrain_fetch_cache_config(config)?;
    let mut dem_candidates = terrain_dem_candidates_for_region(terrain_index_path, region)?;
    if dem_candidates.is_empty() {
        bail!(
            "terrain discovery index has no DEM URLs for {}",
            region.code()
        );
    }
    if let Some(cached_selection) = cached_terrain_dem_selection(&dem_candidates, &fetch_cache)? {
        let dem_source_fingerprint = terrain_source_fingerprint_from_cached(
            &cached_selection.selection.urls,
            &cached_selection.sources,
            &cached_selection.selection.missing_cells,
            Some((geoid_csv_path, geoid_metadata_path)),
        )?;
        let source_fingerprint = terrain_output_fingerprint(&dem_source_fingerprint);
        let version_label = content_product_version_label(&source_fingerprint);
        let inputs = terrain_product_inputs(
            region,
            &source_fingerprint,
            geoid_csv_path,
            geoid_metadata_path,
        )?;
        let prepared = prepare_node_at(
            &build_shared_node_dir(config, &format!("static-terrain-{region_id}"))?,
            &format!("static-terrain-{region_id}"),
            &inputs,
        )?;
        let output_dir = prepared.dir.join("output");
        let zip_path = output_dir.join(format!("terrain_{region_id}_{version_label}.zip"));
        let manifest_path = output_dir.join("manifest.json");
        if let Some(record) = try_load_node_record(&prepared, &[zip_path.clone(), manifest_path])? {
            return Ok((
                zip_path,
                version_label,
                max_optional_utc(source_fetched_at_utc, geoid_source_fetched_at_utc),
                record,
            ));
        }
    }
    let dem_selection = prefetch_terrain_dems_with_fallback(
        &mut dem_candidates,
        &dem_dir,
        config.fetch_jobs,
        &fetch_cache,
        &provenance_dir,
        &format!("terrain-{region_id}-dem"),
    )?;
    let dem_paths = terrain_dem_paths_from_requests(&dem_dir, &dem_selection.requests)?;
    let dem_source_fingerprint = if let Some(sources) =
        cached_terrain_dem_sources_for_requests(&fetch_cache, &dem_selection.requests)?
    {
        terrain_source_fingerprint_from_cached(
            &dem_selection.urls,
            &sources,
            &dem_selection.missing_cells,
            Some((geoid_csv_path, geoid_metadata_path)),
        )?
    } else {
        terrain_source_fingerprint(
            &dem_selection.urls,
            &dem_paths,
            &dem_selection.missing_cells,
            Some((geoid_csv_path, geoid_metadata_path)),
        )?
    };
    let source_fingerprint = terrain_output_fingerprint(&dem_source_fingerprint);
    let version_label = content_product_version_label(&source_fingerprint);
    let inputs = terrain_product_inputs(
        region,
        &source_fingerprint,
        geoid_csv_path,
        geoid_metadata_path,
    )?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &format!("static-terrain-{region_id}"))?,
        &format!("static-terrain-{region_id}"),
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let zip_path = output_dir.join(format!("terrain_{region_id}_{version_label}.zip"));
    let manifest_path = output_dir.join("manifest.json");
    let _build_lock = match claim_or_wait_for_node(&prepared, &[zip_path.clone(), manifest_path])? {
        NodeCacheState::CacheHit(record) => {
            return Ok((
                zip_path,
                version_label,
                max_optional_utc(source_fetched_at_utc, geoid_source_fetched_at_utc),
                record,
            ));
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
    let vrt_path = output_dir.join(format!("terrain_{region_id}.vrt"));
    build_terrain_vrt(&vrt_path, &dem_paths)?;
    build_terrain_region_tiles(
        region,
        &vrt_path,
        geoid_csv_path,
        geoid_metadata_path,
        &output_dir,
        &version_label,
        &dem_selection,
    )?;
    zip_directory_deterministic(&zip_path, &output_dir, &["manifest.json", "tiles"])?;
    let outputs = BTreeMap::from([
        (
            "manifest".to_string(),
            relative_artifact_path(&output_dir.join("manifest.json"), &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&zip_path, &config.build_root),
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
        zip_path,
        version_label,
        max_optional_utc(source_fetched_at_utc, geoid_source_fetched_at_utc),
        record,
    ))
}

pub(super) fn build_terrain_wide_product(
    config: &ProductBuildConfig,
    regional_products: &[(String, PathBuf, String, String, Option<String>)],
) -> anyhow::Result<(PathBuf, String, Option<String>, NodeRecord)> {
    let source_fingerprint = terrain_wide_source_fingerprint(regional_products);
    let version_label = content_product_version_label(&source_fingerprint);
    let source_fetched_at_utc = regional_products
        .iter()
        .filter_map(
            |(_region_id, _output_dir, _source_version, _zip_sha256, fetched_at)| {
                fetched_at.clone()
            },
        )
        .max();
    let inputs = terrain_wide_product_inputs(regional_products, &source_fingerprint);
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &format!("static-terrain-{WIDE_ANGLE_REGION_ID}"))?,
        &format!("static-terrain-{WIDE_ANGLE_REGION_ID}"),
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let manifest_path = output_dir.join("manifest.json");
    let zip_path = output_dir.join(format!(
        "terrain_{WIDE_ANGLE_REGION_ID}_{version_label}.zip"
    ));
    if let Some(record) =
        try_load_node_record(&prepared, &[zip_path.clone(), manifest_path.clone()])?
    {
        return Ok((zip_path, version_label, source_fetched_at_utc, record));
    }
    let _build_lock =
        match claim_or_wait_for_node(&prepared, &[zip_path.clone(), manifest_path.clone()])? {
            NodeCacheState::CacheHit(record) => {
                return Ok((zip_path, version_label, source_fetched_at_utc, record));
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
    composite_terrain_wide_tiles(
        regional_products,
        &output_dir,
        &version_label,
        &source_fingerprint,
    )?;
    zip_directory_deterministic(&zip_path, &output_dir, &["manifest.json", "tiles"])?;
    let outputs = BTreeMap::from([
        (
            "manifest".to_string(),
            relative_artifact_path(&manifest_path, &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&zip_path, &config.build_root),
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
    Ok((zip_path, version_label, source_fetched_at_utc, record))
}

pub(super) fn terrain_wide_source_fingerprint(
    regional_products: &[(String, PathBuf, String, String, Option<String>)],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"terrain-wide-v2-ter2");
    hasher.update(TERRAIN_CONTRACT_ID.as_bytes());
    hasher.update([0]);
    hasher.update(TERRAIN_PIPELINE_VERSION.as_bytes());
    hasher.update([0]);
    hasher.update(TERRAIN_TER2_HEIGHT_QUANTIZATION_FT.to_string().as_bytes());
    hasher.update([0]);
    hasher.update(FULL_COVERAGE_ZOOM.to_string().as_bytes());
    for (region_id, _output_dir, source_version, zip_sha256, _fetched_at) in regional_products {
        hasher.update(region_id.as_bytes());
        hasher.update([0]);
        hasher.update(source_version.as_bytes());
        hasher.update([0]);
        hasher.update(zip_sha256.as_bytes());
        hasher.update([0xff]);
    }
    format!("{:x}", hasher.finalize())
}

pub(super) fn terrain_output_fingerprint(dem_source_fingerprint: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"terrain-output-v1-ter2");
    hasher.update(TERRAIN_CONTRACT_ID.as_bytes());
    hasher.update([0]);
    hasher.update(TERRAIN_PIPELINE_VERSION.as_bytes());
    hasher.update([0]);
    hasher.update(TERRAIN_ZOOM.to_string().as_bytes());
    hasher.update([0]);
    hasher.update(TERRAIN_TILE_SIZE.to_string().as_bytes());
    hasher.update([0]);
    hasher.update(TERRAIN_TER2_HEIGHT_QUANTIZATION_FT.to_string().as_bytes());
    hasher.update([0]);
    hasher.update(dem_source_fingerprint.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(super) fn terrain_wide_product_inputs(
    regional_products: &[(String, PathBuf, String, String, Option<String>)],
    source_fingerprint: &str,
) -> BTreeMap<String, String> {
    let region_versions = regional_products
        .iter()
        .map(
            |(region_id, _output_dir, source_version, zip_sha256, _fetched_at)| {
                format!("{region_id}:{source_version}:{zip_sha256}")
            },
        )
        .collect::<Vec<_>>()
        .join(",");
    BTreeMap::from([
        (
            "product_id".to_string(),
            format!("terrain-{WIDE_ANGLE_REGION_ID}"),
        ),
        (
            "wide_angle_max_zoom".to_string(),
            FULL_COVERAGE_ZOOM.to_string(),
        ),
        ("tile_size".to_string(), TERRAIN_TILE_SIZE.to_string()),
        (
            "terrain_contract_id".to_string(),
            TERRAIN_CONTRACT_ID.to_string(),
        ),
        (
            "height_quantization_ft".to_string(),
            TERRAIN_TER2_HEIGHT_QUANTIZATION_FT.to_string(),
        ),
        (
            "source_fingerprint".to_string(),
            source_fingerprint.to_string(),
        ),
        ("region_versions".to_string(), region_versions),
        (
            "terrain_pipeline".to_string(),
            TERRAIN_PIPELINE_VERSION.to_string(),
        ),
        (
            "wide_angle_script".to_string(),
            hash_text(TERRAIN_WIDE_TILE_SCRIPT),
        ),
    ])
}

pub(super) fn composite_terrain_wide_tiles(
    regional_products: &[(String, PathBuf, String, String, Option<String>)],
    output_dir: &Path,
    version_label: &str,
    source_fingerprint: &str,
) -> anyhow::Result<()> {
    let script_path = output_dir.join("build_terrain_wide_tiles.py");
    fs::write(&script_path, TERRAIN_WIDE_TILE_SCRIPT)
        .with_context(|| format!("failed to write {}", script_path.display()))?;
    let mut command = Command::new("python3");
    command
        .arg(&script_path)
        .arg("--output-dir")
        .arg(output_dir)
        .arg("--version-label")
        .arg(version_label)
        .arg("--source-fingerprint")
        .arg(source_fingerprint)
        .arg("--max-zoom")
        .arg(FULL_COVERAGE_ZOOM.to_string())
        .arg("--tile-size")
        .arg(TERRAIN_TILE_SIZE.to_string())
        .arg("--height-quantization-ft")
        .arg(TERRAIN_TER2_HEIGHT_QUANTIZATION_FT.to_string());
    for (_region_id, output_dir, _source_version, _zip_sha256, _fetched_at) in regional_products {
        command.arg("--source-dir").arg(output_dir);
    }
    let output = command
        .output()
        .with_context(|| format!("failed to run {}", script_path.display()))?;
    if !output.status.success() {
        bail!(
            "terrain wide tile builder failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::remove_file(&script_path)
        .with_context(|| format!("failed to remove {}", script_path.display()))?;
    Ok(())
}

pub(super) fn terrain_product_inputs(
    region: Region,
    source_fingerprint: &str,
    geoid_csv_path: &Path,
    geoid_metadata_path: &Path,
) -> anyhow::Result<BTreeMap<String, String>> {
    let region_id = region.code().to_ascii_lowercase();
    Ok(BTreeMap::from([
        ("product_id".to_string(), format!("terrain-{region_id}")),
        (
            "terrain_contract_id".to_string(),
            TERRAIN_CONTRACT_ID.to_string(),
        ),
        ("region".to_string(), region.code().to_string()),
        ("min_zoom".to_string(), TERRAIN_MIN_ZOOM.to_string()),
        ("max_zoom".to_string(), TERRAIN_ZOOM.to_string()),
        ("tile_size".to_string(), TERRAIN_TILE_SIZE.to_string()),
        (
            "height_quantization_ft".to_string(),
            TERRAIN_TER2_HEIGHT_QUANTIZATION_FT.to_string(),
        ),
        (
            "source_fingerprint".to_string(),
            source_fingerprint.to_string(),
        ),
        (
            "terrain_pipeline".to_string(),
            TERRAIN_PIPELINE_VERSION.to_string(),
        ),
        ("geoid_csv".to_string(), hash_file(geoid_csv_path)?),
        (
            "geoid_metadata".to_string(),
            hash_file(geoid_metadata_path)?,
        ),
    ]))
}

pub(super) fn build_water_mask_product(
    config: &ProductBuildConfig,
    region: Region,
) -> anyhow::Result<(PathBuf, PathBuf, String, Option<String>, NodeRecord)> {
    let region_id = region.code().to_ascii_lowercase();
    let inputs = water_mask_product_inputs(region)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &format!("static-water-mask-{region_id}"))?,
        &format!("static-water-mask-{region_id}"),
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let manifest_path = output_dir.join("manifest.json");
    if let Some(record) = try_load_node_record(&prepared, &[manifest_path.clone()])? {
        let (source_version, source_fetched_at_utc) = water_mask_manifest_versions(&manifest_path)?;
        let zip_path = water_mask_record_zip_path(&prepared.dir, &record)?;
        return Ok((
            zip_path,
            output_dir.join("tiles"),
            source_version,
            source_fetched_at_utc,
            record,
        ));
    }
    let _build_lock = match claim_or_wait_for_node(&prepared, &[manifest_path.clone()])? {
        NodeCacheState::CacheHit(record) => {
            let (source_version, source_fetched_at_utc) =
                water_mask_manifest_versions(&manifest_path)?;
            let zip_path = water_mask_record_zip_path(&prepared.dir, &record)?;
            return Ok((
                zip_path,
                output_dir.join("tiles"),
                source_version,
                source_fetched_at_utc,
                record,
            ));
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
    let source_dir = water_mask_cached_source_dir(config, region, &output_dir)?;
    build_water_mask_region_tiles(region, &output_dir, &source_dir)?;
    let (source_version, source_fetched_at_utc) = water_mask_manifest_versions(&manifest_path)?;
    let zip_path = output_dir.join(format!(
        "water_mask_{region_id}_{}.zip",
        content_product_version_label(&source_version)
    ));
    zip_directory_deterministic(&zip_path, &output_dir, &["manifest.json", "tiles"])?;
    let outputs = BTreeMap::from([
        (
            "manifest".to_string(),
            relative_artifact_path(&manifest_path, &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&zip_path, &config.build_root),
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
        zip_path,
        output_dir.join("tiles"),
        source_version,
        source_fetched_at_utc,
        record,
    ))
}

pub(super) fn water_mask_record_zip_path(
    node_dir: &Path,
    record: &NodeRecord,
) -> anyhow::Result<PathBuf> {
    let value = record
        .outputs
        .get("zip")
        .context("water mask node record missing zip output")?;
    resolve_recorded_output_path(node_dir, value)
        .with_context(|| format!("failed to resolve water mask zip output {value}"))
}

pub(super) fn water_mask_manifest_versions(
    path: &Path,
) -> anyhow::Result<(String, Option<String>)> {
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))?;
    let source_fingerprint = value
        .get("source_fingerprint")
        .and_then(|value| value.as_str())
        .context("water mask manifest missing source_fingerprint")?
        .to_string();
    let source_fetched_at_utc = value
        .get("source_fetched_at_utc")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);
    Ok((source_fingerprint, source_fetched_at_utc))
}

pub(super) fn water_mask_product_inputs(
    region: Region,
) -> anyhow::Result<BTreeMap<String, String>> {
    let region_id = region.code().to_ascii_lowercase();
    Ok(BTreeMap::from([
        ("product_id".to_string(), format!("water-mask-{region_id}")),
        ("region".to_string(), region.code().to_string()),
        ("min_zoom".to_string(), TERRAIN_MIN_ZOOM.to_string()),
        ("max_zoom".to_string(), TERRAIN_ZOOM.to_string()),
        ("tile_size".to_string(), TERRAIN_TILE_SIZE.to_string()),
        (
            "water_mask_pipeline".to_string(),
            WATER_MASK_PIPELINE_VERSION.to_string(),
        ),
        (
            "water_mask_source_fetch".to_string(),
            format!(
                "nhd-object-ids-v1-precision-6-page-size-{}-fetch-workers-{}-layers-{}",
                WATER_MASK_PAGE_SIZE,
                WATER_MASK_FETCH_WORKERS,
                WATER_MASK_NHD_LAYERS
                    .iter()
                    .map(|(layer, _name, where_clause)| format!("{layer}:{where_clause}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        ),
        (
            "water_mask_script".to_string(),
            hash_file(water_mask_tile_script_path())?,
        ),
    ]))
}

pub(super) fn build_world_basemap_product(
    config: &ProductBuildConfig,
) -> anyhow::Result<(
    PathBuf,
    String,
    Option<String>,
    Vec<TileLevelRecord>,
    NodeRecord,
)> {
    let sources = build_world_basemap_source_node(config)?;
    let source_fingerprint = sources.source_fingerprint.clone();
    let source_fetched_at_utc = sources.source_fetched_at_utc.clone();
    let version_label = content_product_version_label(&source_fingerprint);
    let inputs = world_basemap_product_inputs(&source_fingerprint)?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "static-world-basemap")?,
        "static-world-basemap",
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let manifest_path = output_dir.join("manifest.json");
    let zip_path = output_dir.join(format!("world_basemap_{version_label}.zip"));
    if let Some(record) =
        try_load_node_record(&prepared, &[zip_path.clone(), manifest_path.clone()])?
    {
        let tile_levels = read_static_tile_manifest_levels(&manifest_path)?;
        return Ok((
            zip_path,
            version_label,
            source_fetched_at_utc,
            tile_levels,
            record,
        ));
    }
    let _build_lock =
        match claim_or_wait_for_node(&prepared, &[zip_path.clone(), manifest_path.clone()])? {
            NodeCacheState::CacheHit(record) => {
                let tile_levels = read_static_tile_manifest_levels(&manifest_path)?;
                return Ok((
                    zip_path,
                    version_label,
                    source_fetched_at_utc,
                    tile_levels,
                    record,
                ));
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
    build_world_basemap_tiles(
        &sources.land_shp,
        &sources.boundaries_shp,
        &output_dir,
        &version_label,
        source_fetched_at_utc.as_deref(),
    )?;
    let tile_levels = read_static_tile_manifest_levels(&manifest_path)?;
    zip_directory_deterministic(&zip_path, &output_dir, &["manifest.json", "tiles"])?;
    let outputs = BTreeMap::from([
        (
            "manifest".to_string(),
            relative_artifact_path(&manifest_path, &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&zip_path, &config.build_root),
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
        zip_path,
        version_label,
        source_fetched_at_utc,
        tile_levels,
        record,
    ))
}

pub(super) struct WorldBasemapSources {
    land_shp: PathBuf,
    boundaries_shp: PathBuf,
    source_fingerprint: String,
    source_fetched_at_utc: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct CachedSourceManifest {
    source_fingerprint: String,
    source_fetched_at_utc: Option<String>,
}

pub(super) fn build_world_basemap_source_node(
    config: &ProductBuildConfig,
) -> anyhow::Result<WorldBasemapSources> {
    let inputs = BTreeMap::from([
        ("land_url".to_string(), WORLD_BASEMAP_LAND_URL.to_string()),
        (
            "boundaries_url".to_string(),
            WORLD_BASEMAP_BOUNDARIES_URL.to_string(),
        ),
        ("fetch_jobs".to_string(), config.fetch_jobs.to_string()),
        (
            "fetch_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-fetch/src/lib.rs"),
            )?,
        ),
        (
            "source_node_version".to_string(),
            "world-basemap-sources-v1".to_string(),
        ),
    ]);
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "static-world-basemap-sources")?,
        "static-world-basemap-sources",
        &inputs,
    )?;
    let input_dir = prepared.dir.join("input");
    let manifest_path = prepared.dir.join("source-manifest.json");
    let land_shp = input_dir.join("ne_110m_land.shp");
    let boundaries_shp = input_dir.join("ne_110m_admin_0_boundary_lines_land.shp");
    let expected_outputs = vec![
        manifest_path.clone(),
        land_shp.clone(),
        boundaries_shp.clone(),
    ];
    let _build_lock = match claim_or_wait_for_node(&prepared, &expected_outputs)? {
        NodeCacheState::CacheHit(_) => {
            let manifest = read_cached_source_manifest(&manifest_path)?;
            return Ok(WorldBasemapSources {
                land_shp,
                boundaries_shp,
                source_fingerprint: manifest.source_fingerprint,
                source_fetched_at_utc: manifest.source_fetched_at_utc,
            });
        }
        NodeCacheState::Build(lock) => lock,
    };

    let started_at_utc = utc_now_string();
    let started = Instant::now();
    fs::create_dir_all(&input_dir)
        .with_context(|| format!("failed to create {}", input_dir.display()))?;
    let provenance_dir = prepared.dir.join("meta/provenance/world-basemap");
    fs::create_dir_all(&provenance_dir)?;
    let fetch_cache = static_source_fetch_cache_config(config)?;
    let requests = [
        PrefetchRequest::new(WORLD_BASEMAP_LAND_URL).with_logical_file_name("ne_110m_land.zip"),
        PrefetchRequest::new(WORLD_BASEMAP_BOUNDARIES_URL)
            .with_logical_file_name("ne_110m_admin_0_boundary_lines_land.zip"),
    ];
    prefetch_requests_with_provenance(
        &requests,
        &input_dir,
        config.fetch_jobs,
        Some(&fetch_cache),
        &provenance_dir,
        "world-basemap",
    )?;
    if !land_shp.is_file() {
        bail!("world basemap fetch missing {}", land_shp.display());
    }
    if !boundaries_shp.is_file() {
        bail!("world basemap fetch missing {}", boundaries_shp.display());
    }
    let manifest = CachedSourceManifest {
        source_fingerprint: world_basemap_source_fingerprint(&land_shp, &boundaries_shp)?,
        source_fetched_at_utc: source_fetched_at_utc_for_urls(
            &fetch_cache,
            &[WORLD_BASEMAP_LAND_URL, WORLD_BASEMAP_BOUNDARIES_URL],
        )?,
    };
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).context("failed to encode source manifest")?,
    )
    .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    let outputs = BTreeMap::from([
        (
            "source_manifest".to_string(),
            relative_artifact_path(&manifest_path, &config.build_root),
        ),
        (
            "input_dir".to_string(),
            relative_artifact_path(&input_dir, &config.build_root),
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
    )?;
    Ok(WorldBasemapSources {
        land_shp,
        boundaries_shp,
        source_fingerprint: manifest.source_fingerprint,
        source_fetched_at_utc: manifest.source_fetched_at_utc,
    })
}

pub(super) fn read_cached_source_manifest(path: &Path) -> anyhow::Result<CachedSourceManifest> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("failed to parse {}", path.display()))
}

pub(super) fn world_basemap_product_inputs(
    source_fingerprint: &str,
) -> anyhow::Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        ("product_id".to_string(), "world-basemap".to_string()),
        ("min_zoom".to_string(), WORLD_BASEMAP_MIN_ZOOM.to_string()),
        (
            "max_source_zoom".to_string(),
            WORLD_BASEMAP_MAX_SOURCE_ZOOM.to_string(),
        ),
        (
            "max_display_zoom".to_string(),
            WORLD_BASEMAP_MAX_DISPLAY_ZOOM.to_string(),
        ),
        ("tile_size".to_string(), WORLD_BASEMAP_TILE_SIZE.to_string()),
        (
            "source_fingerprint".to_string(),
            source_fingerprint.to_string(),
        ),
        (
            "world_basemap_pipeline".to_string(),
            WORLD_BASEMAP_PIPELINE_VERSION.to_string(),
        ),
        (
            "world_basemap_script".to_string(),
            hash_text(WORLD_BASEMAP_TILE_SCRIPT),
        ),
    ]))
}

pub(super) fn world_basemap_source_fingerprint(
    land_shp: &Path,
    boundaries_shp: &Path,
) -> anyhow::Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(b"world-basemap-v1");
    hasher.update(WORLD_BASEMAP_LAND_URL.as_bytes());
    hasher.update(hash_shapefile_family(land_shp)?.as_bytes());
    hasher.update(WORLD_BASEMAP_BOUNDARIES_URL.as_bytes());
    hasher.update(hash_shapefile_family(boundaries_shp)?.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

pub(super) fn hash_shapefile_family(shp_path: &Path) -> anyhow::Result<String> {
    let mut hasher = Sha256::new();
    let stem = shp_path
        .file_stem()
        .and_then(|value| value.to_str())
        .with_context(|| format!("invalid shapefile name {}", shp_path.display()))?;
    let parent = shp_path
        .parent()
        .with_context(|| format!("shapefile path has no parent {}", shp_path.display()))?;
    for extension in ["shp", "shx", "dbf", "prj"] {
        let path = parent.join(format!("{stem}.{extension}"));
        if !path.is_file() {
            bail!("missing shapefile component {}", path.display());
        }
        hasher.update(extension.as_bytes());
        hasher.update([0]);
        hasher.update(hash_file(&path)?.as_bytes());
        hasher.update([0xff]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(super) fn source_fetched_at_utc_for_urls(
    fetch_cache: &FetchCacheConfig,
    urls: &[&str],
) -> anyhow::Result<Option<String>> {
    let layout = CacheLayout::new(&fetch_cache.root);
    let mut fetched_times = Vec::new();
    for url in urls {
        let metadata_path = layout.http_metadata_path(url);
        if !metadata_path.is_file() {
            continue;
        }
        let value: serde_json::Value = serde_json::from_slice(
            &fs::read(&metadata_path)
                .with_context(|| format!("failed to read {}", metadata_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", metadata_path.display()))?;
        if let Some(fetched_at) = value.get("fetched_at_utc").and_then(|value| value.as_str()) {
            fetched_times.push(fetched_at.to_string());
            continue;
        }
        if let Ok(modified) = fs::metadata(&metadata_path).and_then(|metadata| metadata.modified())
        {
            fetched_times.push(
                DateTime::<Utc>::from(modified)
                    .format("%Y-%m-%dT%H:%M:%SZ")
                    .to_string(),
            );
        }
    }
    fetched_times.sort();
    Ok(fetched_times.into_iter().max())
}

pub(super) fn max_optional_utc(left: Option<String>, right: Option<String>) -> Option<String> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

pub(super) fn build_world_basemap_tiles(
    land_shp: &Path,
    boundaries_shp: &Path,
    output_dir: &Path,
    version_label: &str,
    source_fetched_at_utc: Option<&str>,
) -> anyhow::Result<()> {
    let script_path = output_dir.join("build_world_basemap_tiles.py");
    fs::write(&script_path, WORLD_BASEMAP_TILE_SCRIPT)
        .with_context(|| format!("failed to write {}", script_path.display()))?;
    let mut command = Command::new("python3");
    command
        .arg(&script_path)
        .arg("--land-shp")
        .arg(land_shp)
        .arg("--boundaries-shp")
        .arg(boundaries_shp)
        .arg("--output-dir")
        .arg(output_dir)
        .arg("--version-label")
        .arg(version_label)
        .arg("--min-zoom")
        .arg(WORLD_BASEMAP_MIN_ZOOM.to_string())
        .arg("--max-source-zoom")
        .arg(WORLD_BASEMAP_MAX_SOURCE_ZOOM.to_string())
        .arg("--max-display-zoom")
        .arg(WORLD_BASEMAP_MAX_DISPLAY_ZOOM.to_string())
        .arg("--tile-size")
        .arg(WORLD_BASEMAP_TILE_SIZE.to_string());
    if let Some(value) = source_fetched_at_utc {
        command.arg("--source-fetched-at-utc").arg(value);
    }
    let output = command
        .output()
        .with_context(|| format!("failed to run {}", script_path.display()))?;
    if !output.status.success() {
        bail!(
            "world basemap tile builder failed: {}\n{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
    }
    fs::remove_file(&script_path)
        .with_context(|| format!("failed to remove {}", script_path.display()))?;
    Ok(())
}

pub(super) fn build_shaded_relief_product(
    config: &ProductBuildConfig,
    region: Region,
    terrain_index_path: &Path,
    source_fetched_at_utc: Option<String>,
    water_mask_tiles_dir: &Path,
    water_mask_version: &str,
) -> anyhow::Result<(
    PathBuf,
    String,
    Option<String>,
    Vec<TileLevelRecord>,
    NodeRecord,
)> {
    let region_id = region.code().to_ascii_lowercase();
    let overlays = prepare_shaded_relief_overlay_sources(config)?;
    let input_dir = config
        .build_root
        .join("private-work")
        .join("shaded-relief")
        .join(&region_id)
        .join("input");
    let dem_dir = input_dir.join("dems");
    fs::create_dir_all(&dem_dir)
        .with_context(|| format!("failed to create {}", dem_dir.display()))?;

    let provenance_dir = config
        .build_root
        .join("meta")
        .join("provenance")
        .join(format!("shaded-relief-{region_id}"));
    let fetch_cache = terrain_fetch_cache_config(config)?;
    let mut dem_candidates = terrain_dem_candidates_for_region(terrain_index_path, region)?;
    if dem_candidates.is_empty() {
        bail!(
            "terrain discovery index has no DEM URLs for shaded relief {}",
            region.code()
        );
    }
    if let Some(cached_selection) = cached_terrain_dem_selection(&dem_candidates, &fetch_cache)? {
        let source_fingerprint = terrain_source_fingerprint_from_cached(
            &cached_selection.selection.urls,
            &cached_selection.sources,
            &cached_selection.selection.missing_cells,
            None,
        )?;
        let version_label = content_product_version_label(&source_fingerprint);
        let inputs = shaded_relief_product_inputs(
            region,
            &source_fingerprint,
            water_mask_version,
            &overlays.source_fingerprint,
        )?;
        let prepared = prepare_node_at(
            &build_shared_node_dir(config, &format!("static-shaded-relief-{region_id}"))?,
            &format!("static-shaded-relief-{region_id}"),
            &inputs,
        )?;
        let output_dir = prepared.dir.join("output");
        let package_dir = output_dir.join("package");
        let zip_path = output_dir.join(format!("shaded_relief_{region_id}_{version_label}.zip"));
        let manifest_path = package_dir.join("manifest.json");
        if let Some(record) = try_load_node_record(&prepared, &[zip_path.clone(), manifest_path])? {
            let tile_levels =
                read_static_tile_manifest_levels(&output_dir.join("package/manifest.json"))?;
            return Ok((
                zip_path,
                version_label,
                source_fetched_at_utc,
                tile_levels,
                record,
            ));
        }
    }
    let dem_selection = prefetch_terrain_dems_with_fallback(
        &mut dem_candidates,
        &dem_dir,
        config.fetch_jobs,
        &fetch_cache,
        &provenance_dir,
        &format!("shaded-relief-{region_id}-dem"),
    )?;
    let dem_paths = terrain_dem_paths_from_requests(&dem_dir, &dem_selection.requests)?;
    let source_fingerprint = if let Some(sources) =
        cached_terrain_dem_sources_for_requests(&fetch_cache, &dem_selection.requests)?
    {
        terrain_source_fingerprint_from_cached(
            &dem_selection.urls,
            &sources,
            &dem_selection.missing_cells,
            None,
        )?
    } else {
        terrain_source_fingerprint(
            &dem_selection.urls,
            &dem_paths,
            &dem_selection.missing_cells,
            None,
        )?
    };
    let version_label = content_product_version_label(&source_fingerprint);
    let inputs = shaded_relief_product_inputs(
        region,
        &source_fingerprint,
        water_mask_version,
        &overlays.source_fingerprint,
    )?;
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, &format!("static-shaded-relief-{region_id}"))?,
        &format!("static-shaded-relief-{region_id}"),
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let package_dir = output_dir.join("package");
    let zip_path = output_dir.join(format!("shaded_relief_{region_id}_{version_label}.zip"));
    let manifest_path = package_dir.join("manifest.json");
    let _build_lock = match claim_or_wait_for_node(&prepared, &[zip_path.clone(), manifest_path])? {
        NodeCacheState::CacheHit(record) => {
            let tile_levels =
                read_static_tile_manifest_levels(&output_dir.join("package/manifest.json"))?;
            return Ok((
                zip_path,
                version_label,
                source_fetched_at_utc,
                tile_levels,
                record,
            ));
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
    let vrt_path = output_dir.join(format!("shaded_relief_{region_id}.vrt"));
    build_terrain_vrt(&vrt_path, &dem_paths)?;
    build_shaded_relief_region_tiles(
        region,
        &vrt_path,
        &output_dir,
        &version_label,
        &dem_selection,
        water_mask_tiles_dir,
        &overlays.state_borders_shp,
        &overlays.primary_roads_shp,
        false,
    )?;
    move_static_tile_tree_under_chart_index(&output_dir, 0)?;
    stage_static_tile_zoom_subset(
        &output_dir,
        &package_dir,
        Some(FULL_COVERAGE_ZOOM + 1),
        None,
        false,
    )?;
    let tile_levels = read_static_tile_manifest_levels(&package_dir.join("manifest.json"))?;
    zip_directory_deterministic(&zip_path, &package_dir, &["manifest.json", "tiles"])?;
    let outputs = BTreeMap::from([
        (
            "manifest".to_string(),
            relative_artifact_path(&package_dir.join("manifest.json"), &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&zip_path, &config.build_root),
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
        zip_path,
        version_label,
        source_fetched_at_utc,
        tile_levels,
        record,
    ))
}

pub(super) struct ShadedReliefOverlaySources {
    state_borders_shp: PathBuf,
    primary_roads_shp: PathBuf,
    source_fingerprint: String,
}

pub(super) fn prepare_shaded_relief_overlay_sources(
    config: &ProductBuildConfig,
) -> anyhow::Result<ShadedReliefOverlaySources> {
    let inputs = BTreeMap::from([
        (
            "state_borders_url".to_string(),
            SHADED_RELIEF_STATE_BORDERS_URL.to_string(),
        ),
        (
            "primary_roads_url".to_string(),
            SHADED_RELIEF_PRIMARY_ROADS_URL.to_string(),
        ),
        ("fetch_jobs".to_string(), config.fetch_jobs.to_string()),
        (
            "fetch_lib".to_string(),
            hash_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("preprocessor-cli should live under workspace root")
                    .join("preprocessor-fetch/src/lib.rs"),
            )?,
        ),
        (
            "source_node_version".to_string(),
            "shaded-relief-overlay-sources-v1".to_string(),
        ),
    ]);
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "static-shaded-relief-overlay-sources")?,
        "static-shaded-relief-overlay-sources",
        &inputs,
    )?;
    let input_dir = prepared.dir.join("input");
    let manifest_path = prepared.dir.join("source-manifest.json");
    let state_borders_shp = input_dir.join("ne_50m_admin_1_states_provinces_lines.shp");
    let primary_roads_shp = input_dir.join("tl_2025_us_primaryroads.shp");
    let expected_outputs = vec![
        manifest_path.clone(),
        state_borders_shp.clone(),
        primary_roads_shp.clone(),
    ];
    let _build_lock = match claim_or_wait_for_node(&prepared, &expected_outputs)? {
        NodeCacheState::CacheHit(_) => {
            let manifest = read_cached_source_manifest(&manifest_path)?;
            return Ok(ShadedReliefOverlaySources {
                state_borders_shp,
                primary_roads_shp,
                source_fingerprint: manifest.source_fingerprint,
            });
        }
        NodeCacheState::Build(lock) => lock,
    };

    let started_at_utc = utc_now_string();
    let started = Instant::now();
    fs::create_dir_all(&input_dir)
        .with_context(|| format!("failed to create {}", input_dir.display()))?;
    let provenance_dir = prepared.dir.join("meta/provenance/shaded-relief-overlays");
    fs::create_dir_all(&provenance_dir)?;
    let fetch_cache = static_source_fetch_cache_config(config)?;
    let requests = [
        PrefetchRequest::new(SHADED_RELIEF_STATE_BORDERS_URL)
            .with_logical_file_name("ne_50m_admin_1_states_provinces_lines.zip"),
        PrefetchRequest::new(SHADED_RELIEF_PRIMARY_ROADS_URL)
            .with_logical_file_name("tl_2025_us_primaryroads.zip"),
    ];
    prefetch_requests_with_provenance(
        &requests,
        &input_dir,
        config.fetch_jobs,
        Some(&fetch_cache),
        &provenance_dir,
        "shaded-relief-overlays",
    )?;
    if !state_borders_shp.is_file() {
        bail!(
            "shaded relief overlay fetch missing {}",
            state_borders_shp.display()
        );
    }
    if !primary_roads_shp.is_file() {
        bail!(
            "shaded relief overlay fetch missing {}",
            primary_roads_shp.display()
        );
    }
    let mut hasher = Sha256::new();
    hasher.update(b"shaded-relief-overlays-v1");
    hasher.update(SHADED_RELIEF_STATE_BORDERS_URL.as_bytes());
    hasher.update(hash_shapefile_family(&state_borders_shp)?.as_bytes());
    hasher.update(SHADED_RELIEF_PRIMARY_ROADS_URL.as_bytes());
    hasher.update(hash_shapefile_family(&primary_roads_shp)?.as_bytes());
    let manifest = CachedSourceManifest {
        source_fingerprint: format!("{:x}", hasher.finalize()),
        source_fetched_at_utc: source_fetched_at_utc_for_urls(
            &fetch_cache,
            &[
                SHADED_RELIEF_STATE_BORDERS_URL,
                SHADED_RELIEF_PRIMARY_ROADS_URL,
            ],
        )?,
    };
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).context("failed to encode source manifest")?,
    )
    .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    let outputs = BTreeMap::from([
        (
            "source_manifest".to_string(),
            relative_artifact_path(&manifest_path, &config.build_root),
        ),
        (
            "input_dir".to_string(),
            relative_artifact_path(&input_dir, &config.build_root),
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
    )?;
    Ok(ShadedReliefOverlaySources {
        state_borders_shp,
        primary_roads_shp,
        source_fingerprint: manifest.source_fingerprint,
    })
}

pub(super) fn build_shaded_relief_wide_product(
    config: &ProductBuildConfig,
    regional_products: &[(String, PathBuf, String, String, Option<String>)],
    overlays: &ShadedReliefOverlaySources,
) -> anyhow::Result<(
    PathBuf,
    String,
    Option<String>,
    Vec<TileLevelRecord>,
    NodeRecord,
)> {
    let source_fingerprint =
        shaded_relief_wide_source_fingerprint(regional_products, &overlays.source_fingerprint);
    let version_label = content_product_version_label(&source_fingerprint);
    let source_fetched_at_utc = regional_products
        .iter()
        .filter_map(
            |(_region_id, _output_dir, _source_version, _zip_sha256, fetched_at)| {
                fetched_at.clone()
            },
        )
        .max();
    let inputs = shaded_relief_wide_product_inputs(
        regional_products,
        &source_fingerprint,
        &overlays.source_fingerprint,
    );
    let prepared = prepare_node_at(
        &build_shared_node_dir(
            config,
            &format!("static-shaded-relief-{WIDE_ANGLE_REGION_ID}"),
        )?,
        &format!("static-shaded-relief-{WIDE_ANGLE_REGION_ID}"),
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let manifest_path = output_dir.join("manifest.json");
    let zip_path = output_dir.join(format!(
        "shaded_relief_{WIDE_ANGLE_REGION_ID}_{version_label}.zip"
    ));
    if let Some(record) =
        try_load_node_record(&prepared, &[zip_path.clone(), manifest_path.clone()])?
    {
        let tile_levels = read_static_tile_manifest_levels(&manifest_path)?;
        return Ok((
            zip_path,
            version_label,
            source_fetched_at_utc,
            tile_levels,
            record,
        ));
    }
    let _build_lock =
        match claim_or_wait_for_node(&prepared, &[zip_path.clone(), manifest_path.clone()])? {
            NodeCacheState::CacheHit(record) => {
                let tile_levels = read_static_tile_manifest_levels(&manifest_path)?;
                return Ok((
                    zip_path,
                    version_label,
                    source_fetched_at_utc,
                    tile_levels,
                    record,
                ));
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
    composite_shaded_relief_wide_tiles(
        regional_products,
        &output_dir,
        &version_label,
        &source_fingerprint,
        &overlays.state_borders_shp,
        &overlays.primary_roads_shp,
    )?;
    let tile_levels = read_static_tile_manifest_levels(&manifest_path)?;
    zip_directory_deterministic(&zip_path, &output_dir, &["manifest.json", "tiles"])?;
    let outputs = BTreeMap::from([
        (
            "manifest".to_string(),
            relative_artifact_path(&manifest_path, &config.build_root),
        ),
        (
            "zip".to_string(),
            relative_artifact_path(&zip_path, &config.build_root),
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
        zip_path,
        version_label,
        source_fetched_at_utc,
        tile_levels,
        record,
    ))
}

pub(super) fn shaded_relief_wide_source_fingerprint(
    regional_products: &[(String, PathBuf, String, String, Option<String>)],
    overlay_source_fingerprint: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"shaded-relief-wide-v1");
    hasher.update(FULL_COVERAGE_ZOOM.to_string().as_bytes());
    hasher.update(overlay_source_fingerprint.as_bytes());
    hasher.update(SHADED_RELIEF_OVERLAY_STYLE_VERSION.as_bytes());
    for (region_id, _output_dir, source_version, zip_sha256, _fetched_at) in regional_products {
        hasher.update(region_id.as_bytes());
        hasher.update([0]);
        hasher.update(source_version.as_bytes());
        hasher.update([0]);
        hasher.update(zip_sha256.as_bytes());
        hasher.update([0xff]);
    }
    format!("{:x}", hasher.finalize())
}

pub(super) fn shaded_relief_wide_product_inputs(
    regional_products: &[(String, PathBuf, String, String, Option<String>)],
    source_fingerprint: &str,
    overlay_source_fingerprint: &str,
) -> BTreeMap<String, String> {
    let region_versions = regional_products
        .iter()
        .map(
            |(region_id, _output_dir, source_version, zip_sha256, _fetched_at)| {
                format!("{region_id}:{source_version}:{zip_sha256}")
            },
        )
        .collect::<Vec<_>>()
        .join(",");
    BTreeMap::from([
        (
            "product_id".to_string(),
            format!("shaded-relief-{WIDE_ANGLE_REGION_ID}"),
        ),
        (
            "wide_angle_max_zoom".to_string(),
            FULL_COVERAGE_ZOOM.to_string(),
        ),
        ("tile_size".to_string(), TERRAIN_TILE_SIZE.to_string()),
        (
            "source_fingerprint".to_string(),
            source_fingerprint.to_string(),
        ),
        (
            "overlay_source_fingerprint".to_string(),
            overlay_source_fingerprint.to_string(),
        ),
        (
            "overlay_style".to_string(),
            SHADED_RELIEF_OVERLAY_STYLE_VERSION.to_string(),
        ),
        ("region_versions".to_string(), region_versions),
        (
            "shaded_relief_pipeline".to_string(),
            SHADED_RELIEF_PIPELINE_VERSION.to_string(),
        ),
        (
            "wide_angle_script".to_string(),
            hash_text(SHADED_RELIEF_WIDE_TILE_SCRIPT),
        ),
    ])
}

pub(super) fn composite_shaded_relief_wide_tiles(
    regional_products: &[(String, PathBuf, String, String, Option<String>)],
    output_dir: &Path,
    version_label: &str,
    source_fingerprint: &str,
    state_borders_shp: &Path,
    primary_roads_shp: &Path,
) -> anyhow::Result<()> {
    let script_path = output_dir.join("build_shaded_relief_wide_tiles.py");
    fs::write(&script_path, SHADED_RELIEF_WIDE_TILE_SCRIPT)
        .with_context(|| format!("failed to write {}", script_path.display()))?;
    let mut command = Command::new("python3");
    command
        .arg(&script_path)
        .arg("--output-dir")
        .arg(output_dir)
        .arg("--version-label")
        .arg(version_label)
        .arg("--source-fingerprint")
        .arg(source_fingerprint)
        .arg("--state-borders-shp")
        .arg(state_borders_shp)
        .arg("--primary-roads-shp")
        .arg(primary_roads_shp)
        .arg("--overlay-style-version")
        .arg(SHADED_RELIEF_OVERLAY_STYLE_VERSION)
        .arg("--max-zoom")
        .arg(FULL_COVERAGE_ZOOM.to_string())
        .arg("--tile-size")
        .arg(TERRAIN_TILE_SIZE.to_string());
    for (_region_id, output_dir, _source_version, _zip_sha256, _fetched_at) in regional_products {
        command.arg("--source-dir").arg(output_dir);
    }
    let output = command
        .output()
        .with_context(|| format!("failed to run {}", script_path.display()))?;
    if !output.status.success() {
        bail!(
            "shaded relief wide tile builder failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::remove_file(&script_path)
        .with_context(|| format!("failed to remove {}", script_path.display()))?;
    Ok(())
}

pub(super) fn shaded_relief_product_inputs(
    region: Region,
    source_fingerprint: &str,
    water_mask_version: &str,
    overlay_source_fingerprint: &str,
) -> anyhow::Result<BTreeMap<String, String>> {
    let region_id = region.code().to_ascii_lowercase();
    Ok(BTreeMap::from([
        (
            "product_id".to_string(),
            format!("shaded-relief-{region_id}"),
        ),
        ("region".to_string(), region.code().to_string()),
        ("min_zoom".to_string(), TERRAIN_MIN_ZOOM.to_string()),
        ("max_zoom".to_string(), TERRAIN_ZOOM.to_string()),
        ("tile_size".to_string(), TERRAIN_TILE_SIZE.to_string()),
        (
            "source_fingerprint".to_string(),
            source_fingerprint.to_string(),
        ),
        (
            "water_mask_version".to_string(),
            water_mask_version.to_string(),
        ),
        (
            "overlay_source_fingerprint".to_string(),
            overlay_source_fingerprint.to_string(),
        ),
        (
            "overlay_style".to_string(),
            SHADED_RELIEF_OVERLAY_STYLE_VERSION.to_string(),
        ),
        (
            "shaded_relief_pipeline".to_string(),
            SHADED_RELIEF_PIPELINE_VERSION.to_string(),
        ),
        (
            "shaded_relief_script".to_string(),
            hash_file(shaded_relief_tile_script_path())?,
        ),
    ]))
}

#[derive(Debug, Deserialize)]
pub(super) struct StaticTileManifest {
    levels: Vec<StaticTileManifestLevel>,
}

#[derive(Debug, Deserialize)]
pub(super) struct StaticTileManifestLevel {
    zoom: u32,
    boxes: Vec<TileBoundsRecord>,
}

pub(super) fn read_static_tile_manifest_levels(
    manifest_path: &Path,
) -> anyhow::Result<Vec<TileLevelRecord>> {
    let manifest: StaticTileManifest = serde_json::from_slice(
        &fs::read(manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    let levels = manifest
        .levels
        .into_iter()
        .map(|level| TileLevelRecord {
            zoom: level.zoom,
            boxes: level.boxes,
        })
        .collect::<Vec<_>>();
    if levels.is_empty() {
        bail!(
            "static tile manifest {} had no levels",
            manifest_path.display()
        );
    }
    Ok(levels)
}

pub(super) fn move_static_tile_tree_under_chart_index(
    output_dir: &Path,
    chart_index: u32,
) -> anyhow::Result<()> {
    let tiles_dir = output_dir.join("tiles");
    let chart_index_dir = tiles_dir.join(chart_index.to_string());
    if chart_index_dir.exists() {
        fs::remove_dir_all(&chart_index_dir)
            .with_context(|| format!("failed to remove {}", chart_index_dir.display()))?;
    }
    let tmp_dir = output_dir.join(format!(".tiles-chart-index-{chart_index}"));
    if tmp_dir.exists() {
        fs::remove_dir_all(&tmp_dir)
            .with_context(|| format!("failed to remove {}", tmp_dir.display()))?;
    }
    fs::create_dir_all(&tmp_dir)
        .with_context(|| format!("failed to create {}", tmp_dir.display()))?;
    for entry in fs::read_dir(&tiles_dir)
        .with_context(|| format!("failed to read {}", tiles_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if name.starts_with('.') || name == chart_index.to_string() {
            continue;
        }
        fs::rename(&path, tmp_dir.join(&file_name)).with_context(|| {
            format!(
                "failed to move {} under chart-index staging",
                path.display()
            )
        })?;
    }
    fs::rename(&tmp_dir, &chart_index_dir).with_context(|| {
        format!(
            "failed to install chart-index tile tree at {}",
            chart_index_dir.display()
        )
    })?;
    Ok(())
}

pub(super) fn stage_static_tile_zoom_subset(
    source_output_dir: &Path,
    package_dir: &Path,
    min_zoom: Option<u32>,
    max_zoom: Option<u32>,
    direct_zoom_tiles: bool,
) -> anyhow::Result<()> {
    if package_dir.exists() {
        fs::remove_dir_all(package_dir)
            .with_context(|| format!("failed to remove {}", package_dir.display()))?;
    }
    fs::create_dir_all(package_dir)
        .with_context(|| format!("failed to create {}", package_dir.display()))?;
    let manifest_path = source_output_dir.join("manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    let levels = manifest
        .get("levels")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|level| {
            let Some(zoom) = level.get("zoom").and_then(|value| value.as_u64()) else {
                return false;
            };
            let zoom = zoom as u32;
            min_zoom.map(|min_zoom| zoom >= min_zoom).unwrap_or(true)
                && max_zoom.map(|max_zoom| zoom <= max_zoom).unwrap_or(true)
        })
        .collect::<Vec<_>>();
    if levels.is_empty() {
        bail!(
            "no static tile levels selected from {} for min_zoom={:?} max_zoom={:?}",
            source_output_dir.display(),
            min_zoom,
            max_zoom
        );
    }
    manifest["levels"] = serde_json::Value::Array(levels);
    if let Some(min_zoom) = min_zoom {
        manifest["min_zoom"] = serde_json::Value::from(min_zoom);
    }
    if let Some(max_zoom) = max_zoom {
        manifest["max_zoom"] = serde_json::Value::from(max_zoom);
    }
    fs::write(
        package_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).context("failed to encode static tile manifest")?,
    )
    .with_context(|| {
        format!(
            "failed to write {}",
            package_dir.join("manifest.json").display()
        )
    })?;
    let source_tiles_dir = source_output_dir.join("tiles");
    let package_tiles_dir = package_dir.join("tiles");
    hardlink_static_tile_zoom_subset(
        &source_tiles_dir,
        &package_tiles_dir,
        min_zoom,
        max_zoom,
        direct_zoom_tiles,
    )?;
    Ok(())
}

pub(super) fn hardlink_static_tile_zoom_subset(
    source_dir: &Path,
    output_dir: &Path,
    min_zoom: Option<u32>,
    max_zoom: Option<u32>,
    direct_zoom_tiles: bool,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(source_dir)
        .with_context(|| format!("failed to read {}", source_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let output = output_dir.join(entry.file_name());
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to stat {}", path.display()))?;
        if file_type.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let zoom_filter_applies = path
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .map(|name| name == "0" || (direct_zoom_tiles && name == "tiles"))
                .unwrap_or(false);
            if zoom_filter_applies {
                if let Ok(zoom) = name.parse::<u32>() {
                    if min_zoom.map(|min_zoom| zoom < min_zoom).unwrap_or(false)
                        || max_zoom.map(|max_zoom| zoom > max_zoom).unwrap_or(false)
                    {
                        continue;
                    }
                }
            }
            hardlink_static_tile_zoom_subset(
                &path,
                &output,
                min_zoom,
                max_zoom,
                direct_zoom_tiles,
            )?;
        } else if file_type.is_file() {
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            fs::hard_link(&path, &output).with_context(|| {
                format!(
                    "failed to hardlink {} to {}",
                    path.display(),
                    output.display()
                )
            })?;
        }
    }
    Ok(())
}

pub(super) fn terrain_tnmaccess_request(region: Region) -> PrefetchRequest {
    let bounds = region.bounds();
    let bbox = format!(
        "{},{},{},{}",
        bounds.lon_min, bounds.lat_min, bounds.lon_max, bounds.lat_max
    );
    let url = format!(
        "https://tnmaccess.nationalmap.gov/api/v1/products?bbox={bbox}&datasets=National%20Elevation%20Dataset%20(NED)%201%20arc-second%20Current&prodFormats=GeoTIFF&max=3000"
    );
    let logical_file_name = format!(
        "terrain_{}_tnmaccess.json",
        region.code().to_ascii_lowercase()
    );
    PrefetchRequest::new(&url)
        .with_logical_file_name(&logical_file_name)
        .with_cache_key(format!("{url}#logical_name={logical_file_name}"))
}

pub(super) fn build_terrain_discovery_index(
    config: &ProductBuildConfig,
) -> anyhow::Result<(PathBuf, Option<String>, NodeRecord)> {
    let discovery_dir = config
        .build_root
        .join("private-work")
        .join("terrain")
        .join("global-discovery")
        .join("input");
    fs::create_dir_all(&discovery_dir)
        .with_context(|| format!("failed to create {}", discovery_dir.display()))?;
    let provenance_dir = config
        .build_root
        .join("meta")
        .join("provenance")
        .join("terrain-discovery");
    let fetch_cache = terrain_fetch_cache_config(config)?;
    let discovery_requests = config
        .profile
        .terrain_regions()
        .iter()
        .map(|region| terrain_tnmaccess_request(*region))
        .collect::<Vec<_>>();
    prefetch_archives_with_provenance(
        &discovery_requests,
        &discovery_dir,
        config.fetch_jobs,
        Some(&fetch_cache),
        &provenance_dir,
        "terrain-discovery",
    )?;

    let mut by_cell = BTreeMap::<String, Vec<TerrainDemCandidate>>::new();
    let mut discovery_hashes = BTreeMap::new();
    for region in config.profile.terrain_regions() {
        let region_id = region.code().to_ascii_lowercase();
        let path = discovery_dir.join(format!("terrain_{region_id}_tnmaccess.json"));
        discovery_hashes.insert(region_id, hash_file(&path)?);
        for (cell, mut candidates) in terrain_dem_candidates_from_tnmaccess(&path)? {
            by_cell.entry(cell).or_default().append(&mut candidates);
        }
    }
    normalize_terrain_candidates(&mut by_cell);
    let source_fetched_at_utc =
        terrain_source_fetched_at_utc(&fetch_cache, &discovery_requests, &[])?;
    let source_fingerprint = terrain_discovery_fingerprint(&by_cell, &discovery_hashes);
    let inputs = BTreeMap::from([
        ("product_id".to_string(), "terrain-discovery".to_string()),
        (
            "regions".to_string(),
            config
                .profile
                .terrain_regions()
                .iter()
                .map(|region| region.code())
                .collect::<Vec<_>>()
                .join(","),
        ),
        ("source_fingerprint".to_string(), source_fingerprint.clone()),
        (
            "terrain_discovery_builder".to_string(),
            source_fingerprints::terrain_discovery_builder_fingerprint()?,
        ),
    ]);
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "static-terrain-discovery")?,
        "static-terrain-discovery",
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let index_path = output_dir.join("terrain_dem_index.json");
    let _build_lock = match claim_or_wait_for_node(&prepared, &[index_path.clone()])? {
        NodeCacheState::CacheHit(record) => {
            return Ok((index_path, source_fetched_at_utc, record));
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
    let index = TerrainDemIndex {
        schema_version: 1,
        regions: config
            .profile
            .terrain_regions()
            .iter()
            .map(|region| region.code().to_string())
            .collect(),
        source_fetched_at_utc: source_fetched_at_utc.clone(),
        cells: by_cell,
    };
    fs::write(
        &index_path,
        serde_json::to_vec_pretty(&index).context("failed to encode terrain DEM index")?,
    )
    .with_context(|| format!("failed to write {}", index_path.display()))?;
    let outputs = BTreeMap::from([(
        "index".to_string(),
        relative_artifact_path(&index_path, &config.build_root),
    )]);
    let record = write_node_record(
        prepared,
        inputs,
        outputs,
        false,
        started_at_utc,
        utc_now_string(),
        started.elapsed().as_millis() as u64,
    )?;
    Ok((index_path, source_fetched_at_utc, record))
}

pub(super) fn terrain_dem_candidates_from_tnmaccess(
    path: &Path,
) -> anyhow::Result<BTreeMap<String, Vec<TerrainDemCandidate>>> {
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))?;
    let items = value
        .get("items")
        .and_then(|value| value.as_array())
        .context("TNMAccess response missing items[]")?;
    let mut by_cell = BTreeMap::<String, Vec<TerrainDemCandidate>>::new();
    for item in items {
        let Some(url) = item.get("downloadURL").and_then(|value| value.as_str()) else {
            continue;
        };
        if !url.ends_with(".tif") {
            continue;
        }
        let filename = url.rsplit('/').next().unwrap_or("dem.tif").to_string();
        let Some(cell) = terrain_dem_cell_from_filename(&filename) else {
            continue;
        };
        let candidate = TerrainDemCandidate {
            url: url.to_string(),
            publication_date: item
                .get("publicationDate")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string(),
            last_updated: item
                .get("lastUpdated")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string(),
            filename,
        };
        by_cell.entry(cell).or_default().push(candidate);
    }
    normalize_terrain_candidates(&mut by_cell);
    Ok(by_cell)
}

pub(super) fn normalize_terrain_candidates(
    candidates_by_cell: &mut BTreeMap<String, Vec<TerrainDemCandidate>>,
) {
    for candidates in candidates_by_cell.values_mut() {
        candidates.sort_by(|left, right| right.sort_key().cmp(&left.sort_key()));
        candidates.dedup_by(|left, right| left.url == right.url);
    }
}

pub(super) fn terrain_discovery_fingerprint(
    candidates: &BTreeMap<String, Vec<TerrainDemCandidate>>,
    discovery_hashes: &BTreeMap<String, String>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"terrain-discovery-v1");
    for (region, hash) in discovery_hashes {
        hasher.update(region.as_bytes());
        hasher.update([0]);
        hasher.update(hash.as_bytes());
        hasher.update([0xff]);
    }
    for (cell, cell_candidates) in candidates {
        hasher.update(cell.as_bytes());
        hasher.update([0]);
        for candidate in cell_candidates {
            hasher.update(candidate.url.as_bytes());
            hasher.update([0]);
        }
        hasher.update([0xff]);
    }
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TerrainDemIndex {
    schema_version: u32,
    regions: Vec<String>,
    source_fetched_at_utc: Option<String>,
    cells: BTreeMap<String, Vec<TerrainDemCandidate>>,
}

#[derive(Debug, Clone)]
pub(super) struct TerrainCellCandidates {
    cell: String,
    candidates: Vec<TerrainDemCandidate>,
    selected: usize,
    missing: bool,
}

impl TerrainCellCandidates {
    fn selected_candidate(&self) -> anyhow::Result<&TerrainDemCandidate> {
        self.candidates
            .get(self.selected)
            .with_context(|| format!("terrain cell {} has no selected DEM candidate", self.cell))
    }

    fn selected_request(&self) -> anyhow::Result<PrefetchRequest> {
        Ok(self.selected_candidate()?.prefetch_request())
    }

    fn selected_request_if_available(&self) -> anyhow::Result<Option<PrefetchRequest>> {
        if self.missing {
            return Ok(None);
        }
        Ok(Some(self.selected_request()?))
    }

    fn advance_after_failed_url(&mut self, failed_url: &str) -> anyhow::Result<TerrainCellAction> {
        if self.selected_candidate()?.url != failed_url {
            return Ok(TerrainCellAction::Unaffected);
        }
        if self.selected + 1 >= self.candidates.len() {
            self.missing = true;
            return Ok(TerrainCellAction::MarkedMissing);
        }
        self.selected += 1;
        Ok(TerrainCellAction::Advanced)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TerrainCellAction {
    Unaffected,
    Advanced,
    MarkedMissing,
}

#[derive(Debug, Clone)]
pub(super) struct TerrainDemSelection {
    urls: Vec<String>,
    requests: Vec<PrefetchRequest>,
    missing_cells: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct CachedTerrainDemSelection {
    selection: TerrainDemSelection,
    sources: Vec<CachedTerrainDemSource>,
}

#[derive(Debug, Clone)]
pub(super) struct CachedTerrainDemSource {
    filename: String,
    sha256: String,
}

pub(super) fn terrain_dem_candidates_for_region(
    index_path: &Path,
    region: Region,
) -> anyhow::Result<Vec<TerrainCellCandidates>> {
    let index: TerrainDemIndex = serde_json::from_slice(
        &fs::read(index_path)
            .with_context(|| format!("failed to read {}", index_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", index_path.display()))?;
    Ok(index
        .cells
        .into_iter()
        .filter(|(cell, _)| terrain_cell_intersects_region(cell, region))
        .map(|(cell, candidates)| TerrainCellCandidates {
            cell,
            candidates,
            selected: 0,
            missing: false,
        })
        .collect())
}

pub(super) fn cached_terrain_dem_selection(
    cells: &[TerrainCellCandidates],
    fetch_cache: &FetchCacheConfig,
) -> anyhow::Result<Option<CachedTerrainDemSelection>> {
    let mut urls = Vec::new();
    let mut requests = Vec::new();
    let mut sources = Vec::new();
    let mut missing_cells = Vec::new();
    for cell in cells {
        let mut cached_candidates = Vec::new();
        for (index, candidate) in cell.candidates.iter().enumerate() {
            if let Some(source) = cached_terrain_dem_source(fetch_cache, candidate)? {
                cached_candidates.push((index, candidate, source));
            }
        }
        match cached_candidates.as_slice() {
            [(0, candidate, source), ..] => {
                urls.push(candidate.url.clone());
                requests.push(candidate.prefetch_request());
                sources.push(source.clone());
            }
            [] => missing_cells.push(cell.cell.clone()),
            _ => {
                // A later cached candidate may be an intentional fallback, or it may be stale
                // relative to a newly-discovered newer DEM. Fetching is the only safe way to
                // distinguish those cases, so do not use the early cache-hit path.
                return Ok(None);
            }
        }
    }
    Ok(Some(CachedTerrainDemSelection {
        selection: TerrainDemSelection {
            urls,
            requests,
            missing_cells,
        },
        sources,
    }))
}

pub(super) fn cached_terrain_dem_source(
    fetch_cache: &FetchCacheConfig,
    candidate: &TerrainDemCandidate,
) -> anyhow::Result<Option<CachedTerrainDemSource>> {
    let layout = CacheLayout::new(&fetch_cache.root);
    let metadata_path = layout.http_metadata_path(&candidate.prefetch_request().cache_key);
    if !metadata_path.is_file() {
        return Ok(None);
    }
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(&metadata_path)
            .with_context(|| format!("failed to read {}", metadata_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", metadata_path.display()))?;
    let Some(sha256) = value.get("sha256").and_then(|value| value.as_str()) else {
        return Ok(None);
    };
    if !layout.blob_path(sha256).is_file() {
        return Ok(None);
    }
    Ok(Some(CachedTerrainDemSource {
        filename: candidate.filename.clone(),
        sha256: sha256.to_string(),
    }))
}

pub(super) fn cached_terrain_dem_sources_for_requests(
    fetch_cache: &FetchCacheConfig,
    requests: &[PrefetchRequest],
) -> anyhow::Result<Option<Vec<CachedTerrainDemSource>>> {
    let layout = CacheLayout::new(&fetch_cache.root);
    let mut sources = Vec::new();
    for request in requests {
        let metadata_path = layout.http_metadata_path(&request.cache_key);
        if !metadata_path.is_file() {
            return Ok(None);
        }
        let value: serde_json::Value = serde_json::from_slice(
            &fs::read(&metadata_path)
                .with_context(|| format!("failed to read {}", metadata_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", metadata_path.display()))?;
        let Some(sha256) = value.get("sha256").and_then(|value| value.as_str()) else {
            return Ok(None);
        };
        if !layout.blob_path(sha256).is_file() {
            return Ok(None);
        }
        sources.push(CachedTerrainDemSource {
            filename: request
                .logical_file_name
                .clone()
                .or_else(|| request.url.rsplit('/').next().map(ToOwned::to_owned))
                .context("terrain DEM request has no filename")?,
            sha256: sha256.to_string(),
        });
    }
    Ok(Some(sources))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TerrainDemCandidate {
    url: String,
    publication_date: String,
    last_updated: String,
    filename: String,
}

impl TerrainDemCandidate {
    fn sort_key(&self) -> (&str, &str, &str) {
        (&self.publication_date, &self.last_updated, &self.filename)
    }

    fn prefetch_request(&self) -> PrefetchRequest {
        PrefetchRequest::new(&self.url)
            .with_logical_file_name(&self.filename)
            .with_cache_key(format!("{}#logical_name={}", self.url, self.filename))
    }
}

pub(super) fn terrain_dem_cell_from_filename(filename: &str) -> Option<String> {
    filename
        .split('_')
        .find(|part| {
            let bytes = part.as_bytes();
            matches!(bytes.first(), Some(b'n' | b's')) && (part.contains('w') || part.contains('e'))
        })
        .map(ToOwned::to_owned)
}

pub(super) fn terrain_cell_intersects_region(cell: &str, region: Region) -> bool {
    let Some((lat_min, lon_min)) = terrain_cell_origin(cell) else {
        return false;
    };
    let bounds = region.bounds();
    let lat_max = lat_min + 1.0;
    let lon_max = lon_min + 1.0;
    lon_min < bounds.lon_max
        && lon_max > bounds.lon_min
        && lat_min < bounds.lat_max
        && lat_max > bounds.lat_min
}

pub(super) fn terrain_cell_origin(cell: &str) -> Option<(f64, f64)> {
    let lon_start = cell.find('w').or_else(|| cell.find('e'))?;
    let (lat_part, lon_part_with_dir) = cell.split_at(lon_start);
    let (lon_dir, lon_part) = lon_part_with_dir.split_at(1);
    let lat_abs = lat_part.get(1..)?.parse::<f64>().ok()?;
    let lon_abs = lon_part.parse::<f64>().ok()?;
    let lat_north_edge = if lat_part.starts_with('s') {
        -lat_abs
    } else {
        lat_abs
    };
    let lat = lat_north_edge - 1.0;
    let lon = if lon_dir == "w" { -lon_abs } else { lon_abs };
    Some((lat, lon))
}

pub(super) fn prefetch_terrain_dems_with_fallback(
    cells: &mut [TerrainCellCandidates],
    dem_dir: &Path,
    fetch_jobs: usize,
    fetch_cache: &FetchCacheConfig,
    provenance_dir: &Path,
    label: &str,
) -> anyhow::Result<TerrainDemSelection> {
    loop {
        let requests = cells
            .iter()
            .map(TerrainCellCandidates::selected_request_if_available)
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        match prefetch_archives_with_provenance(
            &requests,
            dem_dir,
            fetch_jobs,
            Some(fetch_cache),
            provenance_dir,
            label,
        ) {
            Ok(()) => {
                let missing_cells = cells
                    .iter()
                    .filter(|cell| cell.missing)
                    .map(|cell| cell.cell.clone())
                    .collect::<Vec<_>>();
                let urls = requests
                    .iter()
                    .map(|request| request.url.clone())
                    .collect::<Vec<_>>();
                return Ok(TerrainDemSelection {
                    urls,
                    requests,
                    missing_cells,
                });
            }
            Err(error) => {
                let message = error.to_string();
                let Some(failed_url) = terrain_failed_fetch_url(&message) else {
                    return Err(error);
                };
                let Some(failed_request) = requests
                    .iter()
                    .find(|request| request.url == failed_url)
                    .cloned()
                else {
                    return Err(error);
                };
                let mut handled = false;
                for cell in cells.iter_mut() {
                    match cell.advance_after_failed_url(&failed_request.url)? {
                        TerrainCellAction::Unaffected => {}
                        TerrainCellAction::Advanced => {
                            eprintln!(
                                "terrain DEM fetch failed for {}; falling back to next candidate for cell {}",
                                failed_request.url, cell.cell
                            );
                            handled = true;
                            break;
                        }
                        TerrainCellAction::MarkedMissing => {
                            eprintln!(
                                "terrain DEM fetch failed for {}; marking cell {} as nodata",
                                failed_request.url, cell.cell
                            );
                            handled = true;
                            break;
                        }
                    }
                }
                if !handled {
                    return Err(error);
                }
            }
        }
    }
}

pub(super) fn terrain_failed_fetch_url(message: &str) -> Option<String> {
    let start = message.find("curl failed for ")? + "curl failed for ".len();
    let rest = &message[start..];
    let end = rest.find(" with HTTP").or_else(|| rest.find('\n'))?;
    Some(rest[..end].to_string())
}

pub(super) fn terrain_dem_paths_from_requests(
    dem_dir: &Path,
    requests: &[PrefetchRequest],
) -> anyhow::Result<Vec<PathBuf>> {
    requests
        .iter()
        .map(|request| {
            let parsed_name = request
                .logical_file_name
                .as_deref()
                .or_else(|| request.url.rsplit('/').next())
                .context("terrain DEM request has no filename")?;
            let path = dem_dir.join(parsed_name);
            if !path.is_file() {
                bail!("terrain DEM download missing {}", path.display());
            }
            Ok(path)
        })
        .collect()
}

pub(super) fn terrain_source_fetched_at_utc(
    fetch_cache: &FetchCacheConfig,
    discovery_requests: &[PrefetchRequest],
    dem_requests: &[PrefetchRequest],
) -> anyhow::Result<Option<String>> {
    let layout = CacheLayout::new(&fetch_cache.root);
    let mut fetched_times = Vec::new();
    for request in discovery_requests.iter().chain(dem_requests.iter()) {
        let metadata_path = layout.http_metadata_path(&request.cache_key);
        if !metadata_path.is_file() {
            continue;
        }
        let value: serde_json::Value = serde_json::from_slice(
            &fs::read(&metadata_path)
                .with_context(|| format!("failed to read {}", metadata_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", metadata_path.display()))?;
        if let Some(fetched_at) = value.get("fetched_at_utc").and_then(|value| value.as_str()) {
            fetched_times.push(fetched_at.to_string());
            continue;
        }
        if let Ok(modified) = fs::metadata(&metadata_path).and_then(|metadata| metadata.modified())
        {
            fetched_times.push(
                DateTime::<Utc>::from(modified)
                    .format("%Y-%m-%dT%H:%M:%SZ")
                    .to_string(),
            );
        }
    }
    fetched_times.sort();
    Ok(fetched_times.into_iter().max())
}

pub(super) fn terrain_source_fingerprint(
    dem_urls: &[String],
    dem_paths: &[PathBuf],
    missing_cells: &[String],
    geoid_paths: Option<(&Path, &Path)>,
) -> anyhow::Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(if geoid_paths.is_some() {
        b"terrain-v2".as_slice()
    } else {
        b"terrain-v1".as_slice()
    });
    hasher.update(TERRAIN_ZOOM.to_string().as_bytes());
    if let Some((geoid_csv_path, geoid_metadata_path)) = geoid_paths {
        hasher.update(hash_file(geoid_csv_path)?.as_bytes());
        hasher.update([0]);
        hasher.update(hash_file(geoid_metadata_path)?.as_bytes());
        hasher.update([0xff]);
    }
    for url in dem_urls {
        hasher.update(url.as_bytes());
        hasher.update([0]);
    }
    for path in dem_paths {
        hasher.update(
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .as_bytes(),
        );
        hasher.update(hash_file(path)?.as_bytes());
    }
    for cell in missing_cells {
        hasher.update(b"missing");
        hasher.update([0]);
        hasher.update(cell.as_bytes());
        hasher.update([0xff]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(super) fn terrain_source_fingerprint_from_cached(
    dem_urls: &[String],
    sources: &[CachedTerrainDemSource],
    missing_cells: &[String],
    geoid_paths: Option<(&Path, &Path)>,
) -> anyhow::Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(if geoid_paths.is_some() {
        b"terrain-v2".as_slice()
    } else {
        b"terrain-v1".as_slice()
    });
    hasher.update(TERRAIN_ZOOM.to_string().as_bytes());
    if let Some((geoid_csv_path, geoid_metadata_path)) = geoid_paths {
        hasher.update(hash_file(geoid_csv_path)?.as_bytes());
        hasher.update([0]);
        hasher.update(hash_file(geoid_metadata_path)?.as_bytes());
        hasher.update([0xff]);
    }
    for url in dem_urls {
        hasher.update(url.as_bytes());
        hasher.update([0]);
    }
    for source in sources {
        hasher.update(source.filename.as_bytes());
        hasher.update(source.sha256.as_bytes());
    }
    for cell in missing_cells {
        hasher.update(b"missing");
        hasher.update([0]);
        hasher.update(cell.as_bytes());
        hasher.update([0xff]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(super) fn build_terrain_vrt(vrt_path: &Path, dem_paths: &[PathBuf]) -> anyhow::Result<()> {
    let mut command = Command::new("gdalbuildvrt");
    command.arg("-overwrite").arg(vrt_path);
    for path in dem_paths {
        command.arg(path);
    }
    let status = command
        .status()
        .with_context(|| format!("failed to run gdalbuildvrt for {}", vrt_path.display()))?;
    if !status.success() {
        bail!("gdalbuildvrt failed for {}", vrt_path.display());
    }
    Ok(())
}

pub(super) fn build_terrain_region_tiles(
    region: Region,
    vrt_path: &Path,
    geoid_csv_path: &Path,
    geoid_metadata_path: &Path,
    output_dir: &Path,
    version_label: &str,
    dem_selection: &TerrainDemSelection,
) -> anyhow::Result<()> {
    let script_path = output_dir.join("build_terrain_tiles.py");
    fs::write(&script_path, TERRAIN_TILE_SCRIPT)
        .with_context(|| format!("failed to write {}", script_path.display()))?;
    let bounds = region.bounds();
    let mut command = Command::new("python3");
    let output = command
        .arg(&script_path)
        .arg("--vrt")
        .arg(vrt_path)
        .arg("--geoid-csv")
        .arg(geoid_csv_path)
        .arg("--geoid-metadata")
        .arg(geoid_metadata_path)
        .arg("--output-dir")
        .arg(output_dir)
        .arg("--region")
        .arg(region.code())
        .arg(format!(
            "--bbox={},{},{},{}",
            bounds.lon_min, bounds.lat_min, bounds.lon_max, bounds.lat_max
        ))
        .arg("--zoom")
        .arg(TERRAIN_ZOOM.to_string())
        .arg("--tile-size")
        .arg(TERRAIN_TILE_SIZE.to_string())
        .arg("--height-quantization-ft")
        .arg(TERRAIN_TER2_HEIGHT_QUANTIZATION_FT.to_string())
        .arg("--version-label")
        .arg(version_label)
        .arg("--source-count")
        .arg(dem_selection.urls.len().to_string())
        .arg("--missing-cells")
        .arg(dem_selection.missing_cells.join(","))
        .arg("--workers")
        .arg(TERRAIN_TILE_WORKERS.to_string())
        .output()
        .with_context(|| format!("failed to run {}", script_path.display()))?;
    if !output.status.success() {
        bail!(
            "terrain tile builder failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

pub(super) fn water_mask_query_url(layer: u32, params: &[(&str, String)]) -> String {
    let query = params
        .iter()
        .map(|(key, value)| format!("{key}={}", percent_encode_query_value(value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{WATER_MASK_NHD_SERVICE}/{layer}/query?{query}")
}

pub(super) fn percent_encode_query_value(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

pub(super) fn water_mask_ids_request(
    layer: u32,
    bbox: &str,
    where_clause: &str,
) -> PrefetchRequest {
    let url = water_mask_query_url(
        layer,
        &[
            ("where", where_clause.to_string()),
            ("geometry", bbox.to_string()),
            ("geometryType", "esriGeometryEnvelope".to_string()),
            ("inSR", "4326".to_string()),
            ("spatialRel", "esriSpatialRelIntersects".to_string()),
            ("returnIdsOnly", "true".to_string()),
            ("f", "json".to_string()),
        ],
    );
    let logical_file_name = format!("layer_{layer}_ids.json");
    PrefetchRequest::new(&url)
        .with_logical_file_name(&logical_file_name)
        .with_cache_key(format!("{url}#logical_name={logical_file_name}"))
}

#[derive(Debug, Clone)]
pub(super) struct WaterMaskPageRequest {
    layer: u32,
    label: String,
    object_ids: Vec<u64>,
}

impl WaterMaskPageRequest {
    fn file_name(&self) -> String {
        format!("layer_{}_chunk_{}.geojson", self.layer, self.label)
    }

    fn request(&self) -> PrefetchRequest {
        water_mask_page_request(self.layer, &self.label, &self.object_ids)
    }
}

pub(super) fn water_mask_page_request(
    layer: u32,
    page_label: &str,
    object_ids: &[u64],
) -> PrefetchRequest {
    let url = water_mask_query_url(
        layer,
        &[
            (
                "objectIds",
                object_ids
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            ("outFields", "FTYPE,FCODE,GNIS_NAME".to_string()),
            ("outSR", "4326".to_string()),
            ("returnGeometry", "true".to_string()),
            ("geometryPrecision", "6".to_string()),
            ("f", "geojson".to_string()),
            ("orderByFields", "OBJECTID".to_string()),
        ],
    );
    let logical_file_name = format!("layer_{layer}_chunk_{page_label}.geojson");
    PrefetchRequest::new(&url)
        .with_logical_file_name(&logical_file_name)
        .with_cache_key(format!("{url}#logical_name={logical_file_name}"))
}

pub(super) fn water_mask_cached_source_dir(
    config: &ProductBuildConfig,
    region: Region,
    output_dir: &Path,
) -> anyhow::Result<PathBuf> {
    let region_id = region.code().to_ascii_lowercase();
    let bounds = region.bounds();
    let bbox = format!(
        "{},{},{},{}",
        bounds.lon_min, bounds.lat_min, bounds.lon_max, bounds.lat_max
    );
    let source_dir = output_dir.join("source-pages");
    fs::create_dir_all(&source_dir)
        .with_context(|| format!("failed to create {}", source_dir.display()))?;
    let provenance_dir = config
        .build_root
        .join("meta")
        .join("provenance")
        .join(format!("water-mask-{region_id}"));
    let fetch_cache = static_source_fetch_cache_config(config)?;
    let ids_requests = WATER_MASK_NHD_LAYERS
        .iter()
        .map(|(layer, _name, where_clause)| water_mask_ids_request(*layer, &bbox, where_clause))
        .collect::<Vec<_>>();
    prefetch_water_mask_source_requests(
        &ids_requests,
        &source_dir,
        &provenance_dir,
        &format!("water-mask-{region_id}-ids"),
        &fetch_cache,
    )?;

    let mut page_requests = Vec::new();
    for (layer, _name, _where_clause) in WATER_MASK_NHD_LAYERS {
        let ids_path = source_dir.join(format!("layer_{layer}_ids.json"));
        let value: serde_json::Value = serde_json::from_slice(
            &fs::read(&ids_path)
                .with_context(|| format!("failed to read {}", ids_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", ids_path.display()))?;
        let mut object_ids = value
            .get("objectIds")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_u64())
            .collect::<Vec<_>>();
        object_ids.sort_unstable();
        for (chunk_index, chunk) in object_ids.chunks(WATER_MASK_PAGE_SIZE).enumerate() {
            page_requests.push(WaterMaskPageRequest {
                layer: *layer,
                label: format!("{chunk_index:05}"),
                object_ids: chunk.to_vec(),
            });
        }
    }
    prefetch_water_mask_source_pages(
        &page_requests,
        &source_dir,
        &provenance_dir,
        &format!("water-mask-{region_id}-page"),
        &fetch_cache,
    )?;
    Ok(source_dir)
}

pub(super) fn prefetch_water_mask_source_requests(
    requests: &[PrefetchRequest],
    source_dir: &Path,
    provenance_dir: &Path,
    label: &str,
    fetch_cache: &FetchCacheConfig,
) -> anyhow::Result<()> {
    prefetch_archives_with_provenance(
        requests,
        source_dir,
        WATER_MASK_FETCH_WORKERS as usize,
        Some(fetch_cache),
        provenance_dir,
        label,
    )
}

pub(super) fn prefetch_water_mask_source_pages(
    pages: &[WaterMaskPageRequest],
    source_dir: &Path,
    provenance_dir: &Path,
    label: &str,
    fetch_cache: &FetchCacheConfig,
) -> anyhow::Result<()> {
    let mut split_page_fetches = 0usize;
    let mut omitted_objects = Vec::new();
    let requests = pages
        .iter()
        .map(WaterMaskPageRequest::request)
        .collect::<Vec<_>>();
    if prefetch_water_mask_source_requests(
        &requests,
        source_dir,
        provenance_dir,
        label,
        fetch_cache,
    )
    .is_err()
    {
        for page in pages {
            prefetch_water_mask_source_page_split(
                page,
                source_dir,
                provenance_dir,
                label,
                fetch_cache,
                &mut split_page_fetches,
                &mut omitted_objects,
            )?;
        }
    }
    if !omitted_objects.is_empty() {
        eprintln!(
            "water mask omitted {} persistent failing NHD object(s): {:?}",
            omitted_objects.len(),
            omitted_objects
        );
    }
    Ok(())
}

pub(super) fn prefetch_water_mask_source_page_split(
    page: &WaterMaskPageRequest,
    source_dir: &Path,
    provenance_dir: &Path,
    label: &str,
    fetch_cache: &FetchCacheConfig,
    split_page_fetches: &mut usize,
    omitted_objects: &mut Vec<u64>,
) -> anyhow::Result<()> {
    let requests = [page.request()];
    match prefetch_water_mask_source_requests(
        &requests,
        source_dir,
        provenance_dir,
        label,
        fetch_cache,
    ) {
        Ok(()) => return Ok(()),
        Err(error) => {
            if page.object_ids.len() > 1 {
                if *split_page_fetches >= WATER_MASK_MAX_SPLIT_SOURCE_PAGES {
                    bail!(
                        "water mask source page splitting exceeded {} split pages after failure: {error}",
                        WATER_MASK_MAX_SPLIT_SOURCE_PAGES
                    );
                }
                let midpoint = page.object_ids.len() / 2;
                let split_pages = [
                    WaterMaskPageRequest {
                        layer: page.layer,
                        label: format!("{}_a", page.label),
                        object_ids: page.object_ids[..midpoint].to_vec(),
                    },
                    WaterMaskPageRequest {
                        layer: page.layer,
                        label: format!("{}_b", page.label),
                        object_ids: page.object_ids[midpoint..].to_vec(),
                    },
                ];
                *split_page_fetches += split_pages.len();
                for split_page in split_pages {
                    prefetch_water_mask_source_page_split(
                        &split_page,
                        source_dir,
                        provenance_dir,
                        label,
                        fetch_cache,
                        split_page_fetches,
                        omitted_objects,
                    )
                    .with_context(|| {
                        format!(
                            "failed while splitting water mask page {} after: {error}",
                            page.file_name()
                        )
                    })?;
                }
                return Ok(());
            }
            if omitted_objects.len() >= WATER_MASK_MAX_OMITTED_OBJECTS {
                bail!(
                    "water mask source omitted object cap exceeded after persistent failure for {}: {error}",
                    page.file_name()
                );
            }
            omitted_objects.push(page.object_ids[0]);
            write_empty_water_mask_page(source_dir, page).with_context(|| {
                format!(
                    "wrote empty water mask page for persistent failing object {} after: {error}",
                    page.object_ids[0]
                )
            })?;
            Ok(())
        }
    }
}

pub(super) fn write_empty_water_mask_page(
    source_dir: &Path,
    page: &WaterMaskPageRequest,
) -> anyhow::Result<()> {
    let path = source_dir.join(page.file_name());
    let value = serde_json::json!({
        "type": "FeatureCollection",
        "features": [],
    });
    fs::write(
        &path,
        serde_json::to_vec(&value).context("failed to encode empty water mask page")?,
    )
    .with_context(|| format!("failed to write empty water mask page {}", path.display()))?;
    Ok(())
}

pub(super) fn build_water_mask_region_tiles(
    region: Region,
    output_dir: &Path,
    source_dir: &Path,
) -> anyhow::Result<()> {
    let script_path = water_mask_tile_script_path();
    let bounds = region.bounds();
    let mut command = Command::new("python3");
    let output = command
        .arg(&script_path)
        .arg("--output-dir")
        .arg(output_dir)
        .arg("--region")
        .arg(region.code())
        .arg(format!(
            "--bbox={},{},{},{}",
            bounds.lon_min, bounds.lat_min, bounds.lon_max, bounds.lat_max
        ))
        .arg("--zoom")
        .arg(TERRAIN_ZOOM.to_string())
        .arg("--tile-size")
        .arg(TERRAIN_TILE_SIZE.to_string())
        .arg("--source-dir")
        .arg(source_dir)
        .arg("--fetch-workers")
        .arg(WATER_MASK_FETCH_WORKERS.to_string())
        .arg("--tile-workers")
        .arg(WATER_MASK_TILE_WORKERS.to_string())
        .output()
        .with_context(|| format!("failed to run {}", script_path.display()))?;
    if !output.status.success() {
        bail!(
            "water mask tile builder failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

pub(super) fn build_shaded_relief_region_tiles(
    region: Region,
    vrt_path: &Path,
    output_dir: &Path,
    version_label: &str,
    dem_selection: &TerrainDemSelection,
    water_mask_tiles_dir: &Path,
    state_borders_shp: &Path,
    primary_roads_shp: &Path,
    draw_low_zoom_overlays: bool,
) -> anyhow::Result<()> {
    let script_path = shaded_relief_tile_script_path();
    let bounds = region.bounds();
    let mut command = Command::new("python3");
    command
        .arg(&script_path)
        .arg("--vrt")
        .arg(vrt_path)
        .arg("--output-dir")
        .arg(output_dir)
        .arg("--region")
        .arg(region.code())
        .arg(format!(
            "--bbox={},{},{},{}",
            bounds.lon_min, bounds.lat_min, bounds.lon_max, bounds.lat_max
        ))
        .arg("--zoom")
        .arg(TERRAIN_ZOOM.to_string())
        .arg("--tile-size")
        .arg(TERRAIN_TILE_SIZE.to_string())
        .arg("--version-label")
        .arg(version_label)
        .arg("--source-count")
        .arg(dem_selection.urls.len().to_string())
        .arg("--missing-cells")
        .arg(dem_selection.missing_cells.join(","))
        .arg("--water-mask-dir")
        .arg(water_mask_tiles_dir)
        .arg("--state-borders-shp")
        .arg(state_borders_shp)
        .arg("--primary-roads-shp")
        .arg(primary_roads_shp)
        .arg("--overlay-style-version")
        .arg(SHADED_RELIEF_OVERLAY_STYLE_VERSION)
        .arg("--workers")
        .arg(SHADED_RELIEF_TILE_WORKERS.to_string());
    if draw_low_zoom_overlays {
        command.arg("--draw-low-zoom-overlays");
    }
    let output = command
        .output()
        .with_context(|| format!("failed to run {}", script_path.display()))?;
    if !output.status.success() {
        bail!(
            "shaded relief tile builder failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

pub(super) fn shaded_relief_tile_script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("build_shaded_relief_tiles.py")
}

pub(super) fn water_mask_tile_script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("build_water_mask_tiles.py")
}

pub(super) fn zip_directory_deterministic(
    zip_path: &Path,
    root: &Path,
    entries: &[&str],
) -> anyhow::Result<()> {
    let mut files = Vec::new();
    for entry in entries {
        collect_zip_files(root, &root.join(entry), &mut files)?;
    }
    let members = files
        .into_iter()
        .map(|(name, path)| {
            let source = ZipSource::new(name.clone(), path);
            if name.ends_with(".terrain") || name.ends_with(".png") || name.ends_with(".webp") {
                source.stored()
            } else {
                source
            }
        })
        .collect::<Vec<_>>();
    write_deterministic_zip(zip_path, &members)
}

pub(super) fn collect_zip_files(
    root: &Path,
    path: &Path,
    files: &mut Vec<(String, PathBuf)>,
) -> anyhow::Result<()> {
    if path.is_file() {
        let relative = path
            .strip_prefix(root)
            .with_context(|| format!("failed to relativize {}", path.display()))?
            .to_string_lossy()
            .replace('\\', "/");
        files.push((relative, path.to_path_buf()));
        return Ok(());
    }
    for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
        let entry = entry?;
        collect_zip_files(root, &entry.path(), files)?;
    }
    Ok(())
}

const WORLD_BASEMAP_TILE_SCRIPT: &str = r#"
import argparse
import json
import math
from pathlib import Path

from osgeo import ogr
from PIL import Image, ImageDraw

RADIUS = 6378137.0
ORIGIN_SHIFT = math.pi * RADIUS
WEB_MERCATOR_LIMIT = 85.05112878
OCEAN = (187, 207, 218)
LAND = (226, 226, 212)
BOUNDARY = (148, 148, 128)

def mercator(lon, lat):
    lat = max(min(lat, WEB_MERCATOR_LIMIT), -WEB_MERCATOR_LIMIT)
    mx = lon * ORIGIN_SHIFT / 180.0
    my = math.log(math.tan((90.0 + lat) * math.pi / 360.0)) * RADIUS
    return mx, my

def tile_bounds(x, y_xyz, z, tile_size):
    resolution = (2.0 * math.pi * RADIUS) / (tile_size * (2 ** z))
    minx = x * tile_size * resolution - ORIGIN_SHIFT
    maxx = (x + 1) * tile_size * resolution - ORIGIN_SHIFT
    maxy = ORIGIN_SHIFT - y_xyz * tile_size * resolution
    miny = ORIGIN_SHIFT - (y_xyz + 1) * tile_size * resolution
    return minx, miny, maxx, maxy

def pixel_for_lonlat(lon, lat, bounds, tile_size):
    mx, my = mercator(lon, lat)
    minx, miny, maxx, maxy = bounds
    return (
        (mx - minx) / (maxx - minx) * tile_size,
        (maxy - my) / (maxy - miny) * tile_size,
    )

def load_geometries(path):
    dataset = ogr.Open(str(path))
    if dataset is None:
        raise RuntimeError(f'failed to open {path}')
    layer = dataset.GetLayer(0)
    geometries = []
    for feature in layer:
        geom = feature.GetGeometryRef()
        if geom is not None:
            geometries.append(geom.Clone())
    return geometries

def ring_points(ring, bounds, tile_size):
    points = []
    for index in range(ring.GetPointCount()):
        lon, lat, _z = ring.GetPoint(index)
        points.append(pixel_for_lonlat(lon, lat, bounds, tile_size))
    return points

def draw_polygon(draw, geom, bounds, tile_size):
    if geom.GetGeometryName().upper() == 'MULTIPOLYGON':
        for index in range(geom.GetGeometryCount()):
            draw_polygon(draw, geom.GetGeometryRef(index), bounds, tile_size)
        return
    if geom.GetGeometryName().upper() != 'POLYGON':
        return
    if geom.GetGeometryCount() == 0:
        return
    exterior = ring_points(geom.GetGeometryRef(0), bounds, tile_size)
    if len(exterior) >= 3:
        draw.polygon(exterior, fill=LAND)
    for ring_index in range(1, geom.GetGeometryCount()):
        hole = ring_points(geom.GetGeometryRef(ring_index), bounds, tile_size)
        if len(hole) >= 3:
            draw.polygon(hole, fill=OCEAN)

def draw_line(draw, geom, bounds, tile_size, width):
    name = geom.GetGeometryName().upper()
    if name == 'MULTILINESTRING':
        for index in range(geom.GetGeometryCount()):
            draw_line(draw, geom.GetGeometryRef(index), bounds, tile_size, width)
        return
    if name != 'LINESTRING':
        return
    points = ring_points(geom, bounds, tile_size)
    if len(points) >= 2:
        draw.line(points, fill=BOUNDARY, width=width, joint='curve')

def render_tile(land_geoms, boundary_geoms, output_path, x, y_xyz, z, tile_size):
    bounds = tile_bounds(x, y_xyz, z, tile_size)
    image = Image.new('RGB', (tile_size, tile_size), OCEAN)
    draw = ImageDraw.Draw(image)
    for geom in land_geoms:
        draw_polygon(draw, geom, bounds, tile_size)
    line_width = 1 if z < 3 else 2
    for geom in boundary_geoms:
        draw_line(draw, geom, bounds, tile_size, line_width)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    image.save(output_path, 'PNG', optimize=True)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--land-shp', required=True)
    parser.add_argument('--boundaries-shp', required=True)
    parser.add_argument('--output-dir', required=True)
    parser.add_argument('--version-label', required=True)
    parser.add_argument('--min-zoom', type=int, required=True)
    parser.add_argument('--max-source-zoom', type=int, required=True)
    parser.add_argument('--max-display-zoom', type=float, required=True)
    parser.add_argument('--tile-size', type=int, required=True)
    parser.add_argument('--source-fetched-at-utc')
    args = parser.parse_args()

    output_dir = Path(args.output_dir)
    land_geoms = load_geometries(Path(args.land_shp))
    boundary_geoms = load_geometries(Path(args.boundaries_shp))
    levels = []
    for z in range(args.min_zoom, args.max_source_zoom + 1):
        tiles_per_side = 1 << z
        for x in range(tiles_per_side):
            for y_xyz in range(tiles_per_side):
                y_tms = tiles_per_side - 1 - y_xyz
                output_path = output_dir / 'tiles' / '0' / str(z) / str(x) / f'{y_tms}.png'
                render_tile(land_geoms, boundary_geoms, output_path, x, y_xyz, z, args.tile_size)
        levels.append({
            'zoom': z,
            'tile_count': tiles_per_side * tiles_per_side,
            'boxes': [{
                'x_min': 0,
                'x_max': tiles_per_side - 1,
                'y_tms_min': 0,
                'y_tms_max': tiles_per_side - 1,
            }],
        })

    manifest = {
        'schema_version': 1,
        'product': 'world-basemap',
        'version_label': args.version_label,
        'source': 'Natural Earth 110m land and admin-0 boundary lines',
        'source_urls': [
            'https://naturalearth.s3.amazonaws.com/110m_physical/ne_110m_land.zip',
            'https://naturalearth.s3.amazonaws.com/110m_cultural/ne_110m_admin_0_boundary_lines_land.zip',
        ],
        'source_fetched_at_utc': args.source_fetched_at_utc,
        'license': 'public-domain',
        'attribution': 'Made with Natural Earth. Free vector and raster map data @ naturalearthdata.com.',
        'min_zoom': args.min_zoom,
        'max_source_zoom': args.max_source_zoom,
        'max_display_zoom': args.max_display_zoom,
        'tile_size': args.tile_size,
        'tile_format': 'png',
        'tile_path_template': 'tiles/0/{z}/{x}/{y}.png',
        'levels': levels,
    }
    (output_dir / 'manifest.json').write_text(json.dumps(manifest, indent=2, sort_keys=True) + '\n')

if __name__ == '__main__':
    main()
"#;

const SHADED_RELIEF_WIDE_TILE_SCRIPT: &str = r#"
import argparse
import json
import math
from collections import defaultdict
from pathlib import Path

from osgeo import ogr
from PIL import Image, ImageDraw

RASTER_TILE_SUFFIXES = {'.webp'}
RADIUS = 6378137.0
ORIGIN_SHIFT = math.pi * RADIUS
STATE_BORDER_RGBA = (128, 128, 128, 204)
PRIMARY_ROAD_RGBA = (91, 111, 122, 153)

def mercator(lon, lat):
    lat = max(min(lat, 85.05112878), -85.05112878)
    mx = lon * ORIGIN_SHIFT / 180.0
    my = math.log(math.tan((90.0 + lat) * math.pi / 360.0)) * RADIUS
    return mx, my

def tile_bounds(x, y, z, tile_size):
    resolution = ((2.0 * math.pi * RADIUS) / tile_size) / (2 ** z)
    minx = x * tile_size * resolution - ORIGIN_SHIFT
    maxx = (x + 1) * tile_size * resolution - ORIGIN_SHIFT
    miny = y * tile_size * resolution - ORIGIN_SHIFT
    maxy = (y + 1) * tile_size * resolution - ORIGIN_SHIFT
    return minx, miny, maxx, maxy

def pixel_for_lonlat(lon, lat, bounds, tile_size):
    mx, my = mercator(lon, lat)
    minx, miny, maxx, maxy = bounds
    return ((mx - minx) / (maxx - minx) * tile_size, (maxy - my) / (maxy - miny) * tile_size)

def scan_sources(source_dirs, max_zoom):
    groups = defaultdict(list)
    for source_dir in source_dirs:
        tiles_root = Path(source_dir) / 'tiles' / '0'
        if not tiles_root.exists():
            continue
        for z_dir in tiles_root.iterdir():
            if not z_dir.is_dir():
                continue
            try:
                z = int(z_dir.name)
            except ValueError:
                continue
            if z > max_zoom:
                continue
            for x_dir in z_dir.iterdir():
                if not x_dir.is_dir():
                    continue
                for tile_path in x_dir.iterdir():
                    if tile_path.suffix.lower() in RASTER_TILE_SUFFIXES:
                        rel = tile_path.relative_to(Path(source_dir)).as_posix()
                        groups[rel].append(tile_path)
    return groups

def composite_group(paths, output_path):
    base = Image.new('RGBA', Image.open(paths[0]).size, (0, 0, 0, 0))
    for path in paths:
        tile = Image.open(path).convert('RGBA')
        base.alpha_composite(tile)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    base.save(output_path, format='WEBP', quality=75, method=4, exact=True, alpha_quality=100)

def scan_levels(output_dir):
    levels = []
    tiles_root = Path(output_dir) / 'tiles' / '0'
    for z_dir in sorted((path for path in tiles_root.iterdir() if path.is_dir()), key=lambda path: int(path.name)):
        zoom = int(z_dir.name)
        coords = []
        for x_dir in z_dir.iterdir():
            if not x_dir.is_dir():
                continue
            x = int(x_dir.name)
            for tile_path in x_dir.iterdir():
                if tile_path.suffix.lower() != '.webp':
                    continue
                coords.append((x, int(tile_path.stem)))
        if not coords:
            continue
        xs = [x for x, _ in coords]
        ys = [y for _, y in coords]
        levels.append({
            'zoom': zoom,
            'tile_count': len(coords),
            'boxes': [{
                'x_min': min(xs),
                'x_max': max(xs),
                'y_tms_min': min(ys),
                'y_tms_max': max(ys),
            }],
        })
    return levels

def load_line_geometries(path):
    dataset = ogr.Open(str(path))
    if dataset is None:
        raise RuntimeError(f'failed to open {path}')
    layer = dataset.GetLayer(0)
    lines = []
    for feature in layer:
        geom = feature.GetGeometryRef()
        if geom is not None:
            for line in iter_lines(geom):
                points = [(line.GetX(i), line.GetY(i)) for i in range(line.GetPointCount())]
                for segment in split_discontinuous_line(points):
                    if len(segment) >= 2:
                        lines.append(segment)
    return lines

def iter_lines(geom):
    name = geom.GetGeometryName().upper()
    if name == 'LINESTRING':
        yield geom
    elif name in ('MULTILINESTRING', 'GEOMETRYCOLLECTION'):
        for index in range(geom.GetGeometryCount()):
            yield from iter_lines(geom.GetGeometryRef(index))

def split_discontinuous_line(points, max_jump_degrees=10.0):
    current = []
    previous = None
    for point in points:
        if previous is not None and (
            abs(point[0] - previous[0]) > max_jump_degrees
            or abs(point[1] - previous[1]) > max_jump_degrees
        ):
            if len(current) >= 2:
                yield current
            current = []
        current.append(point)
        previous = point
    if len(current) >= 2:
        yield current

def draw_dashed_line(draw, points, fill, width, dash=8, gap=6):
    for start, end in zip(points, points[1:]):
        x0, y0 = start
        x1, y1 = end
        dx = x1 - x0
        dy = y1 - y0
        length = math.hypot(dx, dy)
        if length <= 0:
            continue
        distance = 0.0
        while distance < length:
            dash_end = min(distance + dash, length)
            draw.line([
                (x0 + dx * (distance / length), y0 + dy * (distance / length)),
                (x0 + dx * (dash_end / length), y0 + dy * (dash_end / length)),
            ], fill=fill, width=width)
            distance += dash + gap

def offset_polyline(points, offset):
    shifted = []
    for index, (x, y) in enumerate(points):
        normals = []
        if index > 0:
            px, py = points[index - 1]
            dx = x - px
            dy = y - py
            length = math.hypot(dx, dy)
            if length > 0:
                normals.append((-dy / length, dx / length))
        if index + 1 < len(points):
            nx, ny = points[index + 1]
            dx = nx - x
            dy = ny - y
            length = math.hypot(dx, dy)
            if length > 0:
                normals.append((-dy / length, dx / length))
        if normals:
            ox = sum(normal[0] for normal in normals) / len(normals)
            oy = sum(normal[1] for normal in normals) / len(normals)
            length = math.hypot(ox, oy)
            if length > 0:
                shifted.append((x + ox / length * offset, y + oy / length * offset))
                continue
        shifted.append((x, y))
    return shifted

def draw_paired_line(draw, points, fill, width, separation):
    offset = separation / 2.0
    draw.line(offset_polyline(points, -offset), fill=fill, width=width)
    draw.line(offset_polyline(points, offset), fill=fill, width=width)

def line_tile_range(points, z, tile_size):
    lons = [lon for lon, _lat in points]
    lats = [lat for _lon, lat in points]
    resolution = ((2.0 * math.pi * RADIUS) / tile_size) / (2 ** z)
    west_m, south_m = mercator(min(lons), min(lats))
    east_m, north_m = mercator(max(lons), max(lats))
    x0 = math.floor((west_m + ORIGIN_SHIFT) / resolution / tile_size)
    x1 = math.floor((east_m + ORIGIN_SHIFT) / resolution / tile_size)
    y0 = math.floor((south_m + ORIGIN_SHIFT) / resolution / tile_size)
    y1 = math.floor((north_m + ORIGIN_SHIFT) / resolution / tile_size)
    limit = (2 ** z) - 1
    x0 = max(0, min(x0, limit))
    x1 = max(0, min(x1, limit))
    y0 = max(0, min(y0, limit))
    y1 = max(0, min(y1, limit))
    return range(x0, x1 + 1), range(y0, y1 + 1)

def build_overlay_index(lines, min_zoom, max_zoom, tile_size):
    index = {}
    for points in lines:
        for z in range(min_zoom, max_zoom + 1):
            x_range, y_range = line_tile_range(points, z, tile_size)
            for x in x_range:
                for y in y_range:
                    index.setdefault((z, x, y), []).append(points)
    return index

def draw_geometries(draw, lines, bounds, tile_size, z, style):
    margin = 24
    for lonlat_points in lines:
        points = [pixel_for_lonlat(lon, lat, bounds, tile_size) for lon, lat in lonlat_points]
        if len(points) < 2:
            continue
        if not any(-margin <= x <= tile_size + margin and -margin <= y <= tile_size + margin for x, y in points):
            continue
        if style == 'state-border':
            draw_dashed_line(draw, points, STATE_BORDER_RGBA, 1)
        else:
            draw_paired_line(draw, points, PRIMARY_ROAD_RGBA, 1, 2)

def draw_overlays(output_dir, max_zoom, tile_size, state_borders_shp, primary_roads_shp):
    state_index = build_overlay_index(load_line_geometries(state_borders_shp), 0, max_zoom, tile_size)
    road_index = build_overlay_index(load_line_geometries(primary_roads_shp), 0, max_zoom, tile_size)
    tiles_root = Path(output_dir) / 'tiles' / '0'
    for z, x, y in sorted(set(state_index) | set(road_index)):
        tile_path = tiles_root / str(z) / str(x) / f'{y}.webp'
        if not tile_path.exists():
            continue
        image = Image.open(tile_path).convert('RGBA')
        draw = ImageDraw.Draw(image, 'RGBA')
        bounds = tile_bounds(x, y, z, tile_size)
        key = (z, x, y)
        draw_geometries(draw, road_index.get(key, []), bounds, tile_size, z, 'primary-road')
        draw_geometries(draw, state_index.get(key, []), bounds, tile_size, z, 'state-border')
        image.save(tile_path, format='WEBP', quality=75, method=4, exact=True, alpha_quality=100)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--source-dir', action='append', required=True)
    parser.add_argument('--output-dir', required=True)
    parser.add_argument('--version-label', required=True)
    parser.add_argument('--source-fingerprint', required=True)
    parser.add_argument('--state-borders-shp', required=True)
    parser.add_argument('--primary-roads-shp', required=True)
    parser.add_argument('--overlay-style-version', required=True)
    parser.add_argument('--max-zoom', required=True, type=int)
    parser.add_argument('--tile-size', required=True, type=int)
    args = parser.parse_args()

    output_dir = Path(args.output_dir)
    groups = scan_sources(args.source_dir, args.max_zoom)
    for rel, paths in sorted(groups.items()):
        composite_group(paths, output_dir / rel)
    draw_overlays(output_dir, args.max_zoom, args.tile_size, args.state_borders_shp, args.primary_roads_shp)
    levels = scan_levels(output_dir)
    if not levels:
        raise SystemExit('no shaded relief wide-angle tiles were produced')
    manifest = {
        'schema_version': 1,
        'product': 'shaded-relief',
        'region': 'wide',
        'version_label': args.version_label,
        'source_fingerprint': args.source_fingerprint,
        'min_zoom': min(level['zoom'] for level in levels),
        'max_zoom': args.max_zoom,
        'base_zoom': args.max_zoom,
        'tile_size': args.tile_size,
        'tile_format': 'webp_rgba',
        'tile_content_encoding': 'identity',
        'zip_member_compression': 'stored_webp',
        'wide_angle': True,
        'wide_angle_max_zoom': args.max_zoom,
        'source_policy': 'alpha-composite matching low-zoom tiles from regional shaded-relief products',
        'overlays': {
            'style_version': args.overlay_style_version,
            'state_borders': 'Natural Earth 50m admin-1 boundary lines, dashed 80% gray',
            'primary_roads': 'U.S. Census TIGER/Line 2025 national primary roads, 60% blue-gray paired strokes',
        },
        'source_region_count': len(args.source_dir),
        'levels': levels,
        'files': {'tiles': 'tiles'},
    }
    (output_dir / 'manifest.json').write_text(json.dumps(manifest, indent=2, sort_keys=True) + '\n')

if __name__ == '__main__':
    main()
"#;

const TERRAIN_WIDE_TILE_SCRIPT: &str = r#"
import argparse
import gzip
import json
import struct
from collections import defaultdict
from pathlib import Path

import numpy as np

TERRAIN_TILE_SUFFIXES = {'.terrain'}
NODATA = -32768
MAGIC = b'ABT2'

def encode_gradient_delta(samples):
    raw = samples.astype('<i2', copy=False).view('<u2').astype(np.uint32)
    prediction = np.zeros(raw.shape, dtype=np.uint32)
    prediction[0, 1:] = raw[0, :-1]
    prediction[1:, 0] = raw[:-1, 0]
    prediction[1:, 1:] = raw[1:, :-1] + raw[:-1, 1:] - raw[:-1, :-1]
    return ((raw - prediction) & 0xffff).astype('<u2')

def decode_gradient_delta(payload, tile_size):
    residual = np.frombuffer(payload, dtype='<u2').reshape((tile_size, tile_size)).astype(np.uint32)
    raw = residual.cumsum(axis=0, dtype=np.uint32).cumsum(axis=1, dtype=np.uint32)
    return (raw & 0xffff).astype('<u2').view('<i2')

def read_tile(path, tile_size, height_quantization_ft):
    with gzip.open(path, 'rb') as f:
        raw = f.read()
    if raw[:4] != MAGIC:
        raise ValueError(f'{path} is not an ABT2 terrain tile')
    width, height, nodata, _reserved, scale, offset = struct.unpack('<HHhhff', raw[4:20])
    if width != tile_size or height != tile_size or nodata != NODATA or scale != float(height_quantization_ft) or offset != 0.0:
        raise ValueError(f'{path} has unexpected terrain header')
    expected_bytes = 20 + tile_size * tile_size * 2
    if len(raw) != expected_bytes:
        raise ValueError(f'{path} has unexpected terrain payload length')
    samples = decode_gradient_delta(raw[20:], tile_size)
    return scale, offset, samples

def write_tile(path, samples, tile_size, height_quantization_ft):
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = encode_gradient_delta(samples)
    raw = MAGIC + struct.pack('<HHhhff', tile_size, tile_size, NODATA, 0, float(height_quantization_ft), 0.0) + payload.tobytes()
    with open(path, 'wb') as f:
        f.write(gzip.compress(raw, mtime=0))

def scan_sources(source_dirs, max_zoom):
    groups = defaultdict(list)
    for source_dir in source_dirs:
        tiles_root = Path(source_dir) / 'tiles'
        if not tiles_root.exists():
            continue
        for z_dir in tiles_root.iterdir():
            if not z_dir.is_dir():
                continue
            try:
                z = int(z_dir.name)
            except ValueError:
                continue
            if z > max_zoom:
                continue
            for x_dir in z_dir.iterdir():
                if not x_dir.is_dir():
                    continue
                for tile_path in x_dir.iterdir():
                    if tile_path.suffix.lower() in TERRAIN_TILE_SUFFIXES:
                        rel = tile_path.relative_to(Path(source_dir)).as_posix()
                        groups[rel].append(tile_path)
    return groups

def composite_group(paths, output_path, tile_size, height_quantization_ft):
    first_scale, first_offset, first_samples = read_tile(paths[0], tile_size, height_quantization_ft)
    composite = np.full(first_samples.shape, NODATA, dtype='<i2')
    composite_elevations = np.full(first_samples.shape, -np.inf, dtype=np.float64)
    for path in paths:
        scale, offset, samples = read_tile(path, tile_size, height_quantization_ft)
        valid = samples != NODATA
        elevations = samples.astype(np.float64) * scale + offset
        better = valid & (elevations > composite_elevations)
        encoded = np.ceil((elevations - first_offset) / first_scale)
        encoded = np.clip(encoded, np.iinfo(np.int16).min, np.iinfo(np.int16).max).astype('<i2')
        composite[better] = encoded[better]
        composite_elevations[better] = elevations[better]
    write_tile(output_path, composite, tile_size, height_quantization_ft)

def scan_levels(output_dir):
    levels = []
    tiles_root = Path(output_dir) / 'tiles'
    for z_dir in sorted((path for path in tiles_root.iterdir() if path.is_dir()), key=lambda path: int(path.name)):
        zoom = int(z_dir.name)
        coords = []
        for x_dir in z_dir.iterdir():
            if not x_dir.is_dir():
                continue
            x = int(x_dir.name)
            for tile_path in x_dir.iterdir():
                if tile_path.suffix.lower() != '.terrain':
                    continue
                coords.append((x, int(tile_path.stem)))
        if not coords:
            continue
        xs = [x for x, _ in coords]
        ys = [y for _, y in coords]
        levels.append({
            'zoom': zoom,
            'tile_count': len(coords),
            'boxes': [{
                'x_min': min(xs),
                'x_max': max(xs),
                'y_tms_min': min(ys),
                'y_tms_max': max(ys),
            }],
        })
    return levels

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--source-dir', action='append', required=True)
    parser.add_argument('--output-dir', required=True)
    parser.add_argument('--version-label', required=True)
    parser.add_argument('--source-fingerprint', required=True)
    parser.add_argument('--max-zoom', required=True, type=int)
    parser.add_argument('--tile-size', required=True, type=int)
    parser.add_argument('--height-quantization-ft', required=True, type=int)
    args = parser.parse_args()

    output_dir = Path(args.output_dir)
    groups = scan_sources(args.source_dir, args.max_zoom)
    for rel, paths in sorted(groups.items()):
        composite_group(paths, output_dir / rel, args.tile_size, args.height_quantization_ft)
    levels = scan_levels(output_dir)
    if not levels:
        raise SystemExit('no terrain wide-angle tiles were produced')
    manifest = {
        'schema_version': 1,
        'product': 'terrain',
        'region': 'wide',
        'version_label': args.version_label,
        'source_fingerprint': args.source_fingerprint,
        'min_zoom': min(level['zoom'] for level in levels),
        'max_zoom': args.max_zoom,
        'base_zoom': args.max_zoom,
        'tile_size': args.tile_size,
        'tile_format': 'ABT2',
        'tile_content_encoding': 'gzip',
        'zip_member_compression': 'stored',
        'wide_angle': True,
        'wide_angle_max_zoom': args.max_zoom,
        'parent_tile_policy': 'max valid quantized elevation over regional low-zoom source samples; all-nodata regions remain nodata',
        'source_policy': 'max-composite matching low-zoom tiles from regional terrain products',
        'sample_encoding': 'int16_le_quantized_gradient_delta',
        'sample_units': 'height_quantization_ft bins',
        'height_quantization_ft': args.height_quantization_ft,
        'output_units': 'feet',
        'sample_vertical_datum': 'WGS84 ellipsoid',
        'nodata': NODATA,
        'source_region_count': len(args.source_dir),
        'tile_count': sum(level['tile_count'] for level in levels),
        'levels': levels,
        'files': {'tiles': 'tiles'},
    }
    (output_dir / 'manifest.json').write_text(json.dumps(manifest, indent=2, sort_keys=True) + '\n')

if __name__ == '__main__':
    main()
"#;

const TERRAIN_TILE_SCRIPT: &str = r#"
import argparse, gzip, json, math, struct
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path
import numpy as np
from osgeo import gdal

RADIUS = 6378137.0
ORIGIN_SHIFT = math.pi * RADIUS

WORKER_DS = None
WORKER_GEOID = None
WORKER_TILES_ROOT = None
WORKER_ZOOM = None
WORKER_TILE_SIZE = None
WORKER_HEIGHT_QUANTIZATION_FT = None
NODATA = -32768
MAGIC = b'ABT2'

def encode_gradient_delta(samples):
    raw = samples.astype('<i2', copy=False).view('<u2').astype(np.uint32)
    prediction = np.zeros(raw.shape, dtype=np.uint32)
    prediction[0, 1:] = raw[0, :-1]
    prediction[1:, 0] = raw[:-1, 0]
    prediction[1:, 1:] = raw[1:, :-1] + raw[:-1, 1:] - raw[:-1, :-1]
    return ((raw - prediction) & 0xffff).astype('<u2')

def decode_gradient_delta(payload, tile_size):
    residual = np.frombuffer(payload, dtype='<u2').reshape((tile_size, tile_size)).astype(np.uint32)
    raw = residual.cumsum(axis=0, dtype=np.uint32).cumsum(axis=1, dtype=np.uint32)
    return (raw & 0xffff).astype('<u2').view('<i2')

def mercator(lon, lat):
    lat = max(min(lat, 85.05112878), -85.05112878)
    mx = lon * ORIGIN_SHIFT / 180.0
    my = math.log(math.tan((90.0 + lat) * math.pi / 360.0)) * RADIUS
    return mx, my

def lonlat(mx, my):
    lon = (mx / ORIGIN_SHIFT) * 180.0
    lat = (2.0 * math.atan(math.exp(my / RADIUS)) - math.pi / 2.0) * 180.0 / math.pi
    return lon, lat

def tile_bounds(x, y, z, tile_size):
    initial_resolution = (2.0 * math.pi * RADIUS) / tile_size
    resolution = initial_resolution / (2 ** z)
    minx = x * tile_size * resolution - ORIGIN_SHIFT
    maxx = (x + 1) * tile_size * resolution - ORIGIN_SHIFT
    miny = y * tile_size * resolution - ORIGIN_SHIFT
    maxy = (y + 1) * tile_size * resolution - ORIGIN_SHIFT
    return minx, miny, maxx, maxy

def tile_range(west, south, east, north, z, tile_size):
    resolution = ((2.0 * math.pi * RADIUS) / tile_size) / (2 ** z)
    west_m, south_m = mercator(west, south)
    east_m, north_m = mercator(east, north)
    x0 = math.floor((west_m + ORIGIN_SHIFT) / resolution / tile_size)
    x1 = math.floor((east_m + ORIGIN_SHIFT) / resolution / tile_size)
    y0 = math.floor((south_m + ORIGIN_SHIFT) / resolution / tile_size)
    y1 = math.floor((north_m + ORIGIN_SHIFT) / resolution / tile_size)
    return range(x0, x1 + 1), range(y0, y1 + 1)

def load_geoid(path):
    values = {}
    with open(path) as f:
        for line in f:
            lat, lon, height, *_unused = [int(x) for x in line.strip().split(',')]
            values[(lat, lon)] = height
    return values

def geoid(values, lat, lon):
    lon = ((lon + 180.0) % 360.0) - 180.0
    lat = max(min(lat, 89.0), -90.0)
    lat0 = math.floor(lat)
    lat1 = min(lat0 + 1, 89)
    lon0 = math.floor(lon)
    lon1 = lon0 + 1
    if lon1 >= 180:
        lon1 -= 360
    lt = lat - lat0
    ln = lon - lon0
    sw = values[(lat0, lon0)]
    se = values[(lat0, lon1)]
    nw = values[(lat1, lon0)]
    ne = values[(lat1, lon1)]
    return (sw * (1-ln) + se * ln) * (1-lt) + (nw * (1-ln) + ne * ln) * lt

def write_tile(path, samples, tile_size, height_quantization_ft):
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = encode_gradient_delta(samples)
    raw = MAGIC + struct.pack('<HHhhff', tile_size, tile_size, NODATA, 0, float(height_quantization_ft), 0.0) + payload.tobytes()
    with open(path, 'wb') as f:
        f.write(gzip.compress(raw, mtime=0))

def read_tile(path, tile_size, height_quantization_ft):
    with gzip.open(path, 'rb') as f:
        raw = f.read()
    if raw[:4] != MAGIC:
        raise ValueError(f'{path} is not an ABT2 terrain tile')
    width, height, nodata, _reserved, _scale, _offset = struct.unpack('<HHhhff', raw[4:20])
    if width != tile_size or height != tile_size or nodata != NODATA or _scale != float(height_quantization_ft) or _offset != 0.0:
        raise ValueError(f'{path} has unexpected terrain header')
    expected_bytes = 20 + tile_size * tile_size * 2
    if len(raw) != expected_bytes:
        raise ValueError(f'{path} has unexpected terrain payload length')
    return decode_gradient_delta(raw[20:], tile_size)

def max_downsample_2x2(samples):
    blocks = samples.reshape((samples.shape[0] // 2, 2, samples.shape[1] // 2, 2))
    valid = blocks != NODATA
    safe = np.where(valid, blocks, NODATA)
    reduced = safe.max(axis=(1, 3)).astype('<i2')
    reduced[~valid.any(axis=(1, 3))] = NODATA
    return reduced

def build_parent_tile(tiles_root, z, x, y, tile_size, height_quantization_ft):
    half = tile_size // 2
    parent = np.full((tile_size, tile_size), NODATA, dtype='<i2')
    children = [
        (x * 2, y * 2 + 1, 0, half, 0, half),
        (x * 2 + 1, y * 2 + 1, 0, half, half, tile_size),
        (x * 2, y * 2, half, tile_size, 0, half),
        (x * 2 + 1, y * 2, half, tile_size, half, tile_size),
    ]
    for child_x, child_y, row0, row1, col0, col1 in children:
        child_path = tiles_root / str(z + 1) / str(child_x) / f'{child_y}.terrain'
        if child_path.exists():
            parent[row0:row1, col0:col1] = max_downsample_2x2(read_tile(child_path, tile_size, height_quantization_ft))
    write_tile(tiles_root / str(z) / str(x) / f'{y}.terrain', parent, tile_size, height_quantization_ft)

def build_parent_pyramid(tiles_root, max_zoom, tile_size, height_quantization_ft):
    counts = {max_zoom: sum(1 for _ in (tiles_root / str(max_zoom)).glob('*/*.terrain'))}
    for z in range(max_zoom - 1, -1, -1):
        child_root = tiles_root / str(z + 1)
        parents = set()
        for child_path in child_root.glob('*/*.terrain'):
            child_x = int(child_path.parent.name)
            child_y = int(child_path.stem)
            parents.add((child_x // 2, child_y // 2))
        for x, y in sorted(parents):
            build_parent_tile(tiles_root, z, x, y, tile_size, height_quantization_ft)
        counts[z] = len(parents)
    return counts

def scan_terrain_levels(tiles_root):
    levels = []
    for z_dir in sorted((path for path in tiles_root.iterdir() if path.is_dir()), key=lambda path: int(path.name)):
        zoom = int(z_dir.name)
        coords = []
        for x_dir in z_dir.iterdir():
            if not x_dir.is_dir():
                continue
            x = int(x_dir.name)
            for tile_path in x_dir.iterdir():
                if tile_path.suffix.lower() != '.terrain':
                    continue
                coords.append((x, int(tile_path.stem)))
        if not coords:
            continue
        xs = [x for x, _ in coords]
        ys = [y for _, y in coords]
        levels.append({
            'zoom': zoom,
            'tile_count': len(coords),
            'boxes': [{
                'x_min': min(xs),
                'x_max': max(xs),
                'y_tms_min': min(ys),
                'y_tms_max': max(ys),
            }],
        })
    return levels

def init_worker(vrt_path, geoid_csv_path, tiles_root, zoom, tile_size, height_quantization_ft):
    global WORKER_DS, WORKER_GEOID, WORKER_TILES_ROOT, WORKER_ZOOM, WORKER_TILE_SIZE, WORKER_HEIGHT_QUANTIZATION_FT
    WORKER_DS = gdal.Open(vrt_path)
    if WORKER_DS is None:
        raise RuntimeError(f'failed to open {vrt_path}')
    WORKER_GEOID = load_geoid(geoid_csv_path)
    WORKER_TILES_ROOT = Path(tiles_root)
    WORKER_ZOOM = zoom
    WORKER_TILE_SIZE = tile_size
    WORKER_HEIGHT_QUANTIZATION_FT = height_quantization_ft

def render_tile(task):
    x, y = task
    minx, miny, maxx, maxy = tile_bounds(x, y, WORKER_ZOOM, WORKER_TILE_SIZE)
    warped = gdal.Warp(
        '', WORKER_DS, format='MEM', dstSRS='EPSG:3857',
        outputBounds=[minx, miny, maxx, maxy],
        width=WORKER_TILE_SIZE, height=WORKER_TILE_SIZE,
        resampleAlg='max', overviewLevel='NONE', dstNodata=-999999.0,
    )
    arr = warped.ReadAsArray()
    center_lon, center_lat = lonlat((minx + maxx) / 2.0, (miny + maxy) / 2.0)
    tile_geoid_ft = geoid(WORKER_GEOID, center_lat, center_lon)
    invalid = (arr <= -999998.0) | np.isnan(arr)
    samples = np.ceil((arr.astype(np.float64) * 3.280839895 + tile_geoid_ft) / WORKER_HEIGHT_QUANTIZATION_FT)
    samples = np.clip(samples, -32767, 32767).astype('<i2')
    samples[invalid] = NODATA
    write_tile(
        WORKER_TILES_ROOT / str(WORKER_ZOOM) / str(x) / f'{y}.terrain',
        samples,
        WORKER_TILE_SIZE,
        WORKER_HEIGHT_QUANTIZATION_FT,
    )
    return 1

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--vrt', required=True)
    ap.add_argument('--geoid-csv', required=True)
    ap.add_argument('--geoid-metadata', required=True)
    ap.add_argument('--output-dir', required=True)
    ap.add_argument('--region', required=True)
    ap.add_argument('--bbox', required=True)
    ap.add_argument('--zoom', required=True, type=int)
    ap.add_argument('--tile-size', required=True, type=int)
    ap.add_argument('--height-quantization-ft', required=True, type=int)
    ap.add_argument('--version-label', required=True)
    ap.add_argument('--source-count', required=True, type=int)
    ap.add_argument('--missing-cells', default='')
    ap.add_argument('--workers', required=True, type=int)
    args = ap.parse_args()
    geoid_metadata = json.loads(Path(args.geoid_metadata).read_text())
    west, south, east, north = [float(x) for x in args.bbox.split(',')]
    root = Path(args.output_dir)
    tiles_root = root / 'tiles'
    x_range, y_range = tile_range(west, south, east, north, args.zoom, args.tile_size)
    tasks = [(x, y) for x in x_range for y in y_range]
    workers = max(1, args.workers)
    if workers == 1:
        init_worker(args.vrt, args.geoid_csv, str(tiles_root), args.zoom, args.tile_size, args.height_quantization_ft)
        count = sum(render_tile(task) for task in tasks)
    else:
        with ProcessPoolExecutor(
            max_workers=workers,
            initializer=init_worker,
            initargs=(args.vrt, args.geoid_csv, str(tiles_root), args.zoom, args.tile_size, args.height_quantization_ft),
        ) as pool:
            count = sum(pool.map(render_tile, tasks, chunksize=8))
    level_counts = build_parent_pyramid(tiles_root, args.zoom, args.tile_size, args.height_quantization_ft)
    levels = scan_terrain_levels(tiles_root)
    manifest = {
        'schema_version': 1,
        'product': 'terrain',
        'region': args.region,
        'version_label': args.version_label,
        'min_zoom': 0,
        'max_zoom': args.zoom,
        'base_zoom': args.zoom,
        'tile_size': args.tile_size,
        'tile_format': 'ABT2',
        'tile_content_encoding': 'gzip',
        'zip_member_compression': 'stored',
        'base_tile_resampling': 'GDAL max resampling from source DEM with overviewLevel=NONE',
        'parent_tile_policy': 'max valid quantized elevation over child samples; all-nodata children remain nodata',
        'sample_encoding': 'int16_le_quantized_gradient_delta',
        'sample_units': 'height_quantization_ft bins',
        'height_quantization_ft': args.height_quantization_ft,
        'output_units': 'feet',
        'sample_vertical_datum': 'WGS84 ellipsoid',
        'source_dem': 'USGS 3DEP 1 arc-second DEM',
        'source_dem_vertical_datum': 'source tile metadata; generally NAVD88 in CONUS',
        'geoid_model': geoid_metadata,
        'geoid_application_policy': 'one-degree geoid-height grid applied once per tile at tile center',
        'worker_count': workers,
        'refresh_policy': {
            'identity': 'published filename is content-addressed by ZIP bytes',
            'source_fetched_at_utc': 'reported in the cycle bundle package row',
            'refresh_interval': 'producer policy; not embedded in artifact metadata'
        },
        'source_dem_count': args.source_count,
        'missing_dem_cells': [cell for cell in args.missing_cells.split(',') if cell],
        'nodata': -32768,
        'base_tile_count': count,
        'tile_count': sum(level_counts.values()),
        'levels': levels,
        'files': {'tiles': 'tiles'}
    }
    with open(root / 'manifest.json', 'w') as f:
        json.dump(manifest, f, indent=2, sort_keys=True)

if __name__ == '__main__':
    main()
"#;
