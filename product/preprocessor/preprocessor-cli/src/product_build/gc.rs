// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

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
        // Historical filename. There is no configurable production/validation
        // profile anymore; keep the path stable so GC does not discard the
        // existing node-root history and force needless rebuilds.
        .join("production_build_roots.json")
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
    let current_build_manifests = current_build_manifests(config)?;
    let mut records = BTreeMap::<String, Vec<NodeRecord>>::new();
    for current in current_build_manifests.manifests {
        let publish_key = current.publish_key;
        for manifest in current.manifests {
            records
                .entry(format!("{}:{}:build-manifest", publish_key, manifest.cycle))
                .or_default()
                .extend(manifest.nodes);
        }
    }
    record_gc_roots(config, "full", &records)
}

#[derive(Debug)]
struct CurrentBuildManifests {
    current_artifacts_path: PathBuf,
    manifests: Vec<CurrentBuildManifestSet>,
}

#[derive(Debug)]
struct CurrentBuildManifestSet {
    publish_key: String,
    manifests: Vec<BuildManifest>,
}

fn current_build_manifests(config: &ProductBuildConfig) -> anyhow::Result<CurrentBuildManifests> {
    let current_artifacts_path = current_artifacts_path(&config.build_root);
    let current_artifacts = load_current_artifacts_list(&current_artifacts_path)?;
    let build_manifests_root = build_manifests_root(&config.build_root);
    let mut manifest_dirs = BTreeMap::new();
    for manifest in &current_artifacts {
        let publish_dir = publish_dir_for_current_artifacts_manifest(config, manifest)?;
        let publish_key = publish_path_key(&publish_dir, &config.build_root);
        manifest_dirs.insert(publish_key.clone(), build_manifests_root.join(publish_key));
    }

    let mut manifests = Vec::new();
    for (publish_key, manifest_dir) in manifest_dirs {
        if !manifest_dir.is_dir() {
            bail!(
                "build manifest directory for current publication {} is missing: {}",
                publish_key,
                manifest_dir.display()
            );
        }
        let mut build_manifests = Vec::new();
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
            build_manifests.push(manifest);
        }
        if build_manifests.is_empty() {
            bail!(
                "no build-manifest_*.json files found in {}",
                manifest_dir.display()
            );
        }
        manifests.push(CurrentBuildManifestSet {
            publish_key,
            manifests: build_manifests,
        });
    }
    Ok(CurrentBuildManifests {
        current_artifacts_path,
        manifests,
    })
}

fn current_artifacts_path(build_root: &Path) -> PathBuf {
    build_root
        .join("published")
        .join(current_artifacts_latest_alias_filename())
}

fn load_current_artifacts_list(path: &Path) -> anyhow::Result<Vec<CurrentArtifactsManifest>> {
    let current_artifacts: Vec<CurrentArtifactsManifest> = serde_json::from_slice(
        &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))?;
    if current_artifacts.is_empty() {
        bail!("{} must contain at least one manifest", path.display());
    }
    Ok(current_artifacts)
}

fn publish_dir_for_current_artifacts_manifest(
    config: &ProductBuildConfig,
    manifest: &CurrentArtifactsManifest,
) -> anyhow::Result<PathBuf> {
    publish_dir_from_artifact_root(
        &config.build_root,
        &manifest.artifact_roots.packaged,
        "packaged",
    )
}

fn publish_dir_from_artifact_root(
    build_root: &Path,
    artifact_root: &str,
    expected_leaf: &str,
) -> anyhow::Result<PathBuf> {
    let packaged_root = artifact_root.trim();
    let relative_packaged_root = packaged_root.trim_matches('/');
    if relative_packaged_root.is_empty() {
        bail!("current_artifacts artifact root must not be empty");
    }
    let relative_packaged_path = Path::new(relative_packaged_root);
    if relative_packaged_path.is_absolute() {
        bail!(
            "current_artifacts artifact root must be relative: {}",
            artifact_root
        );
    }
    for component in relative_packaged_path.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            bail!(
                "current_artifacts artifact root contains an invalid path component: {}",
                artifact_root
            );
        }
    }
    if relative_packaged_path
        .file_name()
        .and_then(|name| name.to_str())
        != Some(expected_leaf)
    {
        bail!(
            "current_artifacts artifact root must end with {expected_leaf}/: {}",
            artifact_root
        );
    }
    let relative_publish_dir = relative_packaged_path.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "current_artifacts artifact root has no publish directory: {}",
            artifact_root
        )
    })?;
    if relative_publish_dir.as_os_str().is_empty() {
        bail!(
            "current_artifacts artifact root has no publish directory: {}",
            artifact_root
        );
    }
    Ok(build_root.join("published").join(relative_publish_dir))
}

