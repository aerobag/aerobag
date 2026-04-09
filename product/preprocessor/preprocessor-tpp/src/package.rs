use std::{
    fs,
    path::Path,
    process::Command,
};

use anyhow::{bail, Context};
use chrono::Utc;
use preprocessor_core::Region;
use preprocessor_fetch::{
    hash_file, write_package_outputs_jsonl, PackageOutputRecord,
};
use preprocessor_tools::{write_thumbnail_from_png, ToolInvocation};

use crate::calculate_cycle;

pub(crate) fn package_region(
    work_dir: &Path,
    provenance_dir: &Path,
    region: Region,
) -> anyhow::Result<usize> {
    let manifest_name = format!("{}_TPP", region.code());
    let zip_name = format!("{}_TPP.zip", region.code());
    let manifest_path = work_dir.join(&manifest_name);
    let zip_path = work_dir.join(&zip_name);
    remove_if_exists(&manifest_path)?;
    remove_if_exists(&zip_path)?;

    let selected = collect_region_pngs(work_dir, region)?;
    let selected = with_thumbnail_members(work_dir, &selected)?;
    let mut manifest_text = String::new();
    manifest_text.push_str(&current_cycle_manifest());
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
        label: format!("tpp-package-{}", region.code()),
        env: Vec::new(),
        stdin_text: Some(stdin_text),
    };
    let outcome = invocation.run_logged(work_dir.join(".rust-logs"))?;
    if !outcome.success {
        bail!("zip failed for region {}", region.code());
    }

    write_package_outputs_jsonl(
        provenance_dir,
        &[PackageOutputRecord {
            label: format!("tpp-{}", region.code().to_ascii_lowercase()),
            chart: None,
            region: region.code().to_string(),
            manifest: manifest_name,
            manifest_sha256: hash_file(&manifest_path)?,
            zip: zip_name,
            zip_sha256: hash_file(&zip_path)?,
        }],
    )?;

    Ok(1)
}

fn collect_region_pngs(work_dir: &Path, region: Region) -> anyhow::Result<Vec<String>> {
    let script = r#"import glob, sys
from pathlib import Path
root = Path(sys.argv[1])
seen = set()
for state in sys.argv[2:]:
    pattern = root / f"plates/**/*-{state}-*.png"
    for path in glob.glob(str(pattern), recursive=True):
        relative = Path(path).relative_to(root).as_posix()
        if relative not in seen:
            seen.add(relative)
            print(relative)
"#;
    let mut command = Command::new("python3");
    command.arg("-c").arg(script).arg(work_dir);
    for state in region.state_codes() {
        command.arg(state);
    }
    let output = command
        .output()
        .with_context(|| format!("failed to enumerate plates under {}", work_dir.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("python plate enumeration failed: {stderr}");
    }

    let stdout = String::from_utf8(output.stdout).context("plate enumeration was not utf-8")?;
    Ok(stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
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

fn remove_if_exists(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}
