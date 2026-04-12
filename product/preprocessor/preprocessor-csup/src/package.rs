use std::{
    fs,
    path::Path,
};

use anyhow::{bail, Context};
use chrono::Utc;
use preprocessor_core::{PackageAssetManifest, PackageAssetRecord, Region, PACKAGE_ASSET_MANIFEST_NAME};
use preprocessor_fetch::{hash_file, write_package_outputs_jsonl, PackageOutputRecord};
use preprocessor_tools::{write_thumbnail_from_png, ToolInvocation};

use crate::{calculate_cycle, remove_if_exists};

pub fn package_csup_region(work_dir: &Path, region: Region) -> anyhow::Result<PackageOutputRecord> {
    let manifest_cycle = current_cycle_manifest();
    package_csup_region_versioned(work_dir, region, &manifest_cycle, &manifest_cycle)
}

pub fn package_csup_region_versioned(
    work_dir: &Path,
    region: Region,
    manifest_version: &str,
    artifact_version: &str,
) -> anyhow::Result<PackageOutputRecord> {
    let mut records =
        package_csup_region_records(work_dir, &[region], true, manifest_version, artifact_version)?;
    records
        .pop()
        .ok_or_else(|| anyhow::anyhow!("no csup package record generated for {}", region.code()))
}

pub fn package_csup_regions(work_dir: &Path, provenance_dir: &Path) -> anyhow::Result<usize> {
    let manifest_cycle = current_cycle_manifest();
    let records = package_csup_region_records(
        work_dir,
        &Region::ALL,
        true,
        &manifest_cycle,
        &manifest_cycle,
    )?;
    write_package_outputs_jsonl(provenance_dir, &records)?;
    Ok(Region::ALL.len())
}

fn package_csup_region_records(
    work_dir: &Path,
    regions: &[Region],
    produce_records: bool,
    manifest_version: &str,
    artifact_version: &str,
) -> anyhow::Result<Vec<PackageOutputRecord>> {
    let mut package_records = Vec::with_capacity(regions.len());

    for region in regions {
        let manifest_name = format!("{}_CSUP_{}", region.code(), artifact_version);
        let zip_name = format!("{}_CSUP_{}.zip", region.code(), artifact_version);
        let manifest_path = work_dir.join(&manifest_name);
        let zip_path = work_dir.join(&zip_name);
        remove_if_exists(&manifest_path)?;
        remove_if_exists(&zip_path)?;

        let selected = collect_region_pngs(work_dir, region.code())?;
        let package_assets_path = work_dir.join(PACKAGE_ASSET_MANIFEST_NAME);
        remove_if_exists(&package_assets_path)?;
        let selected = with_thumbnail_members(work_dir, &selected)?;
        write_package_asset_manifest(
            &package_assets_path,
            &manifest_name,
            &selected,
        )?;

        let mut manifest_text = String::new();
        manifest_text.push_str(manifest_version);
        manifest_text.push('\n');
        for path in &selected {
            manifest_text.push_str(path);
            manifest_text.push('\n');
        }
        fs::write(&manifest_path, manifest_text)
            .with_context(|| format!("failed to write {}", manifest_path.display()))?;

        let mut stdin_text = String::new();
        for path in &selected {
            stdin_text.push_str(path);
            stdin_text.push('\n');
        }
        stdin_text.push_str(PACKAGE_ASSET_MANIFEST_NAME);
        stdin_text.push('\n');
        stdin_text.push_str(&manifest_name);
        stdin_text.push('\n');

        let invocation = ToolInvocation {
            program: "zip".to_string(),
            args: vec!["-q".to_string(), zip_name.clone(), "-@".to_string()],
            cwd: work_dir.to_path_buf(),
            label: format!("csup-package-{}", region.code()),
            env: Vec::new(),
            stdin_text: Some(stdin_text),
        };
        let outcome = invocation.run_logged(work_dir.join(".rust-logs"))?;
        if !outcome.success {
            bail!("zip failed for region {}", region.code());
        }

        if produce_records {
            package_records.push(PackageOutputRecord {
                label: "csup".to_string(),
                chart: None,
                region: region.code().to_string(),
                manifest: manifest_name,
                manifest_sha256: hash_file(&manifest_path)?,
                zip: zip_name,
                zip_sha256: hash_file(&zip_path)?,
            });
        }
    }

    Ok(package_records)
}

