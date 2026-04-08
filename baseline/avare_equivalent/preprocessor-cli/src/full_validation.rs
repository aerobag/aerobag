use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Instant,
};

use anyhow::{Context, bail};
use chrono::Utc;

#[derive(Debug, Clone)]
pub struct FullValidationConfig {
    pub repo_root: PathBuf,
    pub run_id: String,
    pub validation_root: PathBuf,
    pub avare_source_root: PathBuf,
    pub cache_root: PathBuf,
    pub fetch_cache_root: PathBuf,
    pub fetch_cache_mode: String,
    pub fetch_jobs: usize,
    pub zip_jobs: usize,
    pub cpu_jobs: usize,
    pub native_chart_cpu_jobs: usize,
    pub max_heavy_jobs: usize,
    pub image_sample_percent: u8,
    pub image_rmse_threshold: String,
}

const CGROUP_ACTIVE_ENV: &str = "FULL_VALIDATION_CGROUP_ACTIVE";
const DEFAULT_MEMORY_MAX: &str = "35G";

#[derive(Debug)]
struct SpawnedJob {
    name: String,
    child: Child,
}

#[derive(Debug, Clone)]
struct JobSpec {
    name: String,
    kind: String,
    command: Vec<String>,
    envs: Vec<(String, String)>,
}

#[derive(Debug)]
struct CompletedCommand {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

impl FullValidationConfig {
    pub fn from_env_and_args(args: &[String]) -> anyhow::Result<Self> {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("preprocessor-cli crate should live under the workspace root")
            .to_path_buf();
        let repo_root = workspace_root
            .parent()
            .expect("workspace root should live under baseline/")
            .parent()
            .expect("baseline should live under the repo root")
            .to_path_buf();

        let run_id = env::var("RUN_ID").unwrap_or_else(|_| Utc::now().format("%Y%m%dT%H%M%SZ").to_string());
        let validation_root = env_path("VALIDATION_ROOT")
            .unwrap_or_else(|| repo_root.join("runs").join(format!("{run_id}-validation")));
        let avare_source_root = env_path("AVARE_SOURCE_ROOT").unwrap_or_else(|| repo_root.join("avare-source"));
        let cache_root = env_path("CACHE_ROOT").unwrap_or_else(|| repo_root.join("cache"));
        let fetch_cache_root =
            env_path("FETCH_CACHE_ROOT").unwrap_or_else(|| cache_root.join("fetch"));
        let fetch_cache_mode = env::var("FETCH_CACHE_MODE").unwrap_or_else(|_| "fill".to_string());
        let fetch_jobs = env_usize("FETCH_JOBS").unwrap_or(4);
        let zip_jobs = env_usize("ZIP_JOBS").unwrap_or(2);
        let cpu_jobs = env_usize("CPU_JOBS").unwrap_or_else(default_cpu_jobs);
        let native_chart_cpu_jobs = env_usize("NATIVE_CHART_CPU_JOBS")
            .unwrap_or_else(|| if cpu_jobs > 8 { 8 } else { cpu_jobs.max(1) });
        let max_heavy_jobs = env_usize("MAX_HEAVY_JOBS").unwrap_or(1).max(1);
        let image_sample_percent = env_u8("IMAGE_SAMPLE_PERCENT").unwrap_or(100);
        let image_rmse_threshold =
            env::var("IMAGE_RMSE_THRESHOLD").unwrap_or_else(|_| "0.0".to_string());

        let mut config = Self {
            repo_root,
            run_id,
            validation_root,
            avare_source_root,
            cache_root,
            fetch_cache_root,
            fetch_cache_mode,
            fetch_jobs,
            zip_jobs,
            cpu_jobs,
            native_chart_cpu_jobs,
            max_heavy_jobs,
            image_sample_percent,
            image_rmse_threshold,
        };

        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--validation-root" => {
                    config.validation_root = PathBuf::from(
                        args.get(index + 1)
                            .cloned()
                            .ok_or_else(|| anyhow::anyhow!("missing value for --validation-root"))?,
                    );
                    index += 2;
                }
                "--run-id" => {
                    config.run_id = args
                        .get(index + 1)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("missing value for --run-id"))?;
                    index += 2;
                }
                "--fetch-cache-mode" => {
                    config.fetch_cache_mode = args
                        .get(index + 1)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("missing value for --fetch-cache-mode"))?;
                    index += 2;
                }
                "--image-sample-percent" => {
                    config.image_sample_percent = args
                        .get(index + 1)
                        .ok_or_else(|| anyhow::anyhow!("missing value for --image-sample-percent"))?
                        .parse()
                        .context("failed to parse --image-sample-percent")?;
                    index += 2;
                }
                "--image-rmse-threshold" => {
                    config.image_rmse_threshold = args
                        .get(index + 1)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("missing value for --image-rmse-threshold"))?;
                    index += 2;
                }
                "--max-heavy-jobs" => {
                    config.max_heavy_jobs = args
                        .get(index + 1)
                        .ok_or_else(|| anyhow::anyhow!("missing value for --max-heavy-jobs"))?
                        .parse()
                        .context("failed to parse --max-heavy-jobs")?;
                    config.max_heavy_jobs = config.max_heavy_jobs.max(1);
                    index += 2;
                }
                _ => bail!("unknown run-full-validation argument: {}", args[index]),
            }
        }

        Ok(config)
    }

    fn legacy_run_root(&self) -> PathBuf {
        self.validation_root.join("legacy")
    }

    fn native_root(&self) -> PathBuf {
        self.validation_root.join("native")
    }

    fn prep_root(&self) -> PathBuf {
        self.validation_root.join("prep")
    }

    fn compare_root(&self) -> PathBuf {
        self.validation_root.join("compare")
    }

    fn log_root(&self) -> PathBuf {
        self.validation_root.join("orchestrator-logs")
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var(name).ok().map(PathBuf::from)
}