pub fn gc_build_cache(config: &BuildCacheGcConfig) -> anyhow::Result<BuildCacheGcReport> {
    let publish_dir = config
        .build_root
        .join("published")
        .join("gc")
        .join("00000000T000000Z");
    let product_config = ProductBuildConfig {
        chart_metadata_root: PathBuf::new(),
        build_root: config.build_root.clone(),
        publish_dir: publish_dir.clone(),
        packaged_dir: publish_dir.join("packaged"),
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
    scrub_tpp_render_scratch_cache(&cache_nodes_root, config.mode, &mut report)?;
    scrub_chart_render_intermediates_cache(&cache_nodes_root, config.mode, &mut report)?;
    scrub_water_mask_intermediates_cache(&cache_nodes_root, config.mode, &mut report)?;
    Ok(report)
}

pub fn gc_publication(config: &PublicationGcConfig) -> anyhow::Result<PublicationGcReport> {
    let current_artifacts_path = current_artifacts_path(&config.build_root);
    let current_artifacts = load_current_artifacts_list(&current_artifacts_path)?;
    let mut current_publish_roots = BTreeSet::<PathBuf>::new();
    for manifest in &current_artifacts {
        current_publish_roots.insert(publish_dir_from_artifact_root(
            &config.build_root,
            &manifest.artifact_roots.packaged,
            "packaged",
        )?);
        current_publish_roots.insert(publish_dir_from_artifact_root(
            &config.build_root,
            &manifest.artifact_roots.unpacked,
            "unpacked",
        )?);
    }

    let published_root = config.build_root.join("published");
    let publish_roots = discover_publish_roots(&published_root)?;
    let grace = Duration::from_secs(config.grace_hours.saturating_mul(3600));
    let now = SystemTime::now();
    let mut report = PublicationGcReport {
        current_artifacts_path,
        current_publish_roots: current_publish_roots.len(),
        scanned_publish_roots: 0,
        grace_roots: 0,
        evictable_roots: 0,
        reclaimed_bytes: 0,
        candidates: Vec::new(),
    };
    let mut candidate_roots = Vec::new();
    for publish_root in publish_roots {
        report.scanned_publish_roots += 1;
        if current_publish_roots.contains(&publish_root) {
            continue;
        }
        if is_younger_than(&publish_root, now, grace)? {
            report.grace_roots += 1;
            continue;
        }
        report.evictable_roots += 1;
        candidate_roots.push(publish_root);
    }
    let candidate_sizes = hardlink_reclaimable_sizes(&candidate_roots)?;
    for (publish_root, bytes) in candidate_roots.iter().zip(candidate_sizes) {
        report.reclaimed_bytes = report.reclaimed_bytes.saturating_add(bytes);
        report.candidates.push(PublicationGcCandidate {
            path: publish_root.clone(),
            bytes,
        });
    }
    if config.mode == BuildCacheGcMode::Execute {
        for publish_root in candidate_roots {
            fs::remove_dir_all(&publish_root)
                .with_context(|| format!("failed to remove {}", publish_root.display()))?;
            remove_empty_publish_parent(&published_root, &publish_root)?;
        }
    }
    Ok(report)
}

#[derive(Clone, Copy)]
struct InodeEntry {
    nlink: u64,
    size: u64,
    count: u64,
    first_root: usize,
}

fn hardlink_reclaimable_sizes(roots: &[PathBuf]) -> anyhow::Result<Vec<u64>> {
    let mut inodes = BTreeMap::<(u64, u64), InodeEntry>::new();
    for (root_index, root) in roots.iter().enumerate() {
        collect_candidate_inodes(root, root_index, &mut inodes)?;
    }
    let mut sizes = vec![0_u64; roots.len()];
    for entry in inodes.values() {
        if entry.nlink <= entry.count {
            sizes[entry.first_root] = sizes[entry.first_root].saturating_add(entry.size);
        }
    }
    Ok(sizes)
}

fn collect_candidate_inodes(
    path: &Path,
    root_index: usize,
    inodes: &mut BTreeMap<(u64, u64), InodeEntry>,
) -> anyhow::Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    if metadata.is_file() {
        let key = (metadata.dev(), metadata.ino());
        inodes
            .entry(key)
            .and_modify(|entry| entry.count = entry.count.saturating_add(1))
            .or_insert(InodeEntry {
                nlink: metadata.nlink(),
                size: metadata.len(),
                count: 1,
                first_root: root_index,
            });
        return Ok(());
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
        let entry = entry?;
        collect_candidate_inodes(&entry.path(), root_index, inodes)?;
    }
    Ok(())
}

