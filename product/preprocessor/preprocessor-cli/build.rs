// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn collect_hash_inputs(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_hash_inputs_into(root, root, &mut files);
    files.sort();
    files
}

fn collect_hash_inputs_into(root: &Path, path: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let child = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if child
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == "target" || name == ".git")
            {
                continue;
            }
            collect_hash_inputs_into(root, &child, files);
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let include = child
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext == "rs")
            || child
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == "Cargo.toml" || name == "Cargo.lock");
        if include {
            files.push(
                child
                    .strip_prefix(root)
                    .expect("hashed file should live under root")
                    .to_path_buf(),
            );
        }
    }
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let workspace_root = manifest_dir
        .parent()
        .expect("preprocessor-cli should live under workspace root")
        .to_path_buf();
    let mut hasher = Sha256::new();
    for relative in collect_hash_inputs(&workspace_root) {
        let full_path = workspace_root.join(&relative);
        println!("cargo:rerun-if-changed={}", full_path.display());
        hasher.update(relative.to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update(fs::read(&full_path).expect("read hashed input"));
        hasher.update([0xff]);
    }
    println!(
        "cargo:rustc-env=PREPROCESSOR_WORKSPACE_HASH={:x}",
        hasher.finalize()
    );
}
