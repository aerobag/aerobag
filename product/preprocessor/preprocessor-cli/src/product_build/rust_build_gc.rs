// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fs::{self, File, OpenOptions},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use anyhow::Context;

use super::BuildCacheGcMode;

const RUST_BUILD_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const RUST_BUILD_MAX_BYTES: u64 = 64 * 1024 * 1024 * 1024;
// Cargo does not garbage-collect target directories. Preserve runnable binaries,
// but bound the compiler-owned profile caches by age and an emergency size cap.
const MANAGED_PROFILE_DIRS: [&str; 5] =
    [".fingerprint", "build", "deps", "examples", "incremental"];

#[derive(Debug, Clone)]
pub struct RustBuildCacheGcReport {
    pub target_root: PathBuf,
    pub retention_hours: u64,
    pub max_bytes: u64,
    pub profile_roots: usize,
    pub scanned_entries: usize,
    pub evictable_entries: usize,
    pub managed_bytes: u64,
    pub reclaimed_bytes: u64,
    pub retained_bytes: u64,
    pub pressure_purge: bool,
    pub skipped_locked: bool,
}

#[derive(Debug, Clone, Copy)]
struct RustBuildCacheGcPolicy {
    retention: Duration,
    max_bytes: u64,
}

#[derive(Debug)]
struct CacheEntry {
    path: PathBuf,
    bytes: u64,
    evict: bool,
}

pub fn gc_rust_build_cache(
    build_root: &Path,
    mode: BuildCacheGcMode,
) -> anyhow::Result<RustBuildCacheGcReport> {
    gc_rust_build_cache_with_policy(
        build_root,
        mode,
        RustBuildCacheGcPolicy {
            retention: RUST_BUILD_RETENTION,
            max_bytes: RUST_BUILD_MAX_BYTES,
        },
        SystemTime::now(),
    )
}

fn gc_rust_build_cache_with_policy(
    build_root: &Path,
    mode: BuildCacheGcMode,
    policy: RustBuildCacheGcPolicy,
    now: SystemTime,
) -> anyhow::Result<RustBuildCacheGcReport> {
    let target_root = build_root.join("target");
    let profile_roots = cargo_profile_roots(&target_root)?;
    let mut report = RustBuildCacheGcReport {
        target_root: target_root.clone(),
        retention_hours: policy.retention.as_secs() / 3600,
        max_bytes: policy.max_bytes,
        profile_roots: profile_roots.len(),
        scanned_entries: 0,
        evictable_entries: 0,
        managed_bytes: 0,
        reclaimed_bytes: 0,
        retained_bytes: 0,
        pressure_purge: false,
        skipped_locked: false,
    };
    if profile_roots.is_empty() {
        return Ok(report);
    }

    let Some(_locks) = try_lock_cargo_profiles(&profile_roots)? else {
        report.skipped_locked = true;
        return Ok(report);
    };

    let cutoff = now
        .checked_sub(policy.retention)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut entries = Vec::new();
    for profile_root in &profile_roots {
        for managed_name in MANAGED_PROFILE_DIRS {
            let managed_root = profile_root.join(managed_name);
            let children = match fs::read_dir(&managed_root) {
                Ok(children) => children,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("failed to read {}", managed_root.display()));
                }
            };
            for child in children {
                let path = child?.path();
                let (bytes, modified) = tree_stats(&path)?;
                entries.push(CacheEntry {
                    path,
                    bytes,
                    evict: modified <= cutoff,
                });
            }
        }
    }

    report.scanned_entries = entries.len();
    report.managed_bytes = entries.iter().map(|entry| entry.bytes).sum();
    let stale_bytes: u64 = entries
        .iter()
        .filter(|entry| entry.evict)
        .map(|entry| entry.bytes)
        .sum();
    if report.managed_bytes.saturating_sub(stale_bytes) > policy.max_bytes {
        report.pressure_purge = true;
        for entry in &mut entries {
            entry.evict = true;
        }
    }

    report.evictable_entries = entries.iter().filter(|entry| entry.evict).count();
    report.reclaimed_bytes = entries
        .iter()
        .filter(|entry| entry.evict)
        .map(|entry| entry.bytes)
        .sum();
    report.retained_bytes = report.managed_bytes.saturating_sub(report.reclaimed_bytes);

    if mode == BuildCacheGcMode::Execute {
        for entry in entries.into_iter().filter(|entry| entry.evict) {
            remove_path(&entry.path)?;
        }
    }
    Ok(report)
}

fn cargo_profile_roots(target_root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    if !target_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut roots = Vec::new();
    for profile in ["debug", "release"] {
        let path = target_root.join(profile);
        if path.is_dir() {
            roots.push(path);
        }
    }
    for child in fs::read_dir(target_root)
        .with_context(|| format!("failed to read {}", target_root.display()))?
    {
        let child = child?;
        if !child.file_type()?.is_dir() {
            continue;
        }
        let name = child.file_name();
        if name == "debug" || name == "release" {
            continue;
        }
        for profile in ["debug", "release"] {
            let path = child.path().join(profile);
            if path.is_dir() {
                roots.push(path);
            }
        }
    }
    roots.sort();
    roots.dedup();
    Ok(roots)
}

