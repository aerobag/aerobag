use std::path::{Path, PathBuf};

use preprocessor_tools::write_thumbnail_from_png;

use crate::package::TppThumbnailPlan;

pub fn write_tpp_thumbnail(
    asset_root: &Path,
    output_root: &Path,
    thumbnail: &TppThumbnailPlan,
) -> anyhow::Result<PathBuf> {
    write_tpp_thumbnail_from_source(
        &asset_root.join(&thumbnail.asset_path),
        output_root,
        thumbnail,
    )
}

pub fn write_tpp_thumbnail_from_source(
    source_png: &Path,
    output_root: &Path,
    thumbnail: &TppThumbnailPlan,
) -> anyhow::Result<PathBuf> {
    write_thumbnail_from_png(
        source_png,
        &output_root.join("thumbnails"),
        Path::new(&thumbnail.asset_path),
    )?;
    Ok(output_root.join(&thumbnail.thumbnail_path))
}
