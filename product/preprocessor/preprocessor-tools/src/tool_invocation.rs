// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fs,
    io::{Error as IoError, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};

use anyhow::Context;

use crate::diagnostics::{escape_log_field, TOOL_LOG_TAIL_BYTES};

pub const TOOL_RUNNER_ARG: &str = "__aerobag-tool-runner";
pub const TOOL_RUNNER_EXE_ENV: &str = "AEROBAG_TOOL_RUNNER_EXE";

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

impl ToolOutcome {
    pub fn diagnostic_summary(&self) -> String {
        let mut parts = vec![
            format!(
                "exit_code={}",
                self.exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "signal".to_string())
            ),
            format!("elapsed_ms={}", self.elapsed_ms),
            format!("stdout_log={}", self.logs.stdout.display()),
            format!("stderr_log={}", self.logs.stderr.display()),
        ];
        if let Some(tail) = escaped_log_tail(&self.logs.stdout) {
            parts.push(format!("stdout_tail=\"{tail}\""));
        }
        if let Some(tail) = escaped_log_tail(&self.logs.stderr) {
            parts.push(format!("stderr_tail=\"{tail}\""));
        }
        parts.join(" ")
    }
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
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create logs directory {}", parent.display()))?;
        }
        let image_magick_temp_dir = self.cwd.join(".tmp-imagemagick");
        let needs_image_magick_temp_dir =
            matches!(self.program.as_str(), "mogrify" | "convert" | "magick");
        if needs_image_magick_temp_dir {
            fs::create_dir_all(&image_magick_temp_dir).with_context(|| {
                format!(
                    "failed to create ImageMagick temp directory {}",
                    image_magick_temp_dir.display()
                )
            })?;
        }

        let mut command = self.runner_command()?;
        command.current_dir(&self.cwd);
        for (key, value) in &self.env {
            command.env(key, value);
        }
        if needs_image_magick_temp_dir {
            let temp_dir = image_magick_temp_dir.to_string_lossy().to_string();
            command.env("MAGICK_TEMPORARY_PATH", &temp_dir);
            command.env("TMPDIR", &temp_dir);
        }
        let stdout_file = fs::File::create(&logs.stdout)
            .with_context(|| format!("failed to create stdout log {}", logs.stdout.display()))?;
        let stderr_file = fs::File::create(&logs.stderr)
            .with_context(|| format!("failed to create stderr log {}", logs.stderr.display()))?;
        command
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file));
        if self.stdin_text.is_some() {
            command.stdin(Stdio::piped());
        }

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

        let status = child
            .wait()
            .with_context(|| format!("failed waiting for {}", self.render_command_line()))?;
        let elapsed_ms = start.elapsed().as_millis();

        Ok(ToolOutcome {
            success: status.success(),
            exit_code: status.code(),
            logs,
            elapsed_ms,
        })
    }

    pub fn ensure_success(&self, outcome: &ToolOutcome, context: &str) -> anyhow::Result<()> {
        if outcome.success {
            return Ok(());
        }
        anyhow::bail!(
            "{}; command=\"{}\" {}",
            context,
            escape_log_field(&self.render_command_line()),
            outcome.diagnostic_summary()
        );
    }

    fn runner_command(&self) -> anyhow::Result<Command> {
        let mut command = if let Some(exe) = std::env::var_os(TOOL_RUNNER_EXE_ENV) {
            let mut command = Command::new(exe);
            command.arg(TOOL_RUNNER_ARG).arg("--").arg(&self.program);
            command
        } else {
            Command::new(&self.program)
        };
        command.args(&self.args);
        Ok(command)
    }
}

fn escaped_log_tail(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len == 0 {
        return None;
    }
    let start = len.saturating_sub(TOOL_LOG_TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(escape_log_field(&String::from_utf8_lossy(&bytes)))
}

#[cfg(unix)]
pub fn run_tool_runner(args: &[String]) -> anyhow::Result<()> {
    use std::os::unix::process::CommandExt;

    if args.first().map(String::as_str) != Some("--") {
        anyhow::bail!("{TOOL_RUNNER_ARG} requires -- before tool command");
    }
    let program = args
        .get(1)
        .ok_or_else(|| anyhow::anyhow!("{TOOL_RUNNER_ARG} missing tool command"))?;
    let tool_args = &args[2..];

    configure_current_process_containment()?;
    let error = Command::new(program).args(tool_args).exec();
    Err(error)
        .with_context(|| format!("failed to exec {}", render_command_line(program, tool_args)))
}

#[cfg(unix)]
fn configure_current_process_containment() -> anyhow::Result<()> {
    unsafe {
        if libc::setpgid(0, 0) != 0 {
            return Err(IoError::last_os_error()).context("failed to set tool process group");
        }
        if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
            return Err(IoError::last_os_error()).context("failed to set tool parent-death signal");
        }
        if libc::getppid() == 1 {
            libc::raise(libc::SIGKILL);
        }
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn run_tool_runner(_args: &[String]) -> anyhow::Result<()> {
    anyhow::bail!("{TOOL_RUNNER_ARG} is only supported on Unix")
}

fn render_command_line(program: &str, args: &[String]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(program.to_string());
    parts.extend(args.iter().cloned());
    parts.join(" ")
}
