// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
    time::SystemTime,
};

use anyhow::{bail, Context};
use product_contracts::publication::current::v1::CurrentArtifactsManifest;

use super::{
    active_current_artifacts_paths, build_manifests_root, publish_path_key, BuildCacheGcMode,
};

const BUILD_MANIFEST_HISTORY_DIRS: usize = 2;
const ROTATED_BUILD_LOGS: usize = 8;
const ROTATED_BUILD_LOG_MAX_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ArtifactRetentionGcReport {
    pub current_manifest_dirs: usize,
    pub retained_history_manifest_dirs: usize,
    pub evictable_manifest_dirs: usize,
    pub manifest_reclaimed_bytes: u64,
    pub retained_rotated_logs: usize,
    pub evictable_rotated_logs: usize,
    pub log_reclaimed_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
struct ArtifactRetentionPolicy {
    build_manifest_history_dirs: usize,
    rotated_build_logs: usize,
    rotated_build_log_max_bytes: u64,
}

#[derive(Debug)]
struct RetentionEntry {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}

pub fn gc_artifact_retention(
    build_root: &Path,
    mode: BuildCacheGcMode,
) -> anyhow::Result<ArtifactRetentionGcReport> {
    gc_artifact_retention_with_policy(
        build_root,
        mode,
        ArtifactRetentionPolicy {
            build_manifest_history_dirs: BUILD_MANIFEST_HISTORY_DIRS,
            rotated_build_logs: ROTATED_BUILD_LOGS,
            rotated_build_log_max_bytes: ROTATED_BUILD_LOG_MAX_BYTES,
        },
    )
}

fn gc_artifact_retention_with_policy(
    build_root: &Path,
    mode: BuildCacheGcMode,
    policy: ArtifactRetentionPolicy,
) -> anyhow::Result<ArtifactRetentionGcReport> {
    let current_manifest_keys = current_build_manifest_keys(build_root)?;
    let (current_manifest_dirs, retained_history_manifest_dirs, manifest_candidates) =
        build_manifest_candidates(
            &build_manifests_root(build_root),
            &current_manifest_keys,
            policy.build_manifest_history_dirs,
        )?;

    let mut retained_rotated_logs = 0;
    let mut log_candidates = Vec::new();
    for log_family in ["published", "publication"] {
        let log_root = build_root.join("logs/orchestrator").join(log_family);
        let (retained, mut candidates) = rotated_log_candidates(
            &log_root,
            policy.rotated_build_logs,
            policy.rotated_build_log_max_bytes,
        )?;
        retained_rotated_logs += retained;
        log_candidates.append(&mut candidates);
    }

    let report = ArtifactRetentionGcReport {
        current_manifest_dirs,
        retained_history_manifest_dirs,
        evictable_manifest_dirs: manifest_candidates.len(),
        manifest_reclaimed_bytes: manifest_candidates.iter().map(|entry| entry.bytes).sum(),
        retained_rotated_logs,
        evictable_rotated_logs: log_candidates.len(),
        log_reclaimed_bytes: log_candidates.iter().map(|entry| entry.bytes).sum(),
    };

    if mode == BuildCacheGcMode::Execute {
        for candidate in manifest_candidates {
            fs::remove_dir_all(&candidate.path)
                .with_context(|| format!("failed to remove {}", candidate.path.display()))?;
        }
        for candidate in log_candidates {
            fs::remove_file(&candidate.path)
                .with_context(|| format!("failed to remove {}", candidate.path.display()))?;
        }
    }
    Ok(report)
}

fn current_build_manifest_keys(build_root: &Path) -> anyhow::Result<BTreeSet<String>> {
    let mut keys = BTreeSet::new();
    for current_path in active_current_artifacts_paths(build_root)? {
        let manifests: Vec<CurrentArtifactsManifest> = serde_json::from_slice(
            &fs::read(&current_path)
                .with_context(|| format!("failed to read {}", current_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", current_path.display()))?;
        for manifest in manifests {
            let packaged = publish_dir_from_artifact_root(
                build_root,
                &manifest.artifact_roots.packaged,
                "packaged",
            )?;
            let unpacked = publish_dir_from_artifact_root(
                build_root,
                &manifest.artifact_roots.unpacked,
                "unpacked",
            )?;
            if packaged != unpacked {
                bail!(
                    "current artifact roots resolve to different publications: {} and {}",
                    packaged.display(),
                    unpacked.display()
                );
            }
            keys.insert(publish_path_key(&packaged, build_root));
        }
    }
    Ok(keys)
}

fn publish_dir_from_artifact_root(
    build_root: &Path,
    artifact_root: &str,
    expected_leaf: &str,
) -> anyhow::Result<PathBuf> {
    let relative = Path::new(artifact_root);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("invalid current artifact root {artifact_root}");
    }
    if relative.file_name().and_then(|name| name.to_str()) != Some(expected_leaf) {
        bail!("current artifact root {artifact_root} does not end in {expected_leaf}");
    }
    let publish_relative = relative.parent().ok_or_else(|| {
        anyhow::anyhow!("current artifact root has no publication parent: {artifact_root}")
    })?;
    Ok(build_root.join("published").join(publish_relative))
}

fn build_manifest_candidates(
    root: &Path,
    current_keys: &BTreeSet<String>,
    history_to_keep: usize,
) -> anyhow::Result<(usize, usize, Vec<RetentionEntry>)> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((0, 0, Vec::new()));
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", root.display()));
        }
    };
    let mut current_count = 0;
    let mut history = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let key = entry.file_name().to_string_lossy().to_string();
        if current_keys.contains(&key) {
            current_count += 1;
            continue;
        }
        let metadata = entry.metadata()?;
        history.push(RetentionEntry {
            bytes: directory_size(&entry.path())?,
            modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            path: entry.path(),
        });
    }
    history.sort_by(|left, right| {
        right
            .modified
            .cmp(&left.modified)
            .then_with(|| right.path.cmp(&left.path))
    });
    let retained = history.len().min(history_to_keep);
    let candidates = history.into_iter().skip(retained).collect();
    Ok((current_count, retained, candidates))
}

