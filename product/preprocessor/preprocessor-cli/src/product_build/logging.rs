// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

pub(super) fn format_elapsed(elapsed_secs: u64) -> String {
    let hours = elapsed_secs / 3600;
    let minutes = (elapsed_secs % 3600) / 60;
    let seconds = elapsed_secs % 60;
    if hours > 0 {
        format!("+{}:{minutes:02}:{seconds:02}", hours)
    } else {
        format!("+{minutes}:{seconds:02}")
    }
}

pub(super) struct MasterLog {
    start: Instant,
    file: File,
}

impl MasterLog {
    pub(super) fn create(path: &Path) -> anyhow::Result<Self> {
        rotate_existing_log(path)?;
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        Ok(Self {
            start: Instant::now(),
            file,
        })
    }

    pub(super) fn log(&mut self, message: impl AsRef<str>) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        let line = format!(
            "{} {} {}",
            now,
            format_elapsed(self.start.elapsed().as_secs()),
            message.as_ref()
        );
        self.file
            .write_all(line.as_bytes())
            .and_then(|_| self.file.write_all(b"\n"))
            .context("failed to write master log")?;
        self.file.flush().context("failed to flush master log")?;
        Ok(())
    }
}

fn rotate_existing_log(path: &Path) -> anyhow::Result<()> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to stat {}", path.display()));
        }
    };
    if !metadata.is_file() || metadata.len() == 0 {
        return Ok(());
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("log");
    let extension = path.extension().and_then(|value| value.to_str());
    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let pid = std::process::id();

    for suffix in 0..1000 {
        let candidate_name = match (extension, suffix) {
            (Some(extension), 0) => format!("{stem}-{timestamp}-{pid}.{extension}"),
            (Some(extension), suffix) => format!("{stem}-{timestamp}-{pid}-{suffix}.{extension}"),
            (None, 0) => format!("{stem}-{timestamp}-{pid}"),
            (None, suffix) => format!("{stem}-{timestamp}-{pid}-{suffix}"),
        };
        let candidate = parent.join(candidate_name);
        if candidate.exists() {
            continue;
        }
        return fs::rename(path, &candidate).with_context(|| {
            format!(
                "failed to rotate {} to {}",
                path.display(),
                candidate.display()
            )
        });
    }

    bail!("failed to choose rotated log path for {}", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn master_log_create_rotates_existing_non_empty_log() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("master.log");
        fs::write(&path, "old log\n").expect("write old log");

        let mut log = MasterLog::create(&path).expect("create log");
        log.log("new log").expect("write new log");

        let current = fs::read_to_string(&path).expect("read current log");
        assert!(current.contains("new log"), "{current}");
        assert!(!current.contains("old log"), "{current}");

        let rotated = fs::read_dir(temp.path())
            .expect("read temp dir")
            .map(|entry| entry.expect("entry").path())
            .find(|entry| entry.file_name().and_then(|name| name.to_str()) != Some("master.log"))
            .expect("rotated log");
        assert_eq!(
            fs::read_to_string(rotated).expect("read rotated log"),
            "old log\n"
        );
    }
}
