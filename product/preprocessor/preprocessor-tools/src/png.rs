// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context;
use image::{Rgba, RgbaImage};

use crate::ToolInvocation;

pub fn append_pngs_vertical(
    work_dir: &Path,
    logs_dir: &Path,
    inputs: &[PathBuf],
    output: &Path,
    label: &str,
) -> anyhow::Result<()> {
    if inputs.is_empty() {
        return Ok(());
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).context("failed to create output directory")?;
    }
    let mut args = inputs
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    args.extend([
        "-background".to_string(),
        "none".to_string(),
        "-gravity".to_string(),
        "north".to_string(),
        "-append".to_string(),
        format!("PNG32:{}", output.to_string_lossy()),
    ]);
    let invocation = ToolInvocation {
        program: "convert".to_string(),
        args,
        cwd: work_dir.to_path_buf(),
        label: label.to_string(),
        env: Vec::new(),
        stdin_text: None,
    };
    let outcome = invocation.run_logged(logs_dir)?;
    invocation.ensure_success(
        &outcome,
        &format!(
            "convert failed while concatenating PNGs into {}",
            output.display()
        ),
    )?;
    Ok(())
}

pub fn flatten_png_onto_white(path: &Path) -> anyhow::Result<()> {
    let image = image::open(path)
        .with_context(|| format!("failed to open PNG for white flatten {}", path.display()))?;
    let mut canvas =
        RgbaImage::from_pixel(image.width(), image.height(), Rgba([255, 255, 255, 255]));
    let rgba = image.to_rgba8();
    image::imageops::overlay(&mut canvas, &rgba, 0, 0);
    canvas
        .save(path)
        .with_context(|| format!("failed to rewrite flattened PNG {}", path.display()))?;
    Ok(())
}

pub fn write_thumbnail_from_png(
    source: &Path,
    thumbnail_root: &Path,
    asset_path: &Path,
) -> anyhow::Result<String> {
    let thumbnail_path = thumbnail_root.join(asset_path);
    if let Some(parent) = thumbnail_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let image = image::open(source)
        .with_context(|| format!("failed to open thumbnail source {}", source.display()))?;
    let resized = image.thumbnail(100, 150).to_rgba8();
    let (width, height) = resized.dimensions();
    let x = i64::from((100 - width) / 2);
    let y = i64::from((150 - height) / 2);
    let mut canvas = RgbaImage::from_pixel(100, 150, Rgba([0, 0, 0, 0]));
    image::imageops::overlay(&mut canvas, &resized, x, y);
    canvas
        .save(&thumbnail_path)
        .with_context(|| format!("failed to write thumbnail {}", thumbnail_path.display()))?;
    Ok(thumbnail_path.display().to_string())
}
