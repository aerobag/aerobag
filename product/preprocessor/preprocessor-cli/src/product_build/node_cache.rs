use super::*;

pub(super) fn try_load_node_record(
    prepared: &PreparedNode,
    expected_outputs: &[PathBuf],
) -> anyhow::Result<Option<NodeRecord>> {
    if !prepared.record_path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&prepared.record_path)
        .with_context(|| format!("failed to read {}", prepared.record_path.display()))?;
    let record: NodeRecord =
        serde_json::from_slice(&bytes).context("failed to parse node record")?;
    if record.fingerprint != prepared.fingerprint {
        return Ok(None);
    }
    if expected_outputs.iter().all(|path| path.exists()) {
        let mut cached = record;
        cached.cache_hit = true;
        return Ok(Some(cached));
    }
    Ok(None)
}

pub(super) fn claim_or_wait_for_node(
    prepared: &PreparedNode,
    expected_outputs: &[PathBuf],
) -> anyhow::Result<NodeCacheState> {
    loop {
        if let Some(record) = try_load_node_record(prepared, expected_outputs)? {
            return Ok(NodeCacheState::CacheHit(record));
        }

        // Do not recursively chmod before attempting the lock. Another build may already own
        // this node and be creating/removing renderer scratch files; walking that active tree
        // can race with transient ImageMagick files such as `*.png~`. We only need the node
        // root writable here so the lock file can be created or a stale lock can be removed.
        set_path_readonly(&prepared.dir, false)?;
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&prepared.lock_path)
        {
            Ok(mut file) => {
                let pid = std::process::id();
                let now = utc_now_string();
                use std::io::Write as _;
                writeln!(file, "pid={pid}").ok();
                writeln!(file, "started_at_utc={now}").ok();
                reset_node_dir_for_rebuild(prepared)?;
                return Ok(NodeCacheState::Build(BuildLockGuard {
                    path: prepared.lock_path.clone(),
                    node_dir: prepared.dir.clone(),
                }));
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                remove_stale_lock_if_needed(&prepared.lock_path)?;
                thread::sleep(std::time::Duration::from_millis(250));
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!("failed to acquire {}", prepared.lock_path.display())
                });
            }
        }
    }
}

pub(super) fn run_cached_node<F>(
    prepared: PreparedNode,
    inputs: BTreeMap<String, String>,
    expected_outputs: &[PathBuf],
    build: F,
) -> anyhow::Result<NodeRecord>
where
    F: FnOnce(&PreparedNode) -> anyhow::Result<BTreeMap<String, String>>,
{
    let _build_lock = match claim_or_wait_for_node(&prepared, expected_outputs)? {
        NodeCacheState::CacheHit(record) => return Ok(record),
        NodeCacheState::Build(lock) => lock,
    };
    let started_at_utc = utc_now_string();
    let started = Instant::now();
    let outputs = build(&prepared)?;
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

pub(super) fn reset_node_dir_for_rebuild(prepared: &PreparedNode) -> anyhow::Result<()> {
    set_tree_readonly(&prepared.dir, false)?;
    for entry in fs::read_dir(&prepared.dir)
        .with_context(|| format!("failed to read {}", prepared.dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path == prepared.lock_path {
            continue;
        }
        if entry.file_type()?.is_dir() {
            fs::remove_dir_all(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        } else {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        }
    }
    Ok(())
}

pub(super) fn set_tree_readonly(root: &Path, readonly: bool) -> anyhow::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    if readonly {
        for entry in
            fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                set_tree_readonly(&path, true)?;
            } else {
                set_path_readonly(&path, true)?;
            }
        }
        set_path_readonly(root, true)?;
    } else {
        set_path_readonly(root, false)?;
        for entry in
            fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                set_tree_readonly(&path, false)?;
            } else {
                set_path_readonly(&path, false)?;
            }
        }
    }
    Ok(())
}

pub(super) fn set_path_readonly(path: &Path, readonly: bool) -> anyhow::Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    let mut permissions = metadata.permissions();
    let mut mode = permissions.mode();
    if readonly {
        mode &= !0o222;
    } else if metadata.is_dir() {
        mode |= 0o700;
    } else {
        mode |= 0o600;
    }
    permissions.set_mode(mode);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to chmod {}", path.display()))
}

pub(super) fn remove_stale_lock_if_needed(lock_path: &Path) -> anyhow::Result<()> {
    if !lock_path.is_file() {
        return Ok(());
    }
    let Some(pid) = read_lock_pid(lock_path)? else {
        return Ok(());
    };
    if process_is_alive(pid) {
        return Ok(());
    }
    match fs::remove_file(lock_path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => {
            Err(err).with_context(|| format!("failed to remove stale {}", lock_path.display()))
        }
    }
}

pub(super) fn read_lock_pid(lock_path: &Path) -> anyhow::Result<Option<u32>> {
    let text = fs::read_to_string(lock_path)
        .with_context(|| format!("failed to read {}", lock_path.display()))?;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("pid=") {
            return Ok(value.trim().parse::<u32>().ok());
        }
    }
    Ok(None)
}

