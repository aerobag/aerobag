use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{bail, Context};
use preprocessor_core::nav_kv::NavKvRoot;
use serde::Deserialize;

use crate::engine::CycleDataProvider;

#[derive(Debug, Clone)]
pub struct PublishedCycleDataProvider {
    publication_root: PathBuf,
}

impl PublishedCycleDataProvider {
    pub fn new(publication_root: PathBuf) -> Self {
        Self { publication_root }
    }

    pub fn publication_root(&self) -> &Path {
        &self.publication_root
    }

    pub fn current_nav_db_dir(&self) -> anyhow::Result<PathBuf> {
        let current_path = self.publication_root.join("current_artifacts.json");
        let current: CurrentArtifactsManifest = read_json(&current_path)?;
        let unpacked_root = self
            .publication_root
            .join(public_relative_path(&current.artifact_roots.unpacked)?);
        let cycle_bundle = current
            .bundles
            .iter()
            .find(|bundle| bundle.bundle_type.as_deref() == Some("cycle"))
            .or_else(|| current.bundles.first())
            .context("current_artifacts.json contains no bundles")?;
        let bundle_path = unpacked_root.join(public_relative_path(bundle_ref_path(cycle_bundle))?);
        let bundle: BundleManifest = read_json(&bundle_path)?;
        let nav_db = bundle
            .packages
            .iter()
            .find(|package| package.family_id == "nav-db")
            .context("cycle bundle contains no nav-db package")?;
        let nav_db_relative_path = public_relative_path(&nav_db.relative_path)?;
        let nav_db_dir_name = zip_stem(&nav_db_relative_path)?;
        let nav_db_dir = unpacked_root.join(nav_db_dir_name);
        if !nav_db_dir.is_dir() {
            bail!(
                "published nav-db directory does not exist: {}",
                nav_db_dir.display()
            );
        }
        Ok(nav_db_dir)
    }
}

impl CycleDataProvider for PublishedCycleDataProvider {
    fn current_towered_metar_station_ids(&self) -> anyhow::Result<BTreeSet<String>> {
        load_towered_station_ids_from_nav_db_dir(&self.current_nav_db_dir()?)
    }
}

pub fn load_towered_station_ids_from_nav_db_dir(
    nav_db_dir: &Path,
) -> anyhow::Result<BTreeSet<String>> {
    let root_path = nav_db_dir.join("root");
    let root_bytes =
        fs::read(&root_path).with_context(|| format!("failed to read {}", root_path.display()))?;
    let root = NavKvRoot::parse(&root_bytes).map_err(anyhow::Error::msg)?;
    let prefix = "navref/symbol/airport/";
    let keys = root
        .prefix_keys(prefix, |page| read_nav_kv_page(nav_db_dir, page))
        .with_context(|| format!("failed to scan nav-db prefix {prefix}"))?;
    let mut station_ids = BTreeSet::new();
    for key in keys {
        let Some(value_bytes) = root.extract_value(&key, |page| read_nav_kv_page(nav_db_dir, page))
        else {
            continue;
        };
        let value: serde_json::Value = serde_json::from_slice(&value_bytes)
            .with_context(|| format!("failed to parse nav-db value {key}"))?;
        if value
            .get("towered")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            let station_id = key
                .strip_prefix(prefix)
                .context("airport symbol key missing expected prefix")?
                .trim()
                .to_ascii_uppercase();
            if !station_id.is_empty() {
                station_ids.insert(station_id);
            }
        }
    }
    if station_ids.is_empty() {
        bail!(
            "published nav-db {} yielded no towered airport station ids",
            nav_db_dir.display()
        );
    }
    Ok(station_ids)
}

fn read_nav_kv_page(nav_db_dir: &Path, page: u32) -> Option<Vec<u8>> {
    fs::read(nav_db_dir.join(format!("page_{page:04}"))).ok()
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> anyhow::Result<T> {
    serde_json::from_slice(
        &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))
}

fn bundle_ref_path(bundle: &CurrentBundleRef) -> &str {
    if bundle.relative_path.is_empty() {
        &bundle.filename
    } else {
        &bundle.relative_path
    }
}

fn public_relative_path(value: &str) -> anyhow::Result<PathBuf> {
    let path = PathBuf::from(value);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("publication path is not a safe relative path: {value}");
    }
    Ok(path)
}

fn zip_stem(path: &Path) -> anyhow::Result<PathBuf> {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        bail!("nav-db package path has no filename: {}", path.display());
    };
    let Some(stem) = file_name.strip_suffix(".zip") else {
        bail!("nav-db package path is not a zip: {}", path.display());
    };
    Ok(path.with_file_name(stem))
}

#[derive(Debug, Deserialize)]
struct CurrentArtifactsManifest {
    artifact_roots: ArtifactRoots,
    bundles: Vec<CurrentBundleRef>,
}

#[derive(Debug, Deserialize)]
struct ArtifactRoots {
    unpacked: String,
}

#[derive(Debug, Deserialize)]
struct CurrentBundleRef {
    filename: String,
    #[serde(default)]
    relative_path: String,
    #[serde(default)]
    bundle_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BundleManifest {
    packages: Vec<BundlePackage>,
}

#[derive(Debug, Deserialize)]
struct BundlePackage {
    family_id: String,
    relative_path: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use preprocessor_core::nav_kv::{build_nav_kv_sorted, NavKvPair};
    use tempfile::tempdir;

    #[test]
    fn published_provider_loads_towered_airports_from_nav_db() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let publication_root = temp.path();
        let unpacked = publication_root.join("published_unpacked");
        let nav_db_dir = unpacked.join("nav_db_test_hash");
        fs::create_dir_all(&nav_db_dir)?;
        let built = build_nav_kv_sorted(
            vec![
                pair("chart/catalog", "{}"),
                pair(
                    "navref/symbol/airport/KSEA",
                    r#"{"label":"SEA","towered":true}"#,
                ),
                pair(
                    "navref/symbol/airport/S43",
                    r#"{"label":"S43","towered":false}"#,
                ),
            ],
            256,
        )
        .map_err(anyhow::Error::msg)?;
        fs::write(nav_db_dir.join("root"), built.root_bytes)?;
        for (index, page) in built.pages.iter().enumerate() {
            fs::write(nav_db_dir.join(format!("page_{index:04}")), page)?;
        }
        fs::write(
            publication_root.join("current_artifacts.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "artifact_roots": {"unpacked": "published_unpacked"},
                "bundles": [{
                    "filename": "bundle_cycle_test.json",
                    "relative_path": "bundle_cycle_test.json",
                    "bundle_type": "cycle"
                }]
            }))?,
        )?;
        fs::write(
            unpacked.join("bundle_cycle_test.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "packages": [{
                    "family_id": "nav-db",
                    "relative_path": "nav_db_test_hash.zip"
                }]
            }))?,
        )?;

        let provider = PublishedCycleDataProvider::new(publication_root.to_path_buf());

        assert_eq!(
            provider.current_towered_metar_station_ids()?,
            BTreeSet::from(["KSEA".to_string()])
        );
        Ok(())
    }

    #[test]
    fn publication_paths_must_be_relative_and_simple() {
        assert!(public_relative_path("published_unpacked").is_ok());
        assert!(public_relative_path("../published_unpacked").is_err());
        assert!(public_relative_path("/tmp/published_unpacked").is_err());
        assert!(public_relative_path("").is_err());
    }

    fn pair(key: &str, value: &str) -> NavKvPair {
        NavKvPair {
            key: key.to_string(),
            value: value.as_bytes().to_vec(),
        }
    }
}
