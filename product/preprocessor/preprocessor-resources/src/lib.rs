// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Version-matched resources. Production tools carry `resources/` beside the
//! executable; Cargo builds use their own build-output copy, never a checkout.
use anyhow::{bail, Context};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub const DIGEST: &str = env!("PREPROCESSOR_RESOURCE_DIGEST");

pub fn root() -> PathBuf {
    if let Some(root) = env::var_os("AEROBAG_PREPROCESSOR_RESOURCE_ROOT") {
        return PathBuf::from(root);
    }
    if let Ok(exe) = env::current_exe() {
        let bundled = exe.parent().unwrap().join("resources");
        // An incomplete bundle must fail validation, not fall back to dev data.
        if bundled.exists() || exe.parent().unwrap().join("tool-entry.json").exists() {
            return bundled;
        }
    }
    PathBuf::from(env!("PREPROCESSOR_DEV_RESOURCES"))
}

pub fn path(relative: &str) -> PathBuf {
    root().join(relative)
}

pub fn validate(root: &Path) -> anyhow::Result<()> {
    let encoded = fs::read(root.join("manifest.json")).context("missing tool resource manifest")?;
    if format!("{:x}", Sha256::digest(&encoded)) != DIGEST {
        bail!("tool resource manifest does not match this executable");
    }
    let doc: serde_json::Value = serde_json::from_slice(&encoded)?;
    for (relative, digest) in doc["files"].as_object().context("resource files")? {
        let bytes = fs::read(root.join(relative))
            .with_context(|| format!("missing tool resource {relative}"))?;
        if digest.as_str() != Some(format!("{:x}", Sha256::digest(&bytes)).as_str()) {
            bail!("tool resource content mismatch: {relative}");
        }
    }
    Ok(())
}
