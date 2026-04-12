use std::{
    fs,
    io::{BufRead, BufReader, Error as IoError, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Instant,
};

use anyhow::Context;
use image::{Rgba, RgbaImage};
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
        configure_subprocess_containment(&mut command);

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

#[cfg(unix)]
fn configure_subprocess_containment(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) != 0 {
                return Err(IoError::last_os_error());
            }
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(IoError::last_os_error());
            }
            if libc::getppid() == 1 {
                libc::raise(libc::SIGKILL);
            }
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn configure_subprocess_containment(_command: &mut Command) {}

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
    if !outcome.success {
        anyhow::bail!("convert failed while concatenating PNGs into {}", output.display());
    }
    Ok(())
}

pub fn flatten_png_onto_white(path: &Path) -> anyhow::Result<()> {
    let image = image::open(path)
        .with_context(|| format!("failed to open PNG for white flatten {}", path.display()))?;
    let mut canvas = RgbaImage::from_pixel(image.width(), image.height(), Rgba([255, 255, 255, 255]));
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
