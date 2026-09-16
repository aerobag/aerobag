// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, env, fs, path::Path};

fn copy_tree(repo: &Path, path: &Path, out: &Path, files: &mut BTreeMap<String, String>) {
    println!("cargo:rerun-if-changed={}", path.display());
    assert!(
        !path.is_symlink(),
        "tool resources must not contain symlinks"
    );
    if path.is_dir() {
        for entry in fs::read_dir(path).expect("resource directory") {
            let child = entry.unwrap().path();
            if child.file_name().unwrap() != "__pycache__" {
                copy_tree(repo, &child, out, files);
            }
        }
    } else {
        let relative = path.strip_prefix(repo).unwrap();
        let bytes = fs::read(path).expect("read resource");
        let target = out.join(relative);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, &bytes).unwrap();
        files.insert(
            relative.to_str().unwrap().into(),
            format!("{:x}", Sha256::digest(&bytes)),
        );
    }
}

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let repo = Path::new(&manifest).ancestors().nth(3).unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();
    let out = Path::new(&out_dir).join("resources");
    if out.exists() {
        fs::remove_dir_all(&out).unwrap();
    }
    let mut files = BTreeMap::new();
    for relative in [
        "product/chart-metadata",
        "product/preprocessor/preprocessor-cli/scripts",
        "product/preprocessor/preprocessor-tpp/scripts",
    ] {
        copy_tree(repo, &repo.join(relative), &out, &mut files);
    }
    let encoded =
        serde_json::to_vec(&serde_json::json!({"schema_version": 1, "files": files})).unwrap();
    fs::write(out.join("manifest.json"), &encoded).unwrap();
    println!(
        "cargo:rustc-env=PREPROCESSOR_RESOURCE_DIGEST={:x}",
        Sha256::digest(&encoded)
    );
    println!(
        "cargo:rustc-env=PREPROCESSOR_DEV_RESOURCES={}",
        out.display()
    );
}
