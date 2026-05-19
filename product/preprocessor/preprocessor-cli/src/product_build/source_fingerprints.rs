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
    hasher.update(label.as_bytes());
    hasher.update([0xff]);
    for path in paths {
        hasher.update(path.to_string_lossy().as_bytes());
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

pub(super) fn nav_kv_builder_fingerprint() -> anyhow::Result<String> {
    let crate_root = crate_root();
    let workspace_root = workspace_root();
    let repo_root = repo_root();
    let source_hash = hash_sources(
        "nav-kv-builder-v1",
        &[
            crate_root.join("src/product_build/nav_db.rs"),
            workspace_root.join("preprocessor-core/src/nav_kv.rs"),
            workspace_root.join("preprocessor-procedure-geometry/src/lib.rs"),
            repo_root.join("crates/procedure-geometry-types/src/lib.rs"),
        ],
    )?;
    let constants = serde_json::json!({
        "nav_db_contract_version": super::NAV_DB_CONTRACT_VERSION,
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

pub(super) fn terrain_discovery_builder_fingerprint() -> anyhow::Result<String> {
    let crate_root = crate_root();
    let workspace_root = workspace_root();
    hash_sources(
        "terrain-discovery-builder-v1",
        &[
            crate_root.join("src/product_build/static_products.rs"),
            workspace_root.join("preprocessor-fetch/src/lib.rs"),
        ],
    )
}