pub fn gc_fetch_cache(config: &FetchCacheGcConfig) -> anyhow::Result<FetchCacheGcReport> {
    let current = current_build_manifests(&ProductBuildConfig {
        chart_metadata_root: PathBuf::new(),
        build_root: config.build_root.clone(),
        publish_dir: config
            .build_root
            .join("published")
            .join("gc")
            .join("00000000T000000Z"),
        packaged_dir: config
            .build_root
            .join("published")
            .join("gc")
            .join("00000000T000000Z")
            .join("packaged"),
        publish_label: "gc".to_string(),
        publish_timestamp: "00000000T000000Z".to_string(),
        target_cycle: None,
        fetch_jobs: 1,
        cpu_jobs: 1,
        max_heavy_jobs: 1,
        fetch_cache_root: config.build_root.join("cache").join("fetch"),
        fetch_cache_mode: "cache-first".to_string(),
    })?;
    let fetch_root = config.build_root.join("cache").join("fetch");
    let layout = CacheLayout::new(&fetch_root);
    let mut rooted_shas = BTreeSet::<String>::new();
    let mut rooted_metadata_paths = BTreeSet::<PathBuf>::new();
    let mut rooted_fetch_refs = 0usize;
    let mut build_manifest_count = 0usize;
    for set in &current.manifests {
        build_manifest_count += set.manifests.len();
        for manifest in &set.manifests {
            for node in &manifest.nodes {
                for fetch_ref in &node.fetch_cache_refs {
                    rooted_fetch_refs += 1;
                    rooted_shas.insert(fetch_ref.sha256.clone());
                    rooted_metadata_paths.insert(layout.http_metadata_path(&fetch_ref.cache_key));
                }
            }
        }
    }
    let grace = Duration::from_secs(config.grace_hours.saturating_mul(3600));
    let now = SystemTime::now();
    let mut grace_shas = BTreeSet::<String>::new();
    let mut report = FetchCacheGcReport {
        current_artifacts_path: current.current_artifacts_path,
        build_manifests: build_manifest_count,
        rooted_fetch_refs,
        rooted_blobs: rooted_shas.len(),
        scanned_metadata: 0,
        scanned_blobs: 0,
        grace_metadata: 0,
        grace_blobs: 0,
        evictable_metadata: 0,
        evictable_blobs: 0,
        reclaimed_bytes: 0,
        candidates: Vec::new(),
        missing_fetch_refs: rooted_fetch_refs == 0,
    };
    if rooted_fetch_refs == 0 {
        if config.mode == BuildCacheGcMode::Execute {
            bail!(
                "current build manifests contain no fetch_cache_refs; run a product build with fetch-dependency recording before executing fetch-cache GC"
            );
        }
        return Ok(report);
    }

    for metadata_path in fetch_metadata_paths(&layout.http_dir())? {
        report.scanned_metadata += 1;
        let metadata = read_fetch_metadata(&metadata_path)?;
        let sha = metadata
            .get("sha256")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);
        if rooted_metadata_paths.contains(&metadata_path) {
            continue;
        }
        if is_younger_than(&metadata_path, now, grace)? {
            report.grace_metadata += 1;
            if let Some(sha) = sha {
                grace_shas.insert(sha);
            }
            continue;
        }
        let bytes = file_size(&metadata_path)?;
        report.evictable_metadata += 1;
        report.reclaimed_bytes = report.reclaimed_bytes.saturating_add(bytes);
        report.candidates.push(FetchCacheGcCandidate {
            kind: FetchCacheGcCandidateKind::Metadata,
            path: metadata_path.clone(),
            bytes,
        });
        if config.mode == BuildCacheGcMode::Execute {
            fs::remove_file(&metadata_path)
                .with_context(|| format!("failed to remove {}", metadata_path.display()))?;
        }
    }

    let blob_dir = layout.blobs_dir();
    if blob_dir.is_dir() {
        for entry in fs::read_dir(&blob_dir)
            .with_context(|| format!("failed to read {}", blob_dir.display()))?
        {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let blob_path = entry.path();
            let sha = entry.file_name().to_string_lossy().to_string();
            report.scanned_blobs += 1;
            if rooted_shas.contains(&sha) || grace_shas.contains(&sha) {
                continue;
            }
            if is_younger_than(&blob_path, now, grace)? {
                report.grace_blobs += 1;
                continue;
            }
            let bytes = file_size(&blob_path)?;
            report.evictable_blobs += 1;
            report.reclaimed_bytes = report.reclaimed_bytes.saturating_add(bytes);
            report.candidates.push(FetchCacheGcCandidate {
                kind: FetchCacheGcCandidateKind::Blob,
                path: blob_path.clone(),
                bytes,
            });
            if config.mode == BuildCacheGcMode::Execute {
                fs::remove_file(&blob_path)
                    .with_context(|| format!("failed to remove {}", blob_path.display()))?;
            }
        }
    }
    report.candidates.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(report)
}

