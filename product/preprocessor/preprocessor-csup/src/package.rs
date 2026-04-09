use std::{
    fs,
    path::Path,
};

use anyhow::{bail, Context};
use chrono::Utc;
use preprocessor_core::Region;
use preprocessor_fetch::{hash_file, write_package_outputs_jsonl, PackageOutputRecord};
use preprocessor_tools::{write_thumbnail_from_png, ToolInvocation};

use crate::{calculate_cycle, remove_if_exists};

pub fn package_csup_region(work_dir: &Path, region: Region) -> anyhow::Result<PackageOutputRecord> {
    let mut records = package_csup_region_records(work_dir, &[region], true)?;
    records
        .pop()
        .ok_or_else(|| anyhow::anyhow!("no csup package record generated for {}", region.code()))
}

pub fn package_csup_regions(work_dir: &Path, provenance_dir: &Path) -> anyhow::Result<usize> {
    let records = package_csup_region_records(work_dir, &Region::ALL, true)?;
    write_package_outputs_jsonl(provenance_dir, &records)?;
    Ok(Region::ALL.len())
}

fn package_csup_region_records(
    work_dir: &Path,
    regions: &[Region],
    produce_records: bool,
) -> anyhow::Result<Vec<PackageOutputRecord>> {
    let manifest_cycle = current_cycle_manifest();
    let mut package_records = Vec::with_capacity(regions.len());

    for region in regions {
        let manifest_name = format!("{}_CSUP", region.code());
        let zip_name = format!("{}_CSUP.zip", region.code());
        let manifest_path = work_dir.join(&manifest_name);
        let zip_path = work_dir.join(&zip_name);
        remove_if_exists(&manifest_path)?;
        remove_if_exists(&zip_path)?;

        let selected = collect_region_pngs(work_dir, region.code())?;
        let selected = with_thumbnail_members(work_dir, &selected)?;

        let mut manifest_text = String::new();
        manifest_text.push_str(&manifest_cycle);
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
        write_thumbnail_from_png(&source, &thumbnail_root, asset_path)?;
        all.push(
            Path::new("thumbnails")
                .join(asset_path)
                .to_string_lossy()
                .replace('\\', "/"),
        );
    }
    Ok(all)
}

fn current_cycle_manifest() -> String {
    let (manifest_cycle, _) = calculate_cycle(1, Utc::now());
    manifest_cycle.to_string()
}
