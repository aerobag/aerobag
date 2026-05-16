use anyhow::{bail, Context};
use had_key::component as had_key_component;
use preprocessor_core::nav_kv::NavKvRoot;
use std::{
    collections::BTreeSet,
    env, fs,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
use zip::ZipArchive;

fn main() -> anyhow::Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 1 {
        bail!("usage: had-validate <had-dir-or-zip>");
    }
    let source = PathBuf::from(&args[0]);
    let had = HadSource::open(&source)?;
    validate_chart_catalog_packages(&had)?;
    println!("OK chart/catalog package references are present in package/by-id");
    Ok(())
}

fn validate_chart_catalog_packages(had: &HadSource) -> anyhow::Result<()> {
    let catalog: serde_json::Value = serde_json::from_slice(
        &had.query("chart/catalog")?
            .context("missing HAD key chart/catalog")?,
    )
    .context("failed to decode chart/catalog")?;
    let charts = catalog
        .as_array()
        .or_else(|| catalog.get("charts").and_then(|value| value.as_array()))
        .context("chart/catalog is neither an array nor an object with charts[]")?;

    let mut missing = Vec::new();
    let mut seen = BTreeSet::new();
    for chart in charts {
        let chart_id = chart
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or("<unknown>");
        let Some(map_view) = chart.get("map_view") else {
            continue;
        };
        let package_names = [
            map_view
                .get("package_name")
                .and_then(|value| value.as_str()),
            map_view
                .get("wide_angle")
                .and_then(|value| value.get("package_name"))
                .and_then(|value| value.as_str()),
        ];
        for package_name in package_names.into_iter().flatten() {
            if !seen.insert(package_name.to_string()) {
                continue;
            }
            let key = format!("package/by-id/{}", had_key_component(package_name));
            if had.query(&key)?.is_none() {
                missing.push((format!("{chart_id} uses {package_name}"), key));
            }
        }
    }

    if !missing.is_empty() {
        let details = missing
            .iter()
            .map(|(id, key)| format!("{id} -> {key}"))
            .collect::<Vec<_>>()
            .join(", ");
        bail!("chart/catalog references packages missing from package/by-id: {details}");
    }
    Ok(())
}

enum HadSource {
    Dir { dir: PathBuf, root: NavKvRoot },
    Zip { path: PathBuf, root: NavKvRoot },
}

impl HadSource {
    fn open(path: &Path) -> anyhow::Result<Self> {
        if path.is_dir() {
            let root_bytes = fs::read(path.join("root"))
                .with_context(|| format!("failed to read {}", path.join("root").display()))?;
            let root = NavKvRoot::parse(&root_bytes).map_err(anyhow::Error::msg)?;
            return Ok(Self::Dir {
                dir: path.to_path_buf(),
                root,
            });
        }

        let file =
            File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
        let mut archive = ZipArchive::new(file)
            .with_context(|| format!("failed to read zip {}", path.display()))?;
        let root_bytes = read_zip_member(&mut archive, "root")?;
        let root = NavKvRoot::parse(&root_bytes).map_err(anyhow::Error::msg)?;
        Ok(Self::Zip {
            path: path.to_path_buf(),
            root,
        })
    }

    fn query(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        match self {
            Self::Dir { dir, root } => Ok(root.extract_value(key, |page_index| {
                fs::read(dir.join(format!("page_{page_index:04}"))).ok()
            })),
            Self::Zip { path, root } => {
                let file = File::open(path)
                    .with_context(|| format!("failed to open {}", path.display()))?;
                let mut archive = ZipArchive::new(file)
                    .with_context(|| format!("failed to read zip {}", path.display()))?;
                Ok(root.extract_value(key, |page_index| {
                    read_zip_member(&mut archive, &format!("page_{page_index:04}")).ok()
                }))
            }
        }
    }
}

fn read_zip_member(archive: &mut ZipArchive<File>, name: &str) -> anyhow::Result<Vec<u8>> {
    let mut file = archive
        .by_name(name)
        .with_context(|| format!("missing zip member {name}"))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .with_context(|| format!("failed to read zip member {name}"))?;
    Ok(bytes)
}
