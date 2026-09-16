// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, env, fs, path::Path};

fn collect(repo: &Path, path: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
    // Directory dependencies also detect source additions and deletions.
    println!("cargo:rerun-if-changed={}", path.display());
    for entry in fs::read_dir(path).expect("source directory") {
        let entry = entry.unwrap();
        let child = entry.path();
        if entry.file_type().unwrap().is_dir() {
            if !matches!(
                entry.file_name().to_str(),
                Some("target" | ".git" | "__pycache__")
            ) {
                collect(repo, &child, files);
            }
        } else if matches!(
            child.extension().and_then(|s| s.to_str()),
            Some("rs" | "py" | "toml" | "lock")
        ) {
            files.insert(
                child.strip_prefix(repo).unwrap().to_str().unwrap().into(),
                fs::read(child).unwrap(),
            );
        }
    }
}

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let repo = Path::new(&manifest).ancestors().nth(3).unwrap();
    let mut files = BTreeMap::new();
    for root in ["product/preprocessor", "crates"] {
        collect(repo, &repo.join(root), &mut files);
    }
    let file_hashes: BTreeMap<_, _> = files
        .iter()
        .map(|(path, bytes)| (path, format!("{:x}", Sha256::digest(bytes))))
        .collect();
    let mut trees = BTreeMap::new();
    for root in [
        "product/preprocessor/preprocessor-core",
        "product/preprocessor/preprocessor-vectors",
        "crates/airspace-geometry",
        "product/preprocessor/preprocessor-tpp/src",
        "product/preprocessor/preprocessor-tools/src",
        "product/preprocessor/preprocessor-core/src",
        "product/preprocessor/preprocessor-zip/src",
    ] {
        let mut hash = Sha256::new();
        for (path, bytes) in &files {
            if let Some(relative) = path.strip_prefix(&format!("{root}/")) {
                hash.update(relative.as_bytes());
                hash.update([0]);
                hash.update(bytes);
                hash.update([0]);
            }
        }
        trees.insert(root, format!("{:x}", hash.finalize()));
    }
    let out = env::var("OUT_DIR").unwrap();
    fs::write(
        Path::new(&out).join("compiled-sources.json"),
        serde_json::to_vec(&serde_json::json!({"files": file_hashes, "trees": trees})).unwrap(),
    )
    .unwrap();
}