fn fetch_metadata_paths(http_dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    if !http_dir.is_dir() {
        return Ok(paths);
    }
    collect_fetch_metadata_paths(http_dir, &mut paths)?;
    paths.sort();
    Ok(paths)
}

fn collect_fetch_metadata_paths(dir: &Path, paths: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to stat {}", path.display()))?;
        if file_type.is_dir() {
            collect_fetch_metadata_paths(&path, paths)?;
        } else if file_type.is_file()
            && path.extension().and_then(|value| value.to_str()) == Some("json")
        {
            paths.push(path);
        }
    }
    Ok(())
}

fn read_fetch_metadata(path: &Path) -> anyhow::Result<serde_json::Value> {
    serde_json::from_slice(
        &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))
}

fn file_size(path: &Path) -> anyhow::Result<u64> {
    Ok(fs::metadata(path)
        .with_context(|| format!("failed to stat {}", path.display()))?
        .len())
}

fn discover_publish_roots(published_root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    if !published_root.is_dir() {
        return Ok(roots);
    }
    for entry in fs::read_dir(published_root)
        .with_context(|| format!("failed to read {}", published_root.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path();
        if is_publish_root(&path) {
            roots.push(path);
            continue;
        }
        for child in
            fs::read_dir(&path).with_context(|| format!("failed to read {}", path.display()))?
        {
            let child = child?;
            if child.file_type()?.is_dir() && is_publish_root(&child.path()) {
                roots.push(child.path());
            }
        }
    }
    roots.sort();
    Ok(roots)
}

fn is_publish_root(path: &Path) -> bool {
    path.join("packaged").is_dir() || path.join("unpacked").is_dir()
}

fn remove_empty_publish_parent(published_root: &Path, publish_root: &Path) -> anyhow::Result<()> {
    let Some(parent) = publish_root.parent() else {
        return Ok(());
    };
    if parent == published_root {
        return Ok(());
    }
    match fs::remove_dir(parent) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::DirectoryNotEmpty => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("failed to remove {}", parent.display())),
    }
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
            chart_metadata_root: PathBuf::new(),
            build_root,
            publish_dir: publish_dir.clone(),
            packaged_dir: publish_dir.join("packaged"),
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
            fetch_cache_refs: Vec::new(),
        }
    }

    fn node_record_with_fetch_ref(name: &str, fingerprint: &str, sha256: &str) -> NodeRecord {
        let mut record = node_record(name, fingerprint);
        record.fetch_cache_refs.push(FetchCacheRef {
            cache_key: format!("https://example.test/{sha256}.zip"),
            url: format!("https://example.test/{sha256}.zip"),
            file: format!("{sha256}.zip"),
            sha256: sha256.to_string(),
            size_bytes: Some(4),
        });
        record
    }

    fn empty_build_cache_gc_report() -> BuildCacheGcReport {
        BuildCacheGcReport {
            roots_path: PathBuf::new(),
            rooted_nodes: 0,
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

    #[test]
    fn publication_gc_keeps_all_current_versions_and_evicts_unreferenced_roots() {
        let temp = tempdir().unwrap();
        let build_root = temp.path().join("artifacts");
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
        for (label, timestamp) in [
            ("nav6-sunset-abc", "20260609T000000Z"),
            ("master-def", "20260609T000010Z"),
            ("master-old", "20260601T000000Z"),
        ] {
            let root = build_root.join("published").join(label).join(timestamp);
            fs::create_dir_all(root.join("packaged")).unwrap();
            fs::create_dir_all(root.join("unpacked")).unwrap();
            fs::write(root.join("packaged").join("sentinel"), b"data").unwrap();
        }

        let report = gc_publication(&PublicationGcConfig {
            build_root: build_root.clone(),
            mode: BuildCacheGcMode::DryRun,
            grace_hours: 0,
        })
        .unwrap();
        assert_eq!(report.current_publish_roots, 2);
        assert_eq!(report.scanned_publish_roots, 3);
        assert_eq!(report.evictable_roots, 1);
        assert_eq!(
            report.candidates[0].path,
            build_root
                .join("published")
                .join("master-old")
                .join("20260601T000000Z")
        );

        let report = gc_publication(&PublicationGcConfig {
            build_root: build_root.clone(),
            mode: BuildCacheGcMode::Execute,
            grace_hours: 0,
        })
        .unwrap();
        assert_eq!(report.evictable_roots, 1);
        assert!(!build_root
            .join("published")
            .join("master-old")
            .join("20260601T000000Z")
            .exists());
        assert!(build_root
            .join("published")
            .join("master-def")
            .join("20260609T000010Z")
            .exists());
    }

    #[test]
    fn fetch_cache_gc_keeps_manifest_referenced_blobs_and_evicts_unreferenced_entries() {
        let temp = tempdir().unwrap();
        let build_root = temp.path().join("artifacts");
        let current_artifacts = vec![current_manifest(
            "master-def",
            "20260609T000010Z",
            product_contracts::NAV_DB_CONTRACT_ID,
        )];
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
            "master-def",
            "20260609T000010Z",
            "2606",
            vec![node_record_with_fetch_ref(
                "chart-fetch",
                "fetch-fp",
                "keep",
            )],
        );

        let fetch_root = build_root.join("cache").join("fetch");
        let layout = CacheLayout::new(&fetch_root);
        fs::create_dir_all(layout.blobs_dir()).unwrap();
        fs::create_dir_all(layout.http_dir()).unwrap();
        fs::write(layout.blob_path("keep"), b"keep").unwrap();
        fs::write(layout.blob_path("drop"), b"drop").unwrap();
        fs::write(
            layout.http_metadata_path("https://example.test/keep.zip"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "cache_key": "https://example.test/keep.zip",
                "sha256": "keep",
                "url": "https://example.test/keep.zip",
                "file": "keep.zip",
                "size": 4,
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            layout.http_metadata_path("https://example.test/old-alias.zip"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "cache_key": "https://example.test/old-alias.zip",
                "sha256": "keep",
                "url": "https://example.test/old-alias.zip",
                "file": "old-alias.zip",
                "size": 4,
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            layout.http_metadata_path("https://example.test/drop.zip"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "cache_key": "https://example.test/drop.zip",
                "sha256": "drop",
                "url": "https://example.test/drop.zip",
                "file": "drop.zip",
                "size": 4,
            }))
            .unwrap(),
        )
        .unwrap();

        let report = gc_fetch_cache(&FetchCacheGcConfig {
            build_root: build_root.clone(),
            mode: BuildCacheGcMode::DryRun,
            grace_hours: 0,
        })
        .unwrap();
        assert_eq!(report.build_manifests, 1);
        assert_eq!(report.rooted_fetch_refs, 1);
        assert_eq!(report.rooted_blobs, 1);
        assert_eq!(report.evictable_blobs, 1);
        assert_eq!(report.evictable_metadata, 2);

        let report = gc_fetch_cache(&FetchCacheGcConfig {
            build_root: build_root.clone(),
            mode: BuildCacheGcMode::Execute,
            grace_hours: 0,
        })
        .unwrap();
        assert_eq!(report.evictable_blobs, 1);
        assert!(layout.blob_path("keep").exists());
        assert!(!layout.blob_path("drop").exists());
        assert!(layout
            .http_metadata_path("https://example.test/keep.zip")
            .exists());
        assert!(!layout
            .http_metadata_path("https://example.test/old-alias.zip")
            .exists());
        assert!(!layout
            .http_metadata_path("https://example.test/drop.zip")
            .exists());
    }

    #[test]
    fn fetch_cache_gc_requires_manifests_with_fetch_refs_before_execute() {
        let temp = tempdir().unwrap();
        let build_root = temp.path().join("artifacts");
        let current_artifacts = vec![current_manifest(
            "master-def",
            "20260609T000010Z",
            product_contracts::NAV_DB_CONTRACT_ID,
        )];
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
            "master-def",
            "20260609T000010Z",
            "2606",
            vec![node_record("old-fetch-node", "old-fp")],
        );

        let fetch_root = build_root.join("cache").join("fetch");
        let layout = CacheLayout::new(&fetch_root);
        fs::create_dir_all(layout.blobs_dir()).unwrap();
        fs::create_dir_all(layout.http_dir()).unwrap();
        fs::write(layout.blob_path("drop"), b"drop").unwrap();
        fs::write(
            layout.http_metadata_path("https://example.test/drop.zip"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "sha256": "drop",
                "url": "https://example.test/drop.zip",
                "file": "drop.zip",
            }))
            .unwrap(),
        )
        .unwrap();

        let report = gc_fetch_cache(&FetchCacheGcConfig {
            build_root: build_root.clone(),
            mode: BuildCacheGcMode::DryRun,
            grace_hours: 0,
        })
        .unwrap();
        assert!(report.missing_fetch_refs);
        assert_eq!(report.scanned_blobs, 0);
        assert_eq!(report.scanned_metadata, 0);
        assert!(report.candidates.is_empty());

        let error = gc_fetch_cache(&FetchCacheGcConfig {
            build_root,
            mode: BuildCacheGcMode::Execute,
            grace_hours: 0,
        })
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("current build manifests contain no fetch_cache_refs"));
    }

    #[test]
    fn tpp_render_unit_scrub_removes_only_plate_tiff_intermediates() {
        let temp = tempdir().unwrap();
        let cache_nodes_root = temp.path().join("cache").join("nodes");
        let node_dir = cache_nodes_root
            .join("tpp-se-render-unit")
            .join("render-fingerprint");
        let plates_dir = node_dir.join("work").join("plates").join("SEA");
        fs::create_dir_all(&plates_dir).unwrap();
        fs::write(plates_dir.join("SEA-IAP.tif"), b"temporary tiff").unwrap();
        fs::write(plates_dir.join("SEA-IAP.TIFF"), b"temporary tiff").unwrap();
        fs::write(plates_dir.join("SEA-IAP.png"), b"final png").unwrap();
        fs::write(node_dir.join("work").join("SOURCE.PDF"), b"source pdf").unwrap();
        fs::write(
            node_dir.join("work").join("source.tif"),
            b"non-plate source",
        )
        .unwrap();

        let mut report = empty_build_cache_gc_report();
        scrub_tpp_render_scratch_cache(&cache_nodes_root, BuildCacheGcMode::DryRun, &mut report)
            .unwrap();
        assert_eq!(report.scratch_files, 2);
        assert!(plates_dir.join("SEA-IAP.tif").exists());

        let mut report = empty_build_cache_gc_report();
        scrub_tpp_render_scratch_cache(&cache_nodes_root, BuildCacheGcMode::Execute, &mut report)
            .unwrap();

        assert_eq!(report.scratch_files, 2);
        assert_eq!(report.scratch_bytes, 28);
        assert!(!plates_dir.join("SEA-IAP.tif").exists());
        assert!(!plates_dir.join("SEA-IAP.TIFF").exists());
        assert!(plates_dir.join("SEA-IAP.png").exists());
        assert!(node_dir.join("work").join("SOURCE.PDF").exists());
        assert!(node_dir.join("work").join("source.tif").exists());
    }

    #[test]
    fn chart_render_scrub_keeps_tiles_and_removes_source_work_files() {
        let temp = tempdir().unwrap();
        let cache_nodes_root = temp.path().join("cache").join("nodes");
        let node_dir = cache_nodes_root
            .join("charts-sec-render")
            .join("render-fingerprint");
        let work_dir = node_dir.join("work").join("charts-sec");
        let tile_path = work_dir.join("tiles").join("0").join("1").join("2.webp");
        fs::create_dir_all(tile_path.parent().unwrap()).unwrap();
        fs::write(&tile_path, b"tile").unwrap();
        fs::write(work_dir.join("Seattle SEC.tif"), b"tiff").unwrap();
        fs::write(work_dir.join("Seattle.zip"), b"zip").unwrap();
        fs::write(work_dir.join("Seattle.vrt"), b"vrt").unwrap();
        fs::write(node_dir.join("build-record.json"), b"record").unwrap();

        let mut report = empty_build_cache_gc_report();
        scrub_chart_render_intermediates_cache(
            &cache_nodes_root,
            BuildCacheGcMode::DryRun,
            &mut report,
        )
        .unwrap();
        assert_eq!(report.scratch_files, 3);
        assert!(work_dir.join("Seattle SEC.tif").exists());

        let mut report = empty_build_cache_gc_report();
        scrub_chart_render_intermediates_cache(
            &cache_nodes_root,
            BuildCacheGcMode::Execute,
            &mut report,
        )
        .unwrap();

        assert_eq!(report.scratch_files, 3);
        assert_eq!(report.scratch_bytes, 10);
        assert!(tile_path.exists());
        assert!(node_dir.join("build-record.json").exists());
        assert!(!work_dir.join("Seattle SEC.tif").exists());
        assert!(!work_dir.join("Seattle.zip").exists());
        assert!(!work_dir.join("Seattle.vrt").exists());
    }

    #[test]
    fn water_mask_scrub_keeps_product_outputs_and_removes_sources() {
        let temp = tempdir().unwrap();
        let cache_nodes_root = temp.path().join("cache").join("nodes");
        let node_dir = cache_nodes_root
            .join("static-water-mask-nw")
            .join("water-fingerprint");
        let output_dir = node_dir.join("output");
        let tile_path = output_dir
            .join("tiles")
            .join("0")
            .join("1")
            .join("2.water.png");
        fs::create_dir_all(tile_path.parent().unwrap()).unwrap();
        fs::write(&tile_path, b"tile").unwrap();
        fs::write(output_dir.join("manifest.json"), b"{}").unwrap();
        fs::write(output_dir.join("water_mask_nw_test.zip"), b"zip").unwrap();
        fs::write(output_dir.join("source.geojson"), b"source").unwrap();
        let source_pages = output_dir.join("source-pages");
        fs::create_dir_all(&source_pages).unwrap();
        fs::write(source_pages.join("layer_9_chunk_00001.geojson"), b"page").unwrap();

        let mut report = empty_build_cache_gc_report();
        scrub_water_mask_intermediates_cache(
            &cache_nodes_root,
            BuildCacheGcMode::Execute,
            &mut report,
        )
        .unwrap();

        assert_eq!(report.scratch_files, 2);
        assert_eq!(report.scratch_bytes, 10);
        assert!(tile_path.exists());
        assert!(output_dir.join("manifest.json").exists());
        assert!(output_dir.join("water_mask_nw_test.zip").exists());
        assert!(!output_dir.join("source.geojson").exists());
        assert!(!source_pages.join("layer_9_chunk_00001.geojson").exists());
    }
}

