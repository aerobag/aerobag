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
        self.acquire_lock("serve.lock", true)
    }

    pub fn acquire_reclamation_lock(&self) -> StoreResult<File> {
        self.acquire_lock("blob-reclamation.lock", false)
    }

    fn acquire_lock(&self, name: &str, fail_if_busy: bool) -> StoreResult<File> {
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
        let result = if fail_if_busy {
            file.try_lock_exclusive()
        } else {
            file.lock_exclusive()
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
