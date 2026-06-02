use super::*;

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
    let artifact_root = normalize_absolute_path(build_root);
    let normalized_path = normalize_absolute_path(path);
    normalized_path
        .strip_prefix(&artifact_root)
        .map(|value| value.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

pub(super) fn publish_path_key(path: &Path, build_root: &Path) -> String {
    let relative = relative_artifact_path(path, build_root);
    let key = relative
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    key.trim_matches('_').to_string()
}

pub(super) fn artifact_root_from_publish_dir(publish_dir: &Path) -> anyhow::Result<PathBuf> {
    let timestamp_dir = publish_dir;
    let label_dir = timestamp_dir.parent().ok_or_else(|| {
        anyhow::anyhow!("publish_dir has no label parent: {}", publish_dir.display())
    })?;
    let published_dir = label_dir.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "publish_dir has no published parent: {}",
            publish_dir.display()
        )
    })?;
    if published_dir.file_name().and_then(|name| name.to_str()) != Some("published") {
        bail!(
            "publish_dir must be under <build_root>/published/<label>/<timestamp>, got {}",
            publish_dir.display()
        );
    }
    published_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "published dir has no build_root parent: {}",
                published_dir.display()
            )
        })
}

pub(super) fn build_node_root(config: &ProductBuildConfig, name: &str) -> anyhow::Result<PathBuf> {
    let root = config
        .build_root
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
    let root = config.build_root.join("cache").join("nodes").join(name);
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
