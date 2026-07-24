// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::{Path, PathBuf};

use anyhow::Context;
use preprocessor_fetch::hash_file;
use sha2::{Digest, Sha256};

fn crate_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn workspace_root() -> PathBuf {
    crate_root()
        .parent()
        .expect("preprocessor-cli should live under workspace root")
        .to_path_buf()
}

fn repo_root() -> PathBuf {
    workspace_root()
        .parent()
        .expect("workspace root should live under product")
        .parent()
        .expect("product should live under repo root")
        .to_path_buf()
}

fn hash_sources(label: &str, paths: &[PathBuf]) -> anyhow::Result<String> {
    let mut hasher = Sha256::new();
    let repo_root = repo_root();
    hasher.update(label.as_bytes());
    hasher.update([0xff]);
    for path in paths {
        let source_identity = source_hash_identity(path, &repo_root);
        hasher.update(source_identity.to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update(
            hash_file(path)
                .with_context(|| format!("failed to hash source {}", path.display()))?
                .as_bytes(),
        );
        hasher.update([0xff]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn source_hash_identity<'a>(path: &'a Path, repo_root: &'a Path) -> &'a Path {
    path.strip_prefix(repo_root).unwrap_or(path)
}

fn nav_kv_builder_source_paths() -> Vec<PathBuf> {
    let crate_root = crate_root();
    let workspace_root = workspace_root();
    let repo_root = repo_root();
    vec![
        crate_root.join("src/product_build/nav_db.rs"),
        workspace_root.join("preprocessor-core/src/lib.rs"),
        workspace_root.join("preprocessor-procedure-geometry/src/lib.rs"),
        workspace_root.join("preprocessor-procedure-geometry/src/arinc_ambiguity_resolutions.rs"),
        workspace_root.join("preprocessor-procedure-geometry/src/procedure_geometry.rs"),
        workspace_root.join("preprocessor-procedure-geometry/src/procedure_geometry_constants.rs"),
        workspace_root.join("preprocessor-procedure-geometry/src/procedure_legs.rs"),
        repo_root.join("crates/procedure-geometry-types/src/lib.rs"),
        repo_root.join("crates/had-key/src/lib.rs"),
        repo_root.join("crates/had-nav-kv/src/lib.rs"),
    ]
}

pub(super) fn nav_kv_builder_fingerprint() -> anyhow::Result<String> {
    let source_hash = hash_sources("nav-kv-builder-v2", &nav_kv_builder_source_paths())?;
    let constants = serde_json::json!({
        "nav_db_contract_id": super::NAV_DB_CONTRACT_ID,
        "waypoint_prefix_max_results": super::WAYPOINT_PREFIX_MAX_RESULTS,
        "offline_chart_region_simplify_tolerance_degrees": super::OFFLINE_CHART_REGION_SIMPLIFY_TOLERANCE_DEGREES,
        "offline_chart_region_union_snap_grid_degrees": super::OFFLINE_CHART_REGION_UNION_SNAP_GRID_DEGREES,
        "offline_chart_region_union_expand_degrees": super::OFFLINE_CHART_REGION_UNION_EXPAND_DEGREES,
    });
    let mut hasher = Sha256::new();
    hasher.update(source_hash.as_bytes());
    hasher.update([0xff]);
    hasher.update(constants.to_string().as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

fn terrain_discovery_builder_source_paths() -> Vec<PathBuf> {
    let crate_root = crate_root();
    let workspace_root = workspace_root();
    vec![
        crate_root.join("src/product_build/static_products.rs"),
        workspace_root.join("preprocessor-fetch/src/lib.rs"),
    ]
}

pub(super) fn terrain_discovery_builder_fingerprint() -> anyhow::Result<String> {
    hash_sources(
        "terrain-discovery-builder-v1",
        &terrain_discovery_builder_source_paths(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_fingerprint_source_paths_exist() {
        for path in nav_kv_builder_source_paths()
            .into_iter()
            .chain(terrain_discovery_builder_source_paths())
        {
            assert!(
                path.is_file(),
                "missing builder fingerprint source {}",
                path.display()
            );
        }
    }

    #[test]
    fn builder_fingerprints_hash_successfully() {
        assert_eq!(nav_kv_builder_fingerprint().unwrap().len(), 64);
        assert_eq!(terrain_discovery_builder_fingerprint().unwrap().len(), 64);
    }

    #[test]
    fn builder_source_hash_uses_repo_relative_paths() {
        let path = crate_root().join("src/product_build/nav_db.rs");
        assert_eq!(
            source_hash_identity(&path, &repo_root()),
            Path::new("product/preprocessor/preprocessor-cli/src/product_build/nav_db.rs")
        );
    }
}
