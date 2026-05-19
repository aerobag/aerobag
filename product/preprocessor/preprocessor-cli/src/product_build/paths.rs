use super::*;

pub(super) fn artifact_root_from_build_root(build_root: &Path) -> &Path {
    if build_root
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "published_packaged" || name == "published_packaged_validation")
    {
        return build_root.parent().unwrap_or(build_root);
    }
    if build_root
        .parent()
        .and_then(|value| value.file_name())
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "published_packaged" || name == "published_packaged_validation")
    {
        return build_root
            .parent()
            .and_then(|value| value.parent())
            .unwrap_or(build_root);
    }
    build_root.parent().unwrap_or(build_root)
}

pub(super) fn normalize_absolute_path(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

pub(super) fn relative_artifact_path(path: &Path, build_root: &Path) -> String {
    let artifact_root = normalize_absolute_path(artifact_root_from_build_root(build_root));
    let normalized_path = normalize_absolute_path(path);
    normalized_path
        .strip_prefix(&artifact_root)
        .map(|value| value.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

pub(super) fn relative_product_build_path(path: &Path) -> String {
    let artifact_root = artifact_root_from_build_root(path);
    path.strip_prefix(artifact_root)
        .map(|value| value.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

pub(super) fn build_node_root(config: &ProductBuildConfig, name: &str) -> anyhow::Result<PathBuf> {
    let root = artifact_root_from_build_root(&config.build_root)
        .join("private-work")
        .join("publish-nodes")
        .join(config.profile.as_str())
        .join(name);
    fs::create_dir_all(&root).with_context(|| format!("failed to create {}", root.display()))?;
    Ok(root)
}

pub(super) fn build_shared_node_dir(
    config: &ProductBuildConfig,
    name: &str,
) -> anyhow::Result<PathBuf> {
    let root = artifact_root_from_build_root(&config.build_root)
        .join("cache")
        .join("nodes")
        .join(name);
    fs::create_dir_all(&root).with_context(|| format!("failed to create {}", root.display()))?;
    Ok(root)
}

pub(super) fn preprocessor_workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("preprocessor-cli should live under workspace root")
        .to_path_buf()
}

pub(super) fn repo_root_from_preprocessor_workspace() -> PathBuf {
    preprocessor_workspace_root()
        .parent()
        .expect("preprocessor workspace should live under product/")
        .parent()
        .expect("product should live under repo root")
        .to_path_buf()
}

pub(super) fn vectors_code_fingerprint() -> anyhow::Result<String> {
    let workspace_root = preprocessor_workspace_root();
    let repo_root = repo_root_from_preprocessor_workspace();
    let inputs = serde_json::json!({
        "preprocessor_vectors": hash_tree(&workspace_root.join("preprocessor-vectors"))?,
        "airspace_geometry": hash_tree(&repo_root.join("crates/airspace-geometry"))?,
    });
    Ok(hash_text(
        &serde_json::to_string(&inputs).context("vectors code fingerprint json")?,
    ))
}
