use super::*;

pub(super) fn manifest_generated_at(node_records: &[NodeRecord]) -> String {
    node_records
        .iter()
        .map(|record| record.finished_at_utc.as_str())
        .max()
        .unwrap_or_else(|| panic!("build manifest should include at least one node"))
        .to_string()
}

pub(super) fn gc_roots_path(config: &ProductBuildConfig) -> PathBuf {
    config
        .build_root
        .join("cache")
        .join("gc_roots")
        .join(format!("{}_build_roots.json", config.profile.as_str()))
}

pub(super) fn load_gc_roots(
    path: &Path,
    config: &ProductBuildConfig,
) -> anyhow::Result<GcRootsManifest> {
    if path.is_file() {
        return serde_json::from_slice(
            &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", path.display()));
    }
    Ok(GcRootsManifest {
        schema_version: 1,
        profile: config.profile.as_str().to_string(),
        build_root: config.build_root.display().to_string(),
        updated_at_utc: utc_now_string(),
        node_roots: BTreeMap::new(),
    })
}

pub(super) fn write_gc_roots(path: &Path, roots: &GcRootsManifest) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let temp_path = path.with_extension("json.tmp");
    fs::write(
        &temp_path,
        serde_json::to_vec_pretty(roots).context("failed to encode GC roots")?,
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

pub(super) fn record_gc_roots(
    config: &ProductBuildConfig,
    scope: &str,
    task_records: &BTreeMap<String, Vec<NodeRecord>>,
) -> anyhow::Result<PathBuf> {
    let roots_path = gc_roots_path(config);
    let mut roots = load_gc_roots(&roots_path, config)?;
    let now = utc_now_string();
    roots.schema_version = 1;
    roots.profile = config.profile.as_str().to_string();
    roots.build_root = config.build_root.display().to_string();
    roots.updated_at_utc = now.clone();
    let prefix = format!("{scope}:");
    roots.node_roots.retain(|key, _| !key.starts_with(&prefix));
    let cache_nodes_root = config.build_root.join("cache").join("nodes");
    for (task_id, records) in task_records {
        for record in records {
            let key = format!("{scope}:{task_id}:{}:{}", record.name, record.fingerprint);
            let node_dir = cache_nodes_root
                .join(&record.name)
                .join(&record.fingerprint);
            let record_path = node_dir.join("build-record.json");
            roots.node_roots.insert(
                key,
                GcNodeRoot {
                    scope: scope.to_string(),
                    task_id: task_id.clone(),
                    node_name: record.name.clone(),
                    fingerprint: record.fingerprint.clone(),
                    node_dir: relative_artifact_path(&node_dir, &config.build_root),
                    record_path: relative_artifact_path(&record_path, &config.build_root),
                    cache_hit: record.cache_hit,
                    finished_at_utc: record.finished_at_utc.clone(),
                    updated_at_utc: now.clone(),
                },
            );
        }
    }
    write_gc_roots(&roots_path, &roots)?;
    Ok(roots_path)
}

pub(super) fn record_gc_roots_from_build_manifest(
    config: &ProductBuildConfig,
    scope: &str,
    build_manifest: &BuildManifest,
) -> anyhow::Result<PathBuf> {
    let mut task_records = BTreeMap::<String, Vec<NodeRecord>>::new();
    for record in &build_manifest.nodes {
        task_records
            .entry(record.name.clone())
            .or_default()
            .push(record.clone());
    }
    record_gc_roots(config, scope, &task_records)
}

pub(super) fn bootstrap_gc_roots_from_build_manifests(
    config: &ProductBuildConfig,
) -> anyhow::Result<PathBuf> {
    let manifest_dir = config
        .build_root
        .join("private-work")
        .join("build-manifests")
        .join(publish_path_key(&config.publish_dir, &config.build_root));
    let mut records = BTreeMap::<String, Vec<NodeRecord>>::new();
    if manifest_dir.is_dir() {
        for entry in fs::read_dir(&manifest_dir)
            .with_context(|| format!("failed to read {}", manifest_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("build-manifest_") && name.ends_with(".json"))
            {
                continue;
            }
            let manifest: BuildManifest = serde_json::from_slice(
                &fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?,
            )
            .with_context(|| format!("failed to parse {}", path.display()))?;
            records.insert(format!("{}:build-manifest", manifest.cycle), manifest.nodes);
        }
    }
    if records.is_empty() {
        bail!(
            "no build-manifest_*.json files found in {}",
            manifest_dir.display()
        );
    }
    record_gc_roots(config, "full", &records)
}

pub fn gc_build_cache(config: &BuildCacheGcConfig) -> anyhow::Result<BuildCacheGcReport> {
    let publish_dir = config
        .build_root
        .join("published")
        .join("gc")
        .join("00000000T000000Z");
    let product_config = ProductBuildConfig {
        chart_cutline_root: PathBuf::new(),
        build_root: config.build_root.clone(),
        publish_dir: publish_dir.clone(),
        packaged_dir: publish_dir.join("packaged"),
        profile: config.profile,
        publish_label: "gc".to_string(),
        publish_timestamp: "00000000T000000Z".to_string(),
        target_cycle: None,
        fetch_jobs: 1,
        cpu_jobs: 1,
        max_heavy_jobs: 1,
        fetch_cache_root: config.build_root.join("cache").join("fetch"),
        fetch_cache_mode: "cache-first".to_string(),
    };
    if config.bootstrap_from_build_manifests {
        bootstrap_gc_roots_from_build_manifests(&product_config)?;
    }
    let roots_path = gc_roots_path(&product_config);
    let roots = load_gc_roots(&roots_path, &product_config)?;
    let cache_nodes_root = config.build_root.join("cache").join("nodes");
    let rooted = roots
        .node_roots
        .values()
        .map(|root| (root.node_name.clone(), root.fingerprint.clone()))
        .collect::<BTreeSet<_>>();
    let mut report = BuildCacheGcReport {
        roots_path,
        rooted_nodes: rooted.len(),
        scanned_nodes: 0,
        active_nodes: 0,
        stale_lock_nodes: 0,
        grace_nodes: 0,
        evictable_nodes: 0,
        reclaimed_bytes: 0,
        scratch_files: 0,
        scratch_bytes: 0,
        scratch_active_nodes: 0,
        private_scratch_files: 0,
        private_scratch_bytes: 0,
        private_scratch_active_nodes: 0,
        by_node_name: BTreeMap::new(),
    };
    if !cache_nodes_root.is_dir() {
        return Ok(report);
    }
    let grace = Duration::from_secs(config.grace_hours.saturating_mul(3600));
    let now = SystemTime::now();
    for node_entry in fs::read_dir(&cache_nodes_root)
        .with_context(|| format!("failed to read {}", cache_nodes_root.display()))?
    {
        let node_entry = node_entry?;
        if !node_entry.file_type()?.is_dir() {
            continue;
        }
        let node_name = node_entry.file_name().to_string_lossy().to_string();
        for fingerprint_entry in fs::read_dir(node_entry.path())
            .with_context(|| format!("failed to read {}", node_entry.path().display()))?
        {
            let fingerprint_entry = fingerprint_entry?;
            if !fingerprint_entry.file_type()?.is_dir() {
                continue;
            }
            let fingerprint = fingerprint_entry.file_name().to_string_lossy().to_string();
            report.scanned_nodes += 1;
            if rooted.contains(&(node_name.clone(), fingerprint.clone())) {
                continue;
            }
            let node_dir = fingerprint_entry.path();
            let lock_path = node_dir.join(".build-lock");
            if lock_path.exists() {
                if lock_is_live(&lock_path)? {
                    report.active_nodes += 1;
                    continue;
                }
                report.stale_lock_nodes += 1;
            }
            if is_younger_than(&node_dir, now, grace)? {
                report.grace_nodes += 1;
                continue;
            }
            let bytes = directory_size(&node_dir)?;
            report.evictable_nodes += 1;
            report.reclaimed_bytes = report.reclaimed_bytes.saturating_add(bytes);
            let bucket = report.by_node_name.entry(node_name.clone()).or_default();
            bucket.count += 1;
            bucket.bytes = bucket.bytes.saturating_add(bytes);
            if config.mode == BuildCacheGcMode::Execute {
                set_tree_readonly(&node_dir, false)?;
                fs::remove_dir_all(&node_dir)
                    .with_context(|| format!("failed to remove {}", node_dir.display()))?;
            }
        }
    }
    scrub_rooted_tpp_render_scratch(&cache_nodes_root, &rooted, config.mode, &mut report)?;
    scrub_terrain_private_work_scratch(
        &config.build_root,
        &cache_nodes_root,
        config.mode,
        &mut report,
    )?;
    Ok(report)
}

pub(super) fn scrub_terrain_private_work_scratch(
    build_root: &Path,
    cache_nodes_root: &Path,
    mode: BuildCacheGcMode,
    report: &mut BuildCacheGcReport,
) -> anyhow::Result<()> {
    let private_terrain_root = build_root.join("private-work").join("terrain");
    if !private_terrain_root.exists() {
        return Ok(());
    }
    if terrain_node_build_is_active(cache_nodes_root)? {
        report.private_scratch_active_nodes += 1;
        return Ok(());
    }
    report.private_scratch_files += count_files_in_dir(&private_terrain_root)?;
    report.private_scratch_bytes = report
        .private_scratch_bytes
        .saturating_add(directory_size(&private_terrain_root)?);
    if mode == BuildCacheGcMode::Execute {
        fs::remove_dir_all(&private_terrain_root)
            .with_context(|| format!("failed to remove {}", private_terrain_root.display()))?;
    }
    Ok(())
}

pub(super) fn terrain_node_build_is_active(cache_nodes_root: &Path) -> anyhow::Result<bool> {
    if !cache_nodes_root.is_dir() {
        return Ok(false);
    }
    for node_entry in fs::read_dir(cache_nodes_root)
        .with_context(|| format!("failed to read {}", cache_nodes_root.display()))?
    {
        let node_entry = node_entry?;
        if !node_entry.file_type()?.is_dir() {
            continue;
        }
        let node_name = node_entry.file_name().to_string_lossy().to_string();
        if !node_name.starts_with("static-terrain-") {
            continue;
        }
        for fingerprint_entry in fs::read_dir(node_entry.path())
            .with_context(|| format!("failed to read {}", node_entry.path().display()))?
        {
            let fingerprint_entry = fingerprint_entry?;
            if !fingerprint_entry.file_type()?.is_dir() {
                continue;
            }
            let lock_path = fingerprint_entry.path().join(".build-lock");
            if lock_path.exists() && lock_is_live(&lock_path)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub(super) fn count_files_in_dir(dir: &Path) -> anyhow::Result<usize> {
    let mut count = 0usize;
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            count += count_files_in_dir(&entry.path())?;
        } else {
            count += 1;
        }
    }
    Ok(count)
}

pub(super) fn scrub_rooted_tpp_render_scratch(
    cache_nodes_root: &Path,
    rooted: &BTreeSet<(String, String)>,
    mode: BuildCacheGcMode,
    report: &mut BuildCacheGcReport,
) -> anyhow::Result<()> {
    for (node_name, fingerprint) in rooted {
        if !is_tpp_render_node_name(node_name) {
            continue;
        }
        let node_dir = cache_nodes_root.join(node_name).join(fingerprint);
        if !node_dir.is_dir() {
            continue;
        }
        let lock_path = node_dir.join(".build-lock");
        if lock_path.exists() && lock_is_live(&lock_path)? {
            report.scratch_active_nodes += 1;
            continue;
        }
        scrub_tpp_render_scratch_dir(&node_dir, mode, report)?;
    }
    Ok(())
}

pub(super) fn is_tpp_render_node_name(node_name: &str) -> bool {
    node_name.starts_with("tpp-") && node_name.ends_with("-render")
}

pub(super) fn scrub_tpp_render_scratch_dir(
    dir: &Path,
    mode: BuildCacheGcMode,
    report: &mut BuildCacheGcReport,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            scrub_tpp_render_scratch_dir(&path, mode, report)?;
            continue;
        }
        if !is_tpp_render_scratch_file(&path) {
            continue;
        }
        let bytes = entry
            .metadata()
            .with_context(|| format!("failed to stat {}", path.display()))?
            .len();
        report.scratch_files += 1;
        report.scratch_bytes = report.scratch_bytes.saturating_add(bytes);
        if mode == BuildCacheGcMode::Execute {
            set_path_readonly(&path, false)?;
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        }
    }
    Ok(())
}

pub(super) fn is_tpp_render_scratch_file(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    if extension.eq_ignore_ascii_case("pdf") {
        return true;
    }
    if !(extension.eq_ignore_ascii_case("tif") || extension.eq_ignore_ascii_case("tiff")) {
        return false;
    }
    path.components()
        .any(|component| component.as_os_str() == "plates")
}

pub(super) fn is_younger_than(
    path: &Path,
    now: SystemTime,
    grace: Duration,
) -> anyhow::Result<bool> {
    if grace.is_zero() {
        return Ok(false);
    }
    let modified = fs::metadata(path)
        .with_context(|| format!("failed to stat {}", path.display()))?
        .modified()
        .with_context(|| format!("failed to read mtime for {}", path.display()))?;
    Ok(now
        .duration_since(modified)
        .map(|age| age < grace)
        .unwrap_or(true))
}

pub(super) fn lock_is_live(lock_path: &Path) -> anyhow::Result<bool> {
    let Some(pid) = read_lock_pid(lock_path)? else {
        return Ok(true);
    };
    Ok(process_is_alive(pid))
}

pub(super) fn directory_size(path: &Path) -> anyhow::Result<u64> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Ok(0);
    }
    let mut total = 0_u64;
    for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
        let entry = entry?;
        total = total.saturating_add(directory_size(&entry.path())?);
    }
    Ok(total)
}