fn rotated_log_candidates(
    root: &Path,
    count_to_keep: usize,
    max_bytes: u64,
) -> anyhow::Result<(usize, Vec<RetentionEntry>)> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((0, Vec::new()));
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", root.display()));
        }
    };
    let mut rotated = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file()
            || entry.file_name() == "master.log"
            || entry.path().extension().and_then(|ext| ext.to_str()) != Some("log")
        {
            continue;
        }
        let metadata = entry.metadata()?;
        rotated.push(RetentionEntry {
            path: entry.path(),
            bytes: metadata.len(),
            modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        });
    }
    rotated.sort_by(|left, right| {
        right
            .modified
            .cmp(&left.modified)
            .then_with(|| right.path.cmp(&left.path))
    });

    let mut retained = 0;
    let mut retained_bytes = 0_u64;
    let mut candidates = Vec::new();
    for entry in rotated {
        if retained < count_to_keep && retained_bytes.saturating_add(entry.bytes) <= max_bytes {
            retained += 1;
            retained_bytes = retained_bytes.saturating_add(entry.bytes);
        } else {
            candidates.push(entry);
        }
    }
    Ok((retained, candidates))
}

fn directory_size(path: &Path) -> anyhow::Result<u64> {
    let mut bytes = 0_u64;
    for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            bytes = bytes.saturating_add(directory_size(&entry.path())?);
        } else if metadata.is_file() {
            bytes = bytes.saturating_add(metadata.len());
        }
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs::{File, FileTimes},
        io::Write,
        time::Duration,
    };

    use product_contracts::publication::current::v1::{CurrentArtifactRoots, SCHEMA_VERSION};
    use tempfile::tempdir;

    use super::*;

    fn write_sized_file(path: &Path, size: usize, modified: SystemTime) {
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        let mut file = File::create(path).expect("create file");
        file.write_all(&vec![b'x'; size]).expect("write file");
        file.set_times(FileTimes::new().set_modified(modified))
            .expect("set modified");
    }

    fn set_modified(path: &Path, modified: SystemTime) {
        File::open(path)
            .expect("open path")
            .set_times(FileTimes::new().set_modified(modified))
            .expect("set modified");
    }

    #[test]
    fn retains_current_and_two_newest_historical_build_manifests() {
        let temp = tempdir().expect("tempdir");
        let build_root = temp.path();
        fs::create_dir_all(build_root.join("published")).expect("published");
        let current = CurrentArtifactsManifest {
            schema_version: SCHEMA_VERSION,
            contracts: BTreeMap::new(),
            artifact_roots: CurrentArtifactRoots {
                packaged: "main-current/20260805T000000Z/packaged/".to_string(),
                unpacked: "main-current/20260805T000000Z/unpacked/".to_string(),
            },
            as_of_date: "2026-08-05".to_string(),
            as_of_utc: "2026-08-05T00:00:00Z".to_string(),
            bundles: Vec::new(),
            startup_prefetch: None,
            diagnostics: None,
        };
        fs::write(
            build_root.join("published/current_artifacts.json"),
            serde_json::to_vec(&vec![current]).expect("serialize"),
        )
        .expect("write current");

        let root = build_manifests_root(build_root);
        let now = SystemTime::now();
        for (name, age_hours) in [
            ("published_main-current_20260805T000000Z", 10),
            ("old-one", 3),
            ("old-two", 2),
            ("old-three", 1),
        ] {
            let dir = root.join(name);
            write_sized_file(&dir.join("manifest.json"), 10, now);
            set_modified(&dir, now - Duration::from_secs(age_hours * 3600));
        }

        let report = gc_artifact_retention_with_policy(
            build_root,
            BuildCacheGcMode::Execute,
            ArtifactRetentionPolicy {
                build_manifest_history_dirs: 2,
                rotated_build_logs: 0,
                rotated_build_log_max_bytes: 0,
            },
        )
        .expect("gc");

        assert_eq!(report.current_manifest_dirs, 1);
        assert_eq!(report.retained_history_manifest_dirs, 2);
        assert_eq!(report.evictable_manifest_dirs, 1);
        assert!(root
            .join("published_main-current_20260805T000000Z")
            .exists());
        assert!(!root.join("old-one").exists());
        assert!(root.join("old-two").exists());
        assert!(root.join("old-three").exists());
    }

    #[test]
    fn bounds_rotated_logs_by_count_and_total_bytes() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let now = SystemTime::now();
        write_sized_file(&root.join("master.log"), 100, now);
        for (name, size, age) in [
            ("master-new.log", 40, 1),
            ("master-middle.log", 40, 2),
            ("master-old.log", 40, 3),
        ] {
            write_sized_file(
                &root.join(name),
                size,
                now - Duration::from_secs(age * 3600),
            );
        }

        let (retained, candidates) = rotated_log_candidates(root, 3, 70).expect("candidates");

        assert_eq!(retained, 1);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates.iter().map(|entry| entry.bytes).sum::<u64>(), 80);
        assert!(!candidates
            .iter()
            .any(|entry| entry.path.ends_with("master.log")));
    }
}
