// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

use fs2::FileExt as _;

use crate::{StoreError, StoreResult};

#[derive(Debug, Clone)]
pub struct StorageLayout {
    root: PathBuf,
}

impl StorageLayout {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn live_root(&self) -> PathBuf {
        self.root.join("live")
    }

    pub fn database_path(&self) -> PathBuf {
        self.live_root().join("cloud.sqlite3")
    }

    pub fn blob_root(&self) -> PathBuf {
        self.live_root().join("blobs")
    }

    pub fn snapshots_root(&self) -> PathBuf {
        self.root.join("snapshots")
    }

    pub fn recovery_root(&self) -> PathBuf {
        self.root.join("recovery")
    }

    pub fn locks_root(&self) -> PathBuf {
        self.root.join("locks")
    }

    pub fn ensure(&self) -> StoreResult<()> {
        let paths = [
            self.root.clone(),
            self.live_root(),
            self.blob_root(),
            self.snapshots_root(),
            self.recovery_root(),
            self.locks_root(),
        ];
        for path in paths {
            fs::create_dir_all(path)
                .map_err(|error| StoreError::io("create cloud storage directory", error))?;
        }
        Ok(())
    }

    pub fn acquire_serve_lock(&self) -> StoreResult<File> {
        self.acquire_lock("serve.lock", true, || {})
    }

    pub fn acquire_reclamation_lock(&self) -> StoreResult<File> {
        self.acquire_reclamation_lock_observing_contention(|| {})
    }

    pub(crate) fn acquire_reclamation_lock_observing_contention(
        &self,
        on_contention: impl FnOnce(),
    ) -> StoreResult<File> {
        self.acquire_lock("blob-reclamation.lock", false, on_contention)
    }

    fn acquire_lock(
        &self,
        name: &str,
        fail_if_busy: bool,
        on_contention: impl FnOnce(),
    ) -> StoreResult<File> {
        fs::create_dir_all(self.locks_root())
            .map_err(|error| StoreError::io("create cloud lock directory", error))?;
        let path = self.locks_root().join(name);
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|error| StoreError::io("open cloud storage lock", error))?;
        let result = match file.try_lock_exclusive() {
            Err(error)
                if error.raw_os_error() == fs2::lock_contended_error().raw_os_error()
                    && !fail_if_busy =>
            {
                // Observe actual kernel contention, not a guess based on how long
                // a worker takes to start. The normal path still blocks on flock.
                on_contention();
                file.lock_exclusive()
            }
            result => result,
        };
        result.map_err(|error| {
            StoreError::io(
                if fail_if_busy {
                    "cloud storage lock is already held"
                } else {
                    "acquire cloud storage lock"
                },
                error,
            )
        })?;
        Ok(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn reclamation_guard_retains_kernel_lock_until_drop() {
        let root = TempDir::new().unwrap();
        let layout = StorageLayout::new(root.path().to_path_buf());
        let guard = layout
            .acquire_reclamation_lock_observing_contention(|| {
                panic!("an uncontended acquisition must not report contention")
            })
            .unwrap();
        let probe = OpenOptions::new()
            .read(true)
            .write(true)
            .open(layout.locks_root().join("blob-reclamation.lock"))
            .unwrap();
        assert_eq!(
            probe.try_lock_exclusive().unwrap_err().raw_os_error(),
            fs2::lock_contended_error().raw_os_error()
        );
        drop(guard);
        probe.try_lock_exclusive().unwrap();
    }

    #[test]
    fn serve_lock_still_rejects_a_second_owner() {
        let root = TempDir::new().unwrap();
        let layout = StorageLayout::new(root.path().to_path_buf());
        let guard = layout.acquire_serve_lock().unwrap();
        assert!(layout.acquire_serve_lock().is_err());
        drop(guard);
        layout.acquire_serve_lock().unwrap();
    }
}
