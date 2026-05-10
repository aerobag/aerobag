use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Context;
use zip::{write::SimpleFileOptions, CompressionMethod, DateTime as ZipDateTime, ZipWriter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipSource {
    pub member_name: String,
    pub source_path: PathBuf,
    pub compression: ZipCompression,
}

impl ZipSource {
    pub fn new(member_name: impl Into<String>, source_path: impl Into<PathBuf>) -> Self {
        Self {
            member_name: member_name.into(),
            source_path: source_path.into(),
            compression: ZipCompression::Deflated,
        }
    }

    pub fn stored(mut self) -> Self {
        self.compression = ZipCompression::Stored;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZipCompression {
    Deflated,
    Stored,
}

impl ZipCompression {
    fn method(self) -> CompressionMethod {
        match self {
            Self::Deflated => CompressionMethod::Deflated,
            Self::Stored => CompressionMethod::Stored,
        }
    }
}

pub fn write_deterministic_zip(path: &Path, members: &[ZipSource]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    let file =
        fs::File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    let mut writer = ZipWriter::new(file);
    let mut sorted_members = members.to_vec();
    sorted_members.sort_by(|left, right| left.member_name.cmp(&right.member_name));
    for member in sorted_members {
        let options = SimpleFileOptions::default()
            .compression_method(member.compression.method())
            .last_modified_time(ZipDateTime::default());
        writer
            .start_file(&member.member_name, options)
            .with_context(|| {
                format!("failed to add {} to {}", member.member_name, path.display())
            })?;
        let bytes = fs::read(&member.source_path)
            .with_context(|| format!("failed to read {}", member.source_path.display()))?;
        writer.write_all(&bytes).with_context(|| {
            format!(
                "failed to write {} to {}",
                member.member_name,
                path.display()
            )
        })?;
    }
    writer
        .finish()
        .with_context(|| format!("failed to finish {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use zip::ZipArchive;

    #[test]
    fn deterministic_zip_sorts_members_and_uses_fixed_timestamps() -> anyhow::Result<()> {
        let root =
            std::env::temp_dir().join(format!("preprocessor-zip-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root)?;
        let a = root.join("a.txt");
        let b = root.join("b.txt");
        fs::write(&a, b"a")?;
        fs::write(&b, b"b")?;
        let zip_path = root.join("out.zip");
        write_deterministic_zip(
            &zip_path,
            &[ZipSource::new("b.txt", &b), ZipSource::new("a.txt", &a)],
        )?;

        let file = fs::File::open(&zip_path)?;
        let mut archive = ZipArchive::new(file)?;
        assert_eq!(archive.by_index(0)?.name(), "a.txt");
        assert_eq!(archive.by_index(1)?.name(), "b.txt");
        let mut text = String::new();
        archive.by_name("a.txt")?.read_to_string(&mut text)?;
        assert_eq!(text, "a");
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }
}