fn try_lock_cargo_profiles(profile_roots: &[PathBuf]) -> anyhow::Result<Option<Vec<File>>> {
    let mut locks = Vec::new();
    for profile_root in profile_roots {
        let lock_path = profile_root.join(".cargo-lock");
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lock_path)
            .with_context(|| format!("failed to open {}", lock_path.display()))?;
        let result = unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            if error
                .raw_os_error()
                .is_some_and(|code| code == libc::EAGAIN || code == libc::EWOULDBLOCK)
            {
                return Ok(None);
            }
            return Err(error).with_context(|| format!("failed to lock {}", lock_path.display()));
        }
        locks.push(lock);
    }
    Ok(Some(locks))
}

fn tree_stats(path: &Path) -> anyhow::Result<(u64, SystemTime)> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    let mut bytes = if metadata.is_file() {
        metadata.len()
    } else {
        0
    };
    let mut modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    if metadata.is_dir() {
        for child in
            fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))?
        {
            let (child_bytes, child_modified) = tree_stats(&child?.path())?;
            bytes = bytes.saturating_add(child_bytes);
            modified = modified.max(child_modified);
        }
    }
    Ok((bytes, modified))
}

fn remove_path(path: &Path) -> anyhow::Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    if metadata.is_dir() {
        fs::remove_dir_all(path).with_context(|| format!("failed to remove {}", path.display()))
    } else {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use std::{fs::FileTimes, io::Write};

    use tempfile::tempdir;

    use super::*;

    fn write_file(path: &Path, size: usize, modified: SystemTime) {
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        let mut file = File::create(path).expect("create file");
        file.write_all(&vec![b'x'; size]).expect("write file");
        file.set_times(FileTimes::new().set_modified(modified))
            .expect("set modified");
    }

    #[test]
    fn removes_stale_cargo_entries_but_preserves_recent_entries_and_binaries() {
        let temp = tempdir().expect("tempdir");
        let now = SystemTime::now();
        let profile = temp.path().join("target/debug");
        write_file(
            &profile.join("deps/old.rlib"),
            20,
            now - Duration::from_secs(7200),
        );
        write_file(&profile.join("deps/recent.rlib"), 30, now);
        write_file(&profile.join("preprocessor-cli"), 40, now);

        let report = gc_rust_build_cache_with_policy(
            temp.path(),
            BuildCacheGcMode::Execute,
            RustBuildCacheGcPolicy {
                retention: Duration::from_secs(3600),
                max_bytes: 1024,
            },
            now,
        )
        .expect("gc");

        assert_eq!(report.reclaimed_bytes, 20);
        assert!(!profile.join("deps/old.rlib").exists());
        assert!(profile.join("deps/recent.rlib").exists());
        assert!(profile.join("preprocessor-cli").exists());
    }

    #[test]
    fn pressure_purge_bounds_managed_cache_and_preserves_binaries() {
        let temp = tempdir().expect("tempdir");
        let now = SystemTime::now();
        let profile = temp.path().join("target/debug");
        write_file(&profile.join("deps/a.rlib"), 60, now);
        write_file(&profile.join("incremental/b/object.o"), 60, now);
        write_file(&profile.join("preprocessor-cli"), 40, now);

        let report = gc_rust_build_cache_with_policy(
            temp.path(),
            BuildCacheGcMode::Execute,
            RustBuildCacheGcPolicy {
                retention: Duration::from_secs(3600),
                max_bytes: 100,
            },
            now,
        )
        .expect("gc");

        assert!(report.pressure_purge);
        assert_eq!(report.reclaimed_bytes, 120);
        assert!(profile.join("preprocessor-cli").exists());
        assert!(!profile.join("deps/a.rlib").exists());
        assert!(!profile.join("incremental/b").exists());
    }

    #[test]
    fn dry_run_reports_without_removing_entries() {
        let temp = tempdir().expect("tempdir");
        let now = SystemTime::now();
        let path = temp.path().join("target/debug/deps/old.rlib");
        write_file(&path, 20, now - Duration::from_secs(7200));

        let report = gc_rust_build_cache_with_policy(
            temp.path(),
            BuildCacheGcMode::DryRun,
            RustBuildCacheGcPolicy {
                retention: Duration::from_secs(3600),
                max_bytes: 1024,
            },
            now,
        )
        .expect("gc");

        assert_eq!(report.reclaimed_bytes, 20);
        assert!(path.exists());
    }

    #[test]
    fn skips_cleanup_while_cargo_profile_lock_is_held() {
        let temp = tempdir().expect("tempdir");
        let now = SystemTime::now();
        let profile = temp.path().join("target/debug");
        let path = profile.join("deps/old.rlib");
        write_file(&path, 20, now - Duration::from_secs(7200));
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(profile.join(".cargo-lock"))
            .expect("open lock");
        assert_eq!(
            unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );

        let report = gc_rust_build_cache_with_policy(
            temp.path(),
            BuildCacheGcMode::Execute,
            RustBuildCacheGcPolicy {
                retention: Duration::from_secs(3600),
                max_bytes: 1024,
            },
            now,
        )
        .expect("gc");

        assert!(report.skipped_locked);
        assert!(path.exists());
    }
}