pub(super) fn scrub_tpp_render_scratch_cache(
    cache_nodes_root: &Path,
    mode: BuildCacheGcMode,
    report: &mut BuildCacheGcReport,
) -> anyhow::Result<()> {
    if !cache_nodes_root.is_dir() {
        return Ok(());
    }
    for node_entry in fs::read_dir(cache_nodes_root)
        .with_context(|| format!("failed to read {}", cache_nodes_root.display()))?
    {
        let node_entry = node_entry?;
        if !node_entry.file_type()?.is_dir() {
            continue;
        }
        let node_name = node_entry.file_name().to_string_lossy().to_string();
        let Some(kind) = tpp_render_scratch_kind(&node_name) else {
            continue;
        };
        for fingerprint_entry in fs::read_dir(node_entry.path())
            .with_context(|| format!("failed to read {}", node_entry.path().display()))?
        {
            let fingerprint_entry = fingerprint_entry?;
            if !fingerprint_entry.file_type()?.is_dir() {
                continue;
            }
            let node_dir = fingerprint_entry.path();
            let lock_path = node_dir.join(".build-lock");
            if lock_path.exists() && lock_is_live(&lock_path)? {
                report.scratch_active_nodes += 1;
                continue;
            }
            scrub_tpp_render_scratch_dir(&node_dir, kind, mode, report)?;
        }
    }
    Ok(())
}