pub(super) fn process_is_alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

pub(super) fn acquire_publication_lock<F>(
    build_root: &Path,
    log: F,
) -> anyhow::Result<PublicationLockGuard>
where
    F: FnMut(&str),
{
    let lock_name = build_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("published")
        .to_string();
    acquire_named_publication_lock(build_root, &lock_name, log)
}

pub(super) fn acquire_named_publication_lock<F>(
    build_root: &Path,
    lock_name: &str,
    mut log: F,
) -> anyhow::Result<PublicationLockGuard>
where
    F: FnMut(&str),
{
    let lock_dir = artifact_root_from_build_root(build_root)
        .join("private-work")
        .join("publication-locks");
    fs::create_dir_all(&lock_dir)
        .with_context(|| format!("failed to create {}", lock_dir.display()))?;
    let lock_path = lock_dir.join(format!("{lock_name}.lock"));
    let mut logged_wait = false;
    loop {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                let pid = std::process::id();
                let now = utc_now_string();
                writeln!(file, "pid={pid}").ok();
                writeln!(file, "started_at_utc={now}").ok();
                log(&format!(
                    "publication-lock acquired path={}",
                    lock_path.display()
                ));
                return Ok(PublicationLockGuard { path: lock_path });
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                remove_stale_lock_if_needed(&lock_path)?;
                if !logged_wait {
                    log(&format!(
                        "publication-lock waiting path={}",
                        lock_path.display()
                    ));
                    logged_wait = true;
                }
                thread::sleep(Duration::from_secs(2));
            }
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("failed to acquire {}", lock_path.display()));
            }
        }
    }
}

pub(super) fn normalize_node_record_paths(mut record: NodeRecord, build_root: &Path) -> NodeRecord {
    record.outputs = record
        .outputs
        .into_iter()
        .map(|(key, value)| {
            let normalized = if value.starts_with('/') {
                relative_artifact_path(Path::new(&value), build_root)
            } else {
                value
            };
            (key, normalized)
        })
        .collect();
    record.output_details = record
        .output_details
        .into_iter()
        .map(|(key, mut detail)| {
            if detail.path.starts_with('/') {
                detail.path = relative_artifact_path(Path::new(&detail.path), build_root);
            }
            (key, detail)
        })
        .collect();
    record
}

pub(super) fn write_node_record(
    prepared: PreparedNode,
    inputs: BTreeMap<String, String>,
    outputs: BTreeMap<String, String>,
    cache_hit: bool,
    started_at_utc: String,
    finished_at_utc: String,
    elapsed_ms: u64,
) -> anyhow::Result<NodeRecord> {
    let output_details = node_output_details(&prepared.dir, &outputs)?;
    let record = NodeRecord {
        name: prepared.name,
        fingerprint: prepared.fingerprint,
        started_at_utc,
        finished_at_utc,
        elapsed_ms,
        cache_hit,
        inputs,
        outputs,
        output_details,
    };
    fs::write(
        &prepared.record_path,
        serde_json::to_vec_pretty(&record).context("failed to encode node record")?,
    )
    .with_context(|| format!("failed to write {}", prepared.record_path.display()))?;
    Ok(record)
}

pub(super) fn node_output_details(
    node_dir: &Path,
    outputs: &BTreeMap<String, String>,
) -> anyhow::Result<BTreeMap<String, NodeOutputDetail>> {
    outputs
        .iter()
        .map(|(key, value)| {
            let resolved = resolve_recorded_output_path(node_dir, value);
            let (sha256, size_bytes) = match resolved.as_deref() {
                Some(path) if path.is_file() => {
                    let metadata = fs::metadata(path)
                        .with_context(|| format!("failed to stat {}", path.display()))?;
                    (Some(hash_file(path)?), Some(metadata.len()))
                }
                _ => (None, None),
            };
            Ok((
                key.clone(),
                NodeOutputDetail {
                    path: value.clone(),
                    sha256,
                    size_bytes,
                },
            ))
        })
        .collect()
}

pub(super) fn resolve_recorded_output_path(node_dir: &Path, value: &str) -> Option<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute() {
        return path.exists().then(|| path.to_path_buf());
    }
    for ancestor in node_dir.ancestors() {
        let candidate = ancestor.join(path);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

pub(super) fn load_existing_node_record(
    record_path: &Path,
    expected_name: &str,
) -> anyhow::Result<NodeRecord> {
    let bytes = fs::read(record_path)
        .with_context(|| format!("failed to read {}", record_path.display()))?;
    let record: NodeRecord =
        serde_json::from_slice(&bytes).context("failed to parse node record")?;
    if record.name != expected_name {
        bail!(
            "node record {} had unexpected name {}",
            record_path.display(),
            record.name
        );
    }
    Ok(record)
}