fn collect_region_pngs(work_dir: &Path, region_code: &str) -> anyhow::Result<Vec<String>> {
    fn visit(
        dir: &Path,
        root: &Path,
        region_code: &str,
        out: &mut Vec<String>,
    ) -> anyhow::Result<()> {
        for entry in
            fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .with_context(|| format!("failed to stat {}", path.display()))?;
            if file_type.is_dir() {
                visit(&path, root, region_code, out)?;
            } else if file_type.is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(&format!("CSUP-{region_code}_")) && name.ends_with(".png") {
                    let relative = path
                        .strip_prefix(root)
                        .with_context(|| format!("failed to relativize {}", path.display()))?;
                    out.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        Ok(())
    }

    let afd_dir = work_dir.join("afd");
    if !afd_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    visit(&afd_dir, work_dir, region_code, &mut paths)?;
    Ok(paths)
}

fn with_thumbnail_members(work_dir: &Path, members: &[String]) -> anyhow::Result<Vec<String>> {
    let mut all = Vec::with_capacity(members.len() * 2);
    let thumbnail_root = work_dir.join("thumbnails");
    for member in members {
        all.push(member.clone());
        let asset_path = Path::new(member);
        let source = work_dir.join(asset_path);
        let thumbnail_path = Path::new("thumbnails")
            .join(asset_path)
            .to_string_lossy()
            .replace('\\', "/");
        if !work_dir.join(&thumbnail_path).is_file() {
            write_thumbnail_from_png(&source, &thumbnail_root, asset_path)?;
        }
        all.push(thumbnail_path);
    }
    Ok(all)
}

fn write_package_asset_manifest(
    output_path: &Path,
    package_id: &str,
    members: &[String],
) -> anyhow::Result<()> {
    let assets = members
        .iter()
        .filter(|member| member.ends_with(".png") && !member.starts_with("thumbnails/"))
        .map(|member| {
            let asset_path = Path::new(member);
            let airport_id = asset_path
                .components()
                .nth(1)
                .and_then(|value| value.as_os_str().to_str())
                .unwrap_or_default()
                .to_string();
            let filename = asset_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string();
            PackageAssetRecord {
                id: format!("csup:{airport_id}:{filename}"),
                airport_id,
                label: asset_path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_string(),
                asset_kind: "png".to_string(),
                document_type: "csup".to_string(),
                asset_path: member.clone(),
                thumbnail_path: Path::new("thumbnails")
                    .join(asset_path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            }
        })
        .collect::<Vec<_>>();
    let manifest = PackageAssetManifest {
        schema_version: 1,
        family_id: "csup".to_string(),
        package_id: package_id.to_string(),
        assets,
    };
    fs::write(
        output_path,
        serde_json::to_vec_pretty(&manifest).context("failed to encode csup package asset manifest")?,
    )
    .with_context(|| format!("failed to write {}", output_path.display()))
}

fn current_cycle_manifest() -> String {
    let (manifest_cycle, _) = calculate_cycle(1, Utc::now());
    manifest_cycle.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use zip::ZipArchive;

    const ONE_BY_ONE_PNG: &[u8] = &[
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H',
        b'D', b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
        0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, b'I', b'D', b'A', b'T', 0x78,
        0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0, 0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99,
        0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn csup_package_includes_package_asset_manifest() {
        let temp = tempdir().unwrap();
        let work_dir = temp.path();
        let airport_dir = work_dir.join("afd/AK84");
        fs::create_dir_all(&airport_dir).unwrap();
        fs::write(airport_dir.join("CSUP-AK_0.png"), ONE_BY_ONE_PNG).unwrap();

        let record = package_csup_region_versioned(work_dir, Region::Ak, "2603", "2603").unwrap();
        let zip_path = work_dir.join(record.zip);
        let file = fs::File::open(zip_path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        let manifest: PackageAssetManifest =
            serde_json::from_reader(archive.by_name(PACKAGE_ASSET_MANIFEST_NAME).unwrap()).unwrap();

        assert_eq!(manifest.family_id, "csup");
        assert_eq!(manifest.package_id, "AK_CSUP_2603");
        assert_eq!(manifest.assets.len(), 1);
        assert_eq!(manifest.assets[0].asset_path, "afd/AK84/CSUP-AK_0.png");
        assert_eq!(
            manifest.assets[0].thumbnail_path,
            "thumbnails/afd/AK84/CSUP-AK_0.png"
        );
    }
}