pub(super) fn scrub_chart_render_intermediates_cache(
    cache_nodes_root: &Path,
    mode: BuildCacheGcMode,
    report: &mut BuildCacheGcReport,
) -> anyhow::Result<()> {
    if !cache_nodes_root.is_dir() {
        return Ok(());
    }
    for node_entry in fs::read_dir(cache_nodes_root)
        .with_context(|| format!("failed to read {}", cache_nodes_root.display()))?
    {
        let node_entry = node_entry?;
        if !node_entry.file_type()?.is_dir() {
            continue;
        }
        let node_name = node_entry.file_name().to_string_lossy().to_string();
        if !is_chart_render_node_name(&node_name) {
            continue;
        }
        for fingerprint_entry in fs::read_dir(node_entry.path())
            .with_context(|| format!("failed to read {}", node_entry.path().display()))?
        {
            let fingerprint_entry = fingerprint_entry?;
            if !fingerprint_entry.file_type()?.is_dir() {
                continue;
            }
            let node_dir = fingerprint_entry.path();
            let lock_path = node_dir.join(".build-lock");
            if lock_path.exists() && lock_is_live(&lock_path)? {
                report.scratch_active_nodes += 1;
                continue;
            }
            let work_dir = node_dir.join("work");
            if work_dir.is_dir() {
                scrub_chart_render_intermediates_dir(&work_dir, false, mode, report)?;
            }
        }
    }
    Ok(())
}