fn env_usize(name: &str) -> Option<usize> {
    env::var(name).ok()?.parse().ok()
}

fn env_u8(name: &str) -> Option<u8> {
    env::var(name).ok()?.parse().ok()
}

fn default_cpu_jobs() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(8)
}

fn require_commands(commands: &[&str]) -> anyhow::Result<()> {
    for command in commands {
        if !command_exists(command) {
            bail!("required command not found: {command}");
        }
    }
    Ok(())
}

fn command_exists(command: &str) -> bool {
    if command.contains('/') {
        return Path::new(command).is_file();
    }
    let Some(paths) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&paths).any(|dir| dir.join(command).is_file())
}

pub fn maybe_reexec_under_cgroup(args: &[String]) -> anyhow::Result<bool> {
    if env::var_os(CGROUP_ACTIVE_ENV).is_some() {
        return Ok(false);
    }
    if !command_exists("systemd-run") {
        return Ok(false);
    }
    let memory_max =
        env::var("FULL_VALIDATION_MEMORY_MAX").unwrap_or_else(|_| DEFAULT_MEMORY_MAX.to_string());
    let current_exe = env::current_exe().context("failed to resolve current executable")?;
    let status = Command::new("systemd-run")
        .args(["--quiet", "--wait", "--collect"])
        .args(["-p", &format!("MemoryMax={memory_max}")])
        .args(["-p", "MemorySwapMax=0"])
        .args(["-p", "OOMPolicy=kill"])
        .arg("env")
        .arg(format!("{CGROUP_ACTIVE_ENV}=1"))
        .arg(current_exe)
        .arg("run-full-validation")
        .args(args)
        .status()
        .context("failed to re-exec full validation under systemd-run")?;
    let exit_code = status.code().unwrap_or(1);
    if exit_code == 0 {
        return Ok(true);
    }
    bail!("full validation cgroup wrapper exited with code {exit_code}");
}

