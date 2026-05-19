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
        let file = OpenOptions::new()
            .create(true)
            .append(true)
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
