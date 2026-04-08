use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Instant,
};

use anyhow::Context;
use preprocessor_core::CaptureEntry;

pub fn comparison_targets(entry: &CaptureEntry) -> Vec<&'static str> {
    let mut targets = vec!["zip_members", "package_hashes"];
    if entry.tile_paths.is_some() {
        targets.push("tile_paths");
    }
    if entry.source_urls.is_some() {
        targets.push("source_urls");
    }
    targets
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolInvocation {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub label: String,
    pub env: Vec<(String, String)>,
    pub stdin_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolLogPaths {
    pub stdout: PathBuf,
    pub stderr: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutcome {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub logs: ToolLogPaths,
    pub elapsed_ms: u128,
}

impl ToolInvocation {
    pub fn render_command_line(&self) -> String {
        let mut parts = Vec::with_capacity(self.args.len() + 1);
        parts.push(self.program.clone());
        parts.extend(self.args.iter().cloned());
        parts.join(" ")
    }

    pub fn log_paths(&self, logs_dir: impl AsRef<Path>) -> ToolLogPaths {
        let logs_dir = logs_dir.as_ref();
        ToolLogPaths {
            stdout: logs_dir.join(format!("{}.stdout.log", self.label)),
            stderr: logs_dir.join(format!("{}.stderr.log", self.label)),
        }
    }

    pub fn run_logged(&self, logs_dir: impl AsRef<Path>) -> anyhow::Result<ToolOutcome> {
        let logs = self.log_paths(logs_dir);
        if let Some(parent) = logs.stdout.parent() {
            fs::create_dir_all(parent).context("failed to create logs directory")?;
        }

        let mut command = Command::new(&self.program);
        command.args(&self.args).current_dir(&self.cwd);
        for (key, value) in &self.env {
            command.env(key, value);
        }
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        if self.stdin_text.is_some() {
            command.stdin(Stdio::piped());
        }

        let mut stdout_file =
            fs::File::create(&logs.stdout).context("failed to create stdout log")?;
        let mut stderr_file =
            fs::File::create(&logs.stderr).context("failed to create stderr log")?;

        let start = Instant::now();
        let mut child = command
            .spawn()
            .with_context(|| format!("failed to run {}", self.render_command_line()))?;
        if let Some(stdin_text) = &self.stdin_text {
            let mut stdin = child
                .stdin
                .take()
                .context("failed to capture child stdin")?;
            stdin
                .write_all(stdin_text.as_bytes())
                .context("failed to write child stdin")?;
        }
        let stdout = child
            .stdout
            .take()
            .context("failed to capture child stdout")?;
        let stderr = child
            .stderr
            .take()
            .context("failed to capture child stderr")?;

        let stdout_handle = thread::spawn(move || -> std::io::Result<()> {
            let mut reader = BufReader::new(stdout);
            let mut line = Vec::new();
            loop {
                line.clear();
                let bytes = reader.read_until(b'\n', &mut line)?;
                if bytes == 0 {
                    break;
                }
                stdout_file.write_all(&line)?;
                stdout_file.flush()?;
            }
            Ok(())
        });

        let stderr_handle = thread::spawn(move || -> std::io::Result<()> {
            let mut reader = BufReader::new(stderr);
            let mut line = Vec::new();
            loop {
                line.clear();
                let bytes = reader.read_until(b'\n', &mut line)?;
                if bytes == 0 {
                    break;
                }
                stderr_file.write_all(&line)?;
                stderr_file.flush()?;
            }
            Ok(())
        });

        let status = child
            .wait()
            .with_context(|| format!("failed waiting for {}", self.render_command_line()))?;
        let elapsed_ms = start.elapsed().as_millis();
        stdout_handle
            .join()
            .map_err(|_| anyhow::anyhow!("stdout thread panicked"))?
            .context("failed to stream stdout log")?;
        stderr_handle
            .join()
            .map_err(|_| anyhow::anyhow!("stderr thread panicked"))?
            .context("failed to stream stderr log")?;

        Ok(ToolOutcome {
            success: status.success(),
            exit_code: status.code(),
            logs,
            elapsed_ms,
        })
    }
}
