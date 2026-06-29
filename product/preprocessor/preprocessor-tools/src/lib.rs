use std::{
    fs,
    io::{Error as IoError, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::Instant,
};

use anyhow::Context;
use image::{Rgba, RgbaImage};
use preprocessor_core::CaptureEntry;

pub const TOOL_RUNNER_ARG: &str = "__aerobag-tool-runner";

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

pub fn sanitize_label(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
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

const TOOL_LOG_TAIL_BYTES: u64 = 4096;

pub fn command_output_diagnostic_summary(output: &Output) -> String {
    let mut parts = vec![exit_status_summary(output.status)];
    if !output.stdout.is_empty() {
        parts.push(format!(
            "stdout_tail=\"{}\"",
            escaped_byte_tail(&output.stdout, TOOL_LOG_TAIL_BYTES as usize)
        ));
    }
    if !output.stderr.is_empty() {
        parts.push(format!(
            "stderr_tail=\"{}\"",
            escaped_byte_tail(&output.stderr, TOOL_LOG_TAIL_BYTES as usize)
        ));
    }
    parts.join(" ")
}

fn exit_status_summary(status: std::process::ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("exit_code={code}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("signal={signal}");
        }
    }
    "signal".to_string()
}

fn escaped_byte_tail(bytes: &[u8], max_bytes: usize) -> String {
    let start = bytes.len().saturating_sub(max_bytes);
    escape_log_field(&String::from_utf8_lossy(&bytes[start..]))
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
        let exe = std::env::current_exe().context("failed to resolve current executable")?;
        let mut command = Command::new(exe);
        command.arg(TOOL_RUNNER_ARG).arg("--").arg(&self.program);
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

fn escape_log_field(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            other if other.is_control() => "?".chars().collect::<Vec<_>>(),
            other => vec![other],
        })
        .collect()
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