pub(super) fn is_chart_render_node_name(node_name: &str) -> bool {
    node_name.starts_with("charts-") && node_name.ends_with("-render")
}

pub(super) fn scrub_chart_render_intermediates_dir(
    dir: &Path,
    in_tiles_dir: bool,
    mode: BuildCacheGcMode,
    report: &mut BuildCacheGcReport,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        let is_tiles_dir = entry.file_name().to_string_lossy() == "tiles";
        let child_in_tiles = in_tiles_dir || is_tiles_dir;
        if file_type.is_dir() {
            if !child_in_tiles {
                scrub_chart_render_intermediates_dir(&path, child_in_tiles, mode, report)?;
            }
            continue;
        }
        if !child_in_tiles {
            scrub_scratch_file(&path, mode, report)?;
        }
    }
    Ok(())
}

pub(super) fn scrub_water_mask_intermediates_cache(
    cache_nodes_root: &Path,
    mode: BuildCacheGcMode,
    report: &mut BuildCacheGcReport,
) -> anyhow::Result<()> {
    if !cache_nodes_root.is_dir() {
        return Ok(());
    }
    for node_entry in fs::read_dir(cache_nodes_root)
        .with_context(|| format!("failed to read {}", cache_nodes_root.display()))?
    {
        let node_entry = node_entry?;
        if !node_entry.file_type()?.is_dir() {
            continue;
        }
        let node_name = node_entry.file_name().to_string_lossy().to_string();
        if !node_name.starts_with("static-water-mask-") {
            continue;
        }
        for fingerprint_entry in fs::read_dir(node_entry.path())
            .with_context(|| format!("failed to read {}", node_entry.path().display()))?
        {
            let fingerprint_entry = fingerprint_entry?;
            if !fingerprint_entry.file_type()?.is_dir() {
                continue;
            }
            let node_dir = fingerprint_entry.path();
            let lock_path = node_dir.join(".build-lock");
            if lock_path.exists() && lock_is_live(&lock_path)? {
                report.scratch_active_nodes += 1;
                continue;
            }
            scrub_water_mask_intermediates_dir(&node_dir.join("output"), mode, report)?;
        }
    }
    Ok(())
}

