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
    let current_artifacts_path = config
        .build_root
        .join("published")
        .join(current_artifacts_latest_alias_filename());
    let current_artifacts: Vec<CurrentArtifactsManifest> = serde_json::from_slice(
        &fs::read(&current_artifacts_path)
            .with_context(|| format!("failed to read {}", current_artifacts_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", current_artifacts_path.display()))?;
    if current_artifacts.is_empty() {
        bail!(
            "{} must contain at least one manifest",
            current_artifacts_path.display()
        );
    }

    let build_manifests_root = build_manifests_root(&config.build_root);
    let mut manifest_dirs = BTreeMap::new();
    for manifest in &current_artifacts {
        let publish_dir = publish_dir_for_current_artifacts_manifest(config, manifest)?;
        let publish_key = publish_path_key(&publish_dir, &config.build_root);
        manifest_dirs.insert(publish_key.clone(), build_manifests_root.join(publish_key));
    }

    let mut records = BTreeMap::<String, Vec<NodeRecord>>::new();
    for (publish_key, manifest_dir) in manifest_dirs {
        if !manifest_dir.is_dir() {
            bail!(
                "build manifest directory for current publication {} is missing: {}",
                publish_key,
                manifest_dir.display()
            );
        }
        let mut found = false;
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
            found = true;
            records
                .entry(format!("{}:{}:build-manifest", publish_key, manifest.cycle))
                .or_default()
                .extend(manifest.nodes);
        }
        if !found {
            bail!(
                "no build-manifest_*.json files found in {}",
                manifest_dir.display()
            );
        }
    }
    record_gc_roots(config, "full", &records)
}

fn publish_dir_for_current_artifacts_manifest(
    config: &ProductBuildConfig,
    manifest: &CurrentArtifactsManifest,
) -> anyhow::Result<PathBuf> {
    let packaged_root = manifest.artifact_roots.packaged.trim();
    let relative_packaged_root = packaged_root.trim_matches('/');
    if relative_packaged_root.is_empty() {
        bail!("current_artifacts artifact_roots.packaged must not be empty");
    }
    let relative_packaged_path = Path::new(relative_packaged_root);
    if relative_packaged_path.is_absolute() {
        bail!(
            "current_artifacts artifact_roots.packaged must be relative: {}",
            manifest.artifact_roots.packaged
        );
    }
    for component in relative_packaged_path.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            bail!(
                "current_artifacts artifact_roots.packaged contains an invalid path component: {}",
                manifest.artifact_roots.packaged
            );
        }
    }
    if relative_packaged_path
        .file_name()
        .and_then(|name| name.to_str())
        != Some("packaged")
    {
        bail!(
            "current_artifacts artifact_roots.packaged must end with packaged/: {}",
            manifest.artifact_roots.packaged
        );
    }
    let relative_publish_dir = relative_packaged_path.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "current_artifacts artifact_roots.packaged has no publish directory: {}",
            manifest.artifact_roots.packaged
        )
    })?;
    if relative_publish_dir.as_os_str().is_empty() {
        bail!(
            "current_artifacts artifact_roots.packaged has no publish directory: {}",
            manifest.artifact_roots.packaged
        );
    }
    Ok(config
        .build_root
        .join("published")
        .join(relative_publish_dir))
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
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_config(build_root: PathBuf) -> ProductBuildConfig {
        let publish_dir = build_root
            .join("published")
            .join("gc")
            .join("00000000T000000Z");
        ProductBuildConfig {
            chart_cutline_root: PathBuf::new(),
            build_root,
            publish_dir: publish_dir.clone(),
            packaged_dir: publish_dir.join("packaged"),
            profile: ProductBuildProfile::Production,
            publish_label: "gc".to_string(),
            publish_timestamp: "00000000T000000Z".to_string(),
            target_cycle: None,
            fetch_jobs: 1,
            cpu_jobs: 1,
            max_heavy_jobs: 1,
            fetch_cache_root: PathBuf::new(),
            fetch_cache_mode: "cache-first".to_string(),
        }
    }

    fn current_manifest(
        label: &str,
        timestamp: &str,
        nav_contract: &str,
    ) -> CurrentArtifactsManifest {
        CurrentArtifactsManifest {
            schema_version: 1,
            contracts: BTreeMap::from([("nav-db".to_string(), nav_contract.to_string())]),
            artifact_roots: CurrentArtifactRoots {
                packaged: format!("{label}/{timestamp}/packaged/"),
                unpacked: format!("{label}/{timestamp}/unpacked/"),
            },
            as_of_date: "2026-06-09".to_string(),
            as_of_utc: "2026-06-09T00:00:00Z".to_string(),
            bundles: Vec::new(),
            startup_prefetch: None,
            diagnostics: None,
        }
    }

    fn node_record(name: &str, fingerprint: &str) -> NodeRecord {
        NodeRecord {
            name: name.to_string(),
            fingerprint: fingerprint.to_string(),
            started_at_utc: "2026-06-09T00:00:00Z".to_string(),
            finished_at_utc: "2026-06-09T00:00:01Z".to_string(),
            elapsed_ms: 1000,
            cache_hit: true,
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            output_details: BTreeMap::new(),
        }
    }

    fn write_build_manifest(
        build_root: &Path,
        label: &str,
        timestamp: &str,
        cycle: &str,
        nodes: Vec<NodeRecord>,
    ) {
        let publish_dir = build_root.join("published").join(label).join(timestamp);
        let manifest_dir =
            build_manifests_root(build_root).join(publish_path_key(&publish_dir, build_root));
        fs::create_dir_all(&manifest_dir).unwrap();
        let manifest = BuildManifest {
            schema_version: 1,
            profile: "production".to_string(),
            cycle: cycle.to_string(),
            build_root: build_root.display().to_string(),
            generated_at_utc: "2026-06-09T00:00:01Z".to_string(),
            fetch_cache_root: "cache/fetch".to_string(),
            fetch_cache_mode: "cache-first".to_string(),
            nodes,
        };
        fs::write(
            manifest_dir.join(format!("build-manifest_{cycle}.json")),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn bootstrap_gc_roots_from_build_manifests_uses_merged_current_artifacts() {
        let temp = tempdir().unwrap();
        let build_root = temp.path().join("artifacts");
        let config = test_config(build_root.clone());
        let current_artifacts = vec![
            current_manifest("nav6-sunset-abc", "20260609T000000Z", "NAV6"),
            current_manifest(
                "master-def",
                "20260609T000010Z",
                product_contracts::NAV_DB_CONTRACT_ID,
            ),
        ];
        fs::create_dir_all(build_root.join("published")).unwrap();
        fs::write(
            build_root
                .join("published")
                .join(current_artifacts_latest_alias_filename()),
            serde_json::to_vec_pretty(&current_artifacts).unwrap(),
        )
        .unwrap();
        write_build_manifest(
            &build_root,
            "nav6-sunset-abc",
            "20260609T000000Z",
            "2606",
            vec![node_record("nav-db", "nav6-fingerprint")],
        );
        write_build_manifest(
            &build_root,
            "master-def",
            "20260609T000010Z",
            "2606",
            vec![node_record("nav-db", "nav9-fingerprint")],
        );

        let roots_path =
            bootstrap_gc_roots_from_build_manifests(&config).expect("bootstrap GC roots");
        let roots: GcRootsManifest =
            serde_json::from_slice(&fs::read(roots_path).unwrap()).unwrap();
        let rooted = roots
            .node_roots
            .values()
            .map(|root| (root.node_name.as_str(), root.fingerprint.as_str()))
            .collect::<BTreeSet<_>>();
        assert!(rooted.contains(&("nav-db", "nav6-fingerprint")));
        assert!(rooted.contains(&("nav-db", "nav9-fingerprint")));
    }
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