fn run_command_capture(command: &mut Command) -> anyhow::Result<CompletedCommand> {
    let output = command.output().context("failed to spawn command")?;
    Ok(CompletedCommand {
        exit_code: output.status.code().unwrap_or(1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn run_command_success(command: &mut Command, context: &str) -> anyhow::Result<()> {
    let completed = run_command_capture(command)?;
    if completed.exit_code != 0 {
        bail!(
            "{context} failed with exit code {}\nstdout:\n{}\nstderr:\n{}",
            completed.exit_code,
            completed.stdout,
            completed.stderr
        );
    }
    Ok(())
}

fn spawn_logged(
    name: &str,
    log_root: &Path,
    command: &mut Command,
) -> anyhow::Result<SpawnedJob> {
    let stdout_path = log_root.join(format!("{name}.stdout.log"));
    let stderr_path = log_root.join(format!("{name}.stderr.log"));
    let stdout = File::create(&stdout_path)
        .with_context(|| format!("failed to create {}", stdout_path.display()))?;
    let stderr = File::create(&stderr_path)
        .with_context(|| format!("failed to create {}", stderr_path.display()))?;
    let child = command
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .with_context(|| format!("failed to spawn job {name}"))?;
    Ok(SpawnedJob {
        name: name.to_string(),
        child,
    })
}

fn write_compare_output(compare_root: &Path, name: &str, completed: &CompletedCommand) -> anyhow::Result<()> {
    let path = compare_root.join(format!("{name}.txt"));
    let mut file = File::create(&path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    file.write_all(completed.stdout.as_bytes())?;
    if !completed.stderr.trim().is_empty() {
        if !completed.stdout.ends_with('\n') && !completed.stdout.is_empty() {
            file.write_all(b"\n")?;
        }
        file.write_all(b"stderr:\n")?;
        file.write_all(completed.stderr.as_bytes())?;
    }
    Ok(())
}

fn first_manifest_line(path: &Path) -> anyhow::Result<String> {
    let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    text.lines()
        .next()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("manifest {} was empty", path.display()))
}

struct MasterLog {
    start: Instant,
    file: File,
}

impl MasterLog {
    fn create(path: &Path) -> anyhow::Result<Self> {
        let file = File::create(path)
            .with_context(|| format!("failed to create {}", path.display()))?;
        Ok(Self {
            start: Instant::now(),
            file,
        })
    }

    fn log(&mut self, message: impl AsRef<str>) -> anyhow::Result<()> {
        writeln!(self.file, "{} {}", format_elapsed(self.start.elapsed().as_secs()), message.as_ref())
            .context("failed to write master log")?;
        self.file.flush().context("failed to flush master log")?;
        Ok(())
    }
}

fn format_elapsed(elapsed_secs: u64) -> String {
    let hours = elapsed_secs / 3600;
    let minutes = (elapsed_secs % 3600) / 60;
    let seconds = elapsed_secs % 60;
    if hours == 0 {
        format!("+{}:{seconds:02}", minutes)
    } else {
        format!("+{}:{minutes:02}:{seconds:02}", hours)
    }
}

fn append_summary_file(compare_root: &Path, job_statuses: &BTreeMap<String, i32>, compare_statuses: &BTreeMap<String, i32>) -> anyhow::Result<String> {
    let summary_path = compare_root.join("summary.txt");
    let mut summary = String::new();
    summary.push_str("job statuses\n");
    for (name, exit_code) in job_statuses {
        summary.push_str(&format!("job {name} exit_code={exit_code}\n"));
    }
    for (name, exit_code) in compare_statuses {
        summary.push_str(&format!("compare {name} exit_code={exit_code}\n"));
    }
    let mut compare_files = fs::read_dir(compare_root)?
        .collect::<Result<Vec<_>, _>>()
        .context("failed to list compare dir")?;
    compare_files.sort_by_key(|entry| entry.path());
    for entry in compare_files {
        let path = entry.path();
        if path.file_name().and_then(|name| name.to_str()) == Some("summary.txt") {
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("txt") {
            continue;
        }
        summary.push_str(&format!(
            "===== {} =====\n",
            path.file_name().and_then(|name| name.to_str()).unwrap_or("unknown")
        ));
        summary.push_str(&fs::read_to_string(&path)?);
        if !summary.ends_with('\n') {
            summary.push('\n');
        }
    }
    fs::write(&summary_path, &summary)
        .with_context(|| format!("failed to write {}", summary_path.display()))?;
    Ok(summary)
}

fn self_command(current_exe: &Path) -> Command {
    Command::new(current_exe)
}

fn configured_command(current_exe: &Path, configure: impl FnOnce(&mut Command)) -> Command {
    let mut command = self_command(current_exe);
    configure(&mut command);
    command
}

fn compare_and_record(compare_root: &Path, name: &str, mut command: Command, compare_statuses: &mut BTreeMap<String, i32>) -> anyhow::Result<()> {
    let completed = run_command_capture(&mut command)?;
    write_compare_output(compare_root, name, &completed)?;
    compare_statuses.insert(name.to_string(), completed.exit_code);
    Ok(())
}

fn spawn_job_from_spec(spec: &JobSpec, log_root: &Path) -> anyhow::Result<SpawnedJob> {
    let program = spec
        .command
        .first()
        .ok_or_else(|| anyhow::anyhow!("job {} has empty command", spec.name))?;
    let mut command = Command::new(program);
    for arg in &spec.command[1..] {
        command.arg(arg);
    }
    for (key, value) in &spec.envs {
        command.env(key, value);
    }
    spawn_logged(&spec.name, log_root, &mut command)
}

fn wait_for_any_job(
    running: &mut Vec<SpawnedJob>,
    statuses: &mut BTreeMap<String, i32>,
) -> anyhow::Result<(String, i32)> {
    loop {
        let mut finished_index = None;
        let mut finished_name = String::new();
        let mut finished_code = 1;
        for (index, job) in running.iter_mut().enumerate() {
            if let Some(status) = job
                .child
                .try_wait()
                .with_context(|| format!("failed to poll job {}", job.name))?
            {
                finished_index = Some(index);
                finished_name = job.name.clone();
                finished_code = status.code().unwrap_or(1);
                break;
            }
        }
        if let Some(index) = finished_index {
            running.remove(index);
            statuses.insert(finished_name.clone(), finished_code);
            return Ok((finished_name, finished_code));
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

pub fn run_full_validation(config: &FullValidationConfig) -> anyhow::Result<()> {
    let compare_root = config.compare_root();
    let log_root = config.log_root();
    fs::create_dir_all(&config.validation_root)?;
    fs::create_dir_all(config.prep_root())?;
    fs::create_dir_all(&compare_root)?;
    fs::create_dir_all(&log_root)?;
    fs::create_dir_all(&config.cache_root)?;
    fs::create_dir_all(&config.fetch_cache_root)?;
    let mut master_log = MasterLog::create(&log_root.join("master.log"))?;
    master_log.log(format!(
        "begin run_id={} validation_root={} max_heavy_jobs={} native_chart_cpu_jobs={} fetch_cache_mode={}",
        config.run_id,
        config.validation_root.display(),
        config.max_heavy_jobs,
        config.native_chart_cpu_jobs,
        config.fetch_cache_mode
    ))?;
    let result = (|| -> anyhow::Result<()> {
    require_commands(&[
        "git",
        "python3",
        "cargo",
        "unzip",
        "zip",
        "sha256sum",
        "/bin/bash",
    ])?;
    master_log.log("verified required commands")?;

    run_command_success(
        Command::new("/bin/bash").arg(config.repo_root.join("legacy-capture/hydrate_legacy_sources.sh")),
        "hydrate_legacy_sources.sh",
    )?;
    master_log.log("hydrated legacy sources")?;

    let source_urls_dir = config.prep_root().join("source-urls");
    fs::create_dir_all(&source_urls_dir)?;
    let emit_log_out = log_root.join("emit-source-urls.stdout.log");
    let emit_log_err = log_root.join("emit-source-urls.stderr.log");
    let emit_completed = run_command_capture(
        Command::new("python3")
            .env("FETCH_CACHE_ROOT", &config.fetch_cache_root)
            .env("FETCH_CACHE_MODE", &config.fetch_cache_mode)
            .arg(config.repo_root.join("legacy-capture/emit_source_urls.py"))
            .args(["--avare-source-root", &config.avare_source_root.display().to_string()])
            .args(["--output-dir", &source_urls_dir.display().to_string()]),
    )?;
    fs::write(&emit_log_out, &emit_completed.stdout)?;
    fs::write(&emit_log_err, &emit_completed.stderr)?;
    if emit_completed.exit_code != 0 {
        bail!("emit_source_urls.py failed; see {}", emit_log_err.display());
    }
    master_log.log("emitted source url manifests")?;

    let current_exe = env::current_exe().context("failed to resolve current executable")?;
    let mut pending_jobs = Vec::new();

    pending_jobs.push(JobSpec {
        name: "legacy".to_string(),
        kind: "legacy".to_string(),
        command: vec![
            "/bin/bash".to_string(),
            config
                .repo_root
                .join("legacy-capture/run_legacy_capture_direct.sh")
                .display()
                .to_string(),
        ],
        envs: vec![
            ("RUN_ID".to_string(), format!("{}-legacy", config.run_id)),
            ("OUTPUT_ROOT".to_string(), config.legacy_run_root().display().to_string()),
            ("CACHE_ROOT".to_string(), config.cache_root.display().to_string()),
            ("FETCH_CACHE_ROOT".to_string(), config.fetch_cache_root.display().to_string()),
            ("FETCH_CACHE_MODE".to_string(), config.fetch_cache_mode.clone()),
            ("CPU_JOBS".to_string(), config.cpu_jobs.to_string()),
            ("FETCH_JOBS".to_string(), config.fetch_jobs.to_string()),
            ("ZIP_JOBS".to_string(), config.zip_jobs.to_string()),
        ],
    });

    for family in ["sec", "tac", "enr-l", "enr-h"] {
        pending_jobs.push(JobSpec {
            name: format!("native-charts-{family}"),
            kind: "chart".to_string(),
            command: vec![
                current_exe.display().to_string(),
                "run-native-chart".to_string(),
                "--family".to_string(),
                family.to_string(),
                "--source-repo".to_string(),
                config.avare_source_root.join("charts").display().to_string(),
                "--run-root".to_string(),
                config
                    .native_root()
                    .join(format!("charts-{family}"))
                    .display()
                    .to_string(),
                "--cpu-jobs".to_string(),
                config.native_chart_cpu_jobs.to_string(),
                "--prefetch-source-urls".to_string(),
                source_urls_dir
                    .join(format!("charts-{family}/source_urls.jsonl"))
                    .display()
                    .to_string(),
                "--fetch-jobs".to_string(),
                config.fetch_jobs.to_string(),
            ],
            envs: vec![
                ("FETCH_CACHE_ROOT".to_string(), config.fetch_cache_root.display().to_string()),
                ("FETCH_CACHE_MODE".to_string(), config.fetch_cache_mode.clone()),
            ],
        });
    }

    pending_jobs.push(JobSpec {
        name: "native-csup".to_string(),
        kind: "csup".to_string(),
        command: vec![
            current_exe.display().to_string(),
            "run-native-csup".to_string(),
            "--source-repo".to_string(),
            config.avare_source_root.join("csup").display().to_string(),
            "--run-root".to_string(),
            config.native_root().join("csup").display().to_string(),
            "--prefetch-source-urls".to_string(),
            source_urls_dir.join("csup/source_urls.jsonl").display().to_string(),
            "--fetch-jobs".to_string(),
            config.fetch_jobs.to_string(),
        ],
        envs: vec![
            ("FETCH_CACHE_ROOT".to_string(), config.fetch_cache_root.display().to_string()),
            ("FETCH_CACHE_MODE".to_string(), config.fetch_cache_mode.clone()),
        ],
    });

    pending_jobs.push(JobSpec {
        name: "native-tpp-ne".to_string(),
        kind: "tpp".to_string(),
        command: vec![
            current_exe.display().to_string(),
            "run-native-tpp".to_string(),
            "--region".to_string(),
            "NE".to_string(),
            "--source-repo".to_string(),
            config.avare_source_root.join("tpp").display().to_string(),
            "--run-root".to_string(),
            config.native_root().join("tpp-ne").display().to_string(),
            "--prefetch-source-urls".to_string(),
            source_urls_dir.join("tpp-ne/source_urls.jsonl").display().to_string(),
            "--fetch-jobs".to_string(),
            config.fetch_jobs.to_string(),
        ],
        envs: vec![
            ("FETCH_CACHE_ROOT".to_string(), config.fetch_cache_root.display().to_string()),
            ("FETCH_CACHE_MODE".to_string(), config.fetch_cache_mode.clone()),
        ],
    });

    pending_jobs.push(JobSpec {
        name: "native-tpp-nw".to_string(),
        kind: "tpp".to_string(),
        command: vec![
            current_exe.display().to_string(),
            "run-native-tpp".to_string(),
            "--region".to_string(),
            "NW".to_string(),
            "--source-repo".to_string(),
            config.avare_source_root.join("tpp").display().to_string(),
            "--run-root".to_string(),
            config.native_root().join("tpp-nw").display().to_string(),
            "--prefetch-source-urls".to_string(),
            source_urls_dir.join("tpp-nw/source_urls.jsonl").display().to_string(),
            "--fetch-jobs".to_string(),
            config.fetch_jobs.to_string(),
        ],
        envs: vec![
            ("FETCH_CACHE_ROOT".to_string(), config.fetch_cache_root.display().to_string()),
            ("FETCH_CACHE_MODE".to_string(), config.fetch_cache_mode.clone()),
        ],
    });

    let total_heavy_jobs = pending_jobs.len();
    let mut running_jobs = Vec::new();
    let mut job_statuses = BTreeMap::new();
    let mut launched_jobs = 0usize;
    while !pending_jobs.is_empty() || !running_jobs.is_empty() {
        while running_jobs.len() < config.max_heavy_jobs && !pending_jobs.is_empty() {
            let spec = pending_jobs.remove(0);
            let job = spawn_job_from_spec(&spec, &log_root)?;
            launched_jobs += 1;
            master_log.log(format!(
                "launch {} kind={} progress launched={}/{} running={} completed={}",
                spec.name,
                spec.kind,
                launched_jobs,
                total_heavy_jobs,
                running_jobs.len() + 1,
                job_statuses.len()
            ))?;
            running_jobs.push(job);
        }
        if !running_jobs.is_empty() {
            let completed = wait_for_any_job(&mut running_jobs, &mut job_statuses)?;
            master_log.log(format!(
                "complete {} exit_code={} progress completed={}/{} running={} pending={}",
                completed.0,
                completed.1,
                job_statuses.len(),
                total_heavy_jobs,
                running_jobs.len(),
                pending_jobs.len()
            ))?;
        }
    }

    if job_statuses.get("legacy") == Some(&0) {
        let legacy_data_root = config.legacy_run_root().join("work/data");
        let manifest_version = first_manifest_line(&legacy_data_root.join("databases"))?;
        master_log.log("launch native-data kind=data progress phase=post-legacy")?;
        let mut build_data = self_command(&current_exe);
        build_data
            .arg("build-data")
            .args(["--input-dir", &legacy_data_root.display().to_string()])
            .args(["--output-dir", &config.native_root().join("data").display().to_string()])
            .args(["--manifest-version", &manifest_version]);
        let completed = run_command_capture(&mut build_data)?;
        fs::write(log_root.join("native-data.stdout.log"), &completed.stdout)?;
        fs::write(log_root.join("native-data.stderr.log"), &completed.stderr)?;
        job_statuses.insert("native-data".to_string(), completed.exit_code);
        master_log.log(format!("complete native-data exit_code={}", completed.exit_code))?;
    } else {
        job_statuses.insert("native-data".to_string(), 1);
        master_log.log("skip native-data because legacy failed")?;
    }

    fs::write(
        compare_root.join("job-status.txt"),
        job_statuses
            .iter()
            .map(|(name, exit_code)| format!("job {name} exit_code={exit_code}\n"))
            .collect::<String>(),
    )?;

    let mut compare_statuses = BTreeMap::new();
    master_log.log("begin compare phase")?;

    let legacy_root = config.legacy_run_root();
    let native_root = config.native_root();

    if job_statuses.get("legacy") == Some(&0) && job_statuses.get("native-charts-sec") == Some(&0) {
        compare_and_record(&compare_root, "charts-sec-tile-paths", configured_command(&current_exe, |cmd| {
            cmd.arg("compare-chart-tile-paths")
                .args(["--family", "sec"])
                .args(["--legacy-work-dir", &legacy_root.join("work/charts-sec").display().to_string()])
                .args(["--rust-work-dir", &native_root.join("charts-sec/work/charts-sec").display().to_string()]);
        }), &mut compare_statuses)?;
        master_log.log(format!(
            "compare charts-sec-tile-paths exit_code={}",
            compare_statuses.get("charts-sec-tile-paths").copied().unwrap_or(1)
        ))?;
        compare_and_record(&compare_root, "charts-sec-packages", configured_command(&current_exe, |cmd| {
            cmd.arg("compare-chart-packages")
                .args(["--family", "sec"])
                .args(["--legacy-work-dir", &legacy_root.join("work/charts-sec").display().to_string()])
                .args(["--rust-work-dir", &native_root.join("charts-sec/work/charts-sec").display().to_string()]);
        }), &mut compare_statuses)?;
        master_log.log(format!(
            "compare charts-sec-packages exit_code={}",
            compare_statuses.get("charts-sec-packages").copied().unwrap_or(1)
        ))?;
        compare_and_record(&compare_root, "charts-sec-provenance", configured_command(&current_exe, |cmd| {
            cmd.arg("compare-provenance")
                .args(["--left-provenance-dir", &legacy_root.join("meta/provenance/charts-sec").display().to_string()])
                .args(["--right-provenance-dir", &native_root.join("charts-sec/meta/provenance/charts-sec").display().to_string()]);
        }), &mut compare_statuses)?;
        master_log.log(format!(
            "compare charts-sec-provenance exit_code={}",
            compare_statuses.get("charts-sec-provenance").copied().unwrap_or(1)
        ))?;
        compare_and_record(&compare_root, "charts-sec-images", configured_command(&current_exe, |cmd| {
            cmd.arg("compare-sampled-images")
                .args(["--left-root", &legacy_root.join("work/charts-sec/tiles/0").display().to_string()])
                .args(["--right-root", &native_root.join("charts-sec/work/charts-sec/tiles/0").display().to_string()])
                .args(["--sample-percent", &config.image_sample_percent.to_string()])
                .args(["--rmse-threshold", &config.image_rmse_threshold]);
        }), &mut compare_statuses)?;
        master_log.log(format!(
            "compare charts-sec-images exit_code={}",
            compare_statuses.get("charts-sec-images").copied().unwrap_or(1)
        ))?;
    }

    for (family, tile_index) in [("tac", "1"), ("enr-l", "3"), ("enr-h", "4")] {
        let native_job = format!("native-charts-{family}");
        if job_statuses.get("legacy") == Some(&0) && job_statuses.get(&native_job) == Some(&0) {
            let legacy_dir = legacy_root.join(format!("work/charts-{family}"));
            let native_dir = native_root.join(format!("charts-{family}/work/charts-{family}"));
            compare_and_record(&compare_root, &format!("charts-{family}-tile-paths"), configured_command(&current_exe, |cmd| {
                cmd.arg("compare-chart-tile-paths")
                    .args(["--family", family])
                    .args(["--legacy-work-dir", &legacy_dir.display().to_string()])
                    .args(["--rust-work-dir", &native_dir.display().to_string()]);
            }), &mut compare_statuses)?;
            master_log.log(format!(
                "compare charts-{family}-tile-paths exit_code={}",
                compare_statuses.get(&format!("charts-{family}-tile-paths")).copied().unwrap_or(1)
            ))?;
            compare_and_record(&compare_root, &format!("charts-{family}-packages"), configured_command(&current_exe, |cmd| {
                cmd.arg("compare-chart-packages")
                    .args(["--family", family])
                    .args(["--legacy-work-dir", &legacy_dir.display().to_string()])
                    .args(["--rust-work-dir", &native_dir.display().to_string()]);
            }), &mut compare_statuses)?;
            master_log.log(format!(
                "compare charts-{family}-packages exit_code={}",
                compare_statuses.get(&format!("charts-{family}-packages")).copied().unwrap_or(1)
            ))?;
            compare_and_record(&compare_root, &format!("charts-{family}-provenance"), configured_command(&current_exe, |cmd| {
                cmd.arg("compare-provenance")
                    .args(["--left-provenance-dir", &legacy_root.join(format!("meta/provenance/charts-{family}")).display().to_string()])
                    .args(["--right-provenance-dir", &native_root.join(format!("charts-{family}/meta/provenance/charts-{family}")).display().to_string()]);
            }), &mut compare_statuses)?;
            master_log.log(format!(
                "compare charts-{family}-provenance exit_code={}",
                compare_statuses.get(&format!("charts-{family}-provenance")).copied().unwrap_or(1)
            ))?;
            compare_and_record(&compare_root, &format!("charts-{family}-images"), configured_command(&current_exe, |cmd| {
                cmd.arg("compare-sampled-images")
                    .args(["--left-root", &legacy_root.join(format!("work/charts-{family}/tiles/{tile_index}")).display().to_string()])
                    .args(["--right-root", &native_dir.join(format!("tiles/{tile_index}")).display().to_string()])
                    .args(["--sample-percent", &config.image_sample_percent.to_string()])
                    .args(["--rmse-threshold", &config.image_rmse_threshold]);
            }), &mut compare_statuses)?;
            master_log.log(format!(
                "compare charts-{family}-images exit_code={}",
                compare_statuses.get(&format!("charts-{family}-images")).copied().unwrap_or(1)
            ))?;
        }
    }

    if job_statuses.get("legacy") == Some(&0) && job_statuses.get("native-csup") == Some(&0) {
        compare_and_record(&compare_root, "csup-packages", configured_command(&current_exe, |cmd| {
            cmd.arg("compare-csup-packages")
                .args(["--legacy-work-dir", &legacy_root.join("work/csup").display().to_string()])
                .args(["--rust-work-dir", &native_root.join("csup/work/csup").display().to_string()]);
        }), &mut compare_statuses)?;
        master_log.log(format!(
            "compare csup-packages exit_code={}",
            compare_statuses.get("csup-packages").copied().unwrap_or(1)
        ))?;
        compare_and_record(&compare_root, "csup-provenance", configured_command(&current_exe, |cmd| {
            cmd.arg("compare-provenance")
                .args(["--left-provenance-dir", &legacy_root.join("meta/provenance/csup").display().to_string()])
                .args(["--right-provenance-dir", &native_root.join("csup/meta/provenance/csup").display().to_string()]);
        }), &mut compare_statuses)?;
        master_log.log(format!(
            "compare csup-provenance exit_code={}",
            compare_statuses.get("csup-provenance").copied().unwrap_or(1)
        ))?;
        compare_and_record(&compare_root, "csup-images", configured_command(&current_exe, |cmd| {
            cmd.arg("compare-csup-images")
                .args(["--legacy-work-dir", &legacy_root.join("work/csup").display().to_string()])
                .args(["--rust-work-dir", &native_root.join("csup/work/csup").display().to_string()])
                .args(["--sample-percent", &config.image_sample_percent.to_string()])
                .args(["--rmse-threshold", &config.image_rmse_threshold]);
        }), &mut compare_statuses)?;
        master_log.log(format!(
            "compare csup-images exit_code={}",
            compare_statuses.get("csup-images").copied().unwrap_or(1)
        ))?;
    }

    if job_statuses.get("legacy") == Some(&0) && job_statuses.get("native-tpp-ne") == Some(&0) {
        compare_and_record(&compare_root, "tpp-ne-packages", configured_command(&current_exe, |cmd| {
            cmd.arg("compare-tpp-packages")
                .args(["--region", "NE"])
                .args(["--legacy-work-dir", &legacy_root.join("work/tpp-ne").display().to_string()])
                .args(["--rust-work-dir", &native_root.join("tpp-ne/work/tpp-ne").display().to_string()]);
        }), &mut compare_statuses)?;
        master_log.log(format!(
            "compare tpp-ne-packages exit_code={}",
            compare_statuses.get("tpp-ne-packages").copied().unwrap_or(1)
        ))?;
        compare_and_record(&compare_root, "tpp-ne-provenance", configured_command(&current_exe, |cmd| {
            cmd.arg("compare-provenance")
                .args(["--left-provenance-dir", &legacy_root.join("meta/provenance/tpp-ne").display().to_string()])
                .args(["--right-provenance-dir", &native_root.join("tpp-ne/meta/provenance/tpp-ne").display().to_string()]);
        }), &mut compare_statuses)?;
        master_log.log(format!(
            "compare tpp-ne-provenance exit_code={}",
            compare_statuses.get("tpp-ne-provenance").copied().unwrap_or(1)
        ))?;
        compare_and_record(&compare_root, "tpp-ne-images", configured_command(&current_exe, |cmd| {
            cmd.arg("compare-tpp-images")
                .args(["--region", "NE"])
                .args(["--legacy-work-dir", &legacy_root.join("work/tpp-ne").display().to_string()])
                .args(["--rust-work-dir", &native_root.join("tpp-ne/work/tpp-ne").display().to_string()])
                .args(["--sample-percent", &config.image_sample_percent.to_string()])
                .args(["--rmse-threshold", &config.image_rmse_threshold]);
        }), &mut compare_statuses)?;
        master_log.log(format!(
            "compare tpp-ne-images exit_code={}",
            compare_statuses.get("tpp-ne-images").copied().unwrap_or(1)
        ))?;
    }

    if job_statuses.get("legacy") == Some(&0) && job_statuses.get("native-tpp-nw") == Some(&0) {
        compare_and_record(&compare_root, "tpp-nw-packages", configured_command(&current_exe, |cmd| {
            cmd.arg("compare-tpp-packages")
                .args(["--region", "NW"])
                .args(["--legacy-work-dir", &legacy_root.join("work/tpp-nw").display().to_string()])
                .args(["--rust-work-dir", &native_root.join("tpp-nw/work/tpp-nw").display().to_string()]);
        }), &mut compare_statuses)?;
        master_log.log(format!(
            "compare tpp-nw-packages exit_code={}",
            compare_statuses.get("tpp-nw-packages").copied().unwrap_or(1)
        ))?;
        compare_and_record(&compare_root, "tpp-nw-provenance", configured_command(&current_exe, |cmd| {
            cmd.arg("compare-provenance")
                .args(["--left-provenance-dir", &legacy_root.join("meta/provenance/tpp-nw").display().to_string()])
                .args(["--right-provenance-dir", &native_root.join("tpp-nw/meta/provenance/tpp-nw").display().to_string()]);
        }), &mut compare_statuses)?;
        master_log.log(format!(
            "compare tpp-nw-provenance exit_code={}",
            compare_statuses.get("tpp-nw-provenance").copied().unwrap_or(1)
        ))?;
        compare_and_record(&compare_root, "tpp-nw-images", configured_command(&current_exe, |cmd| {
            cmd.arg("compare-tpp-images")
                .args(["--region", "NW"])
                .args(["--legacy-work-dir", &legacy_root.join("work/tpp-nw").display().to_string()])
                .args(["--rust-work-dir", &native_root.join("tpp-nw/work/tpp-nw").display().to_string()])
                .args(["--sample-percent", &config.image_sample_percent.to_string()])
                .args(["--rmse-threshold", &config.image_rmse_threshold]);
        }), &mut compare_statuses)?;
        master_log.log(format!(
            "compare tpp-nw-images exit_code={}",
            compare_statuses.get("tpp-nw-images").copied().unwrap_or(1)
        ))?;
    }

    if job_statuses.get("legacy") == Some(&0) && job_statuses.get("native-data") == Some(&0) {
        compare_and_record(&compare_root, "data-db", configured_command(&current_exe, |cmd| {
            cmd.arg("compare-data-db")
                .args(["--left-db", &legacy_root.join("work/data/main.db").display().to_string()])
                .args(["--right-db", &native_root.join("data/main.db").display().to_string()]);
        }), &mut compare_statuses)?;
        master_log.log(format!(
            "compare data-db exit_code={}",
            compare_statuses.get("data-db").copied().unwrap_or(1)
        ))?;
    }

    let summary = append_summary_file(&compare_root, &job_statuses, &compare_statuses)?;
    master_log.log("wrote compare summary")?;
    print!("{summary}");

    let failed_job = job_statuses.iter().find(|(_, code)| **code != 0);
    let failed_compare = compare_statuses.iter().find(|(_, code)| **code != 0);
    if let Some((name, code)) = failed_job {
        bail!("validation job {name} failed with exit code {code}");
    }
    if let Some((name, code)) = failed_compare {
        bail!("validation compare {name} failed with exit code {code}");
    }
    Ok(())
    })();

    match &result {
        Ok(()) => {
            master_log.log("complete PASS")?;
        }
        Err(error) => {
            let message = error.to_string().replace('\n', " | ");
            let _ = master_log.log(format!("complete FAIL error={message}"));
        }
    }

    result
}