pub(super) fn scrub_water_mask_intermediates_dir(
    output_dir: &Path,
    mode: BuildCacheGcMode,
    report: &mut BuildCacheGcReport,
) -> anyhow::Result<()> {
    let source_geojson = output_dir.join("source.geojson");
    if source_geojson.is_file() {
        scrub_scratch_file(&source_geojson, mode, report)?;
    }
    let source_pages = output_dir.join("source-pages");
    if source_pages.is_dir() {
        scrub_scratch_dir_files(&source_pages, mode, report)?;
    }
    Ok(())
}

pub(super) fn scrub_scratch_dir_files(
    dir: &Path,
    mode: BuildCacheGcMode,
    report: &mut BuildCacheGcReport,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            scrub_scratch_dir_files(&path, mode, report)?;
            continue;
        }
        scrub_scratch_file(&path, mode, report)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TppRenderScratchKind {
    RegionRender,
    RenderUnit,
}

pub(super) fn tpp_render_scratch_kind(node_name: &str) -> Option<TppRenderScratchKind> {
    if !node_name.starts_with("tpp-") {
        return None;
    }
    if node_name.ends_with("-render") {
        return Some(TppRenderScratchKind::RegionRender);
    }
    if node_name.ends_with("-render-unit") {
        return Some(TppRenderScratchKind::RenderUnit);
    }
    None
}

pub(super) fn scrub_tpp_render_scratch_dir(
    dir: &Path,
    kind: TppRenderScratchKind,
    mode: BuildCacheGcMode,
    report: &mut BuildCacheGcReport,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            scrub_tpp_render_scratch_dir(&path, kind, mode, report)?;
            continue;
        }
        if !is_tpp_render_scratch_file(&path, kind) {
            continue;
        }
        scrub_scratch_file(&path, mode, report)?;
    }
    Ok(())
}

pub(super) fn scrub_scratch_file(
    path: &Path,
    mode: BuildCacheGcMode,
    report: &mut BuildCacheGcReport,
) -> anyhow::Result<()> {
    let bytes = fs::metadata(path)
        .with_context(|| format!("failed to stat {}", path.display()))?
        .len();
    report.scratch_files += 1;
    report.scratch_bytes = report.scratch_bytes.saturating_add(bytes);
    if mode == BuildCacheGcMode::Execute {
        set_path_readonly(path, false)?;
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

pub(super) fn is_tpp_render_scratch_file(path: &Path, kind: TppRenderScratchKind) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    if kind == TppRenderScratchKind::RegionRender && extension.eq_ignore_ascii_case("pdf") {
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
