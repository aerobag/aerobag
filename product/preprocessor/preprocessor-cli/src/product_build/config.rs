use super::*;

impl ProductBuildConfig {
    pub fn from_env_and_args(args: &[String]) -> anyhow::Result<Self> {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("preprocessor-cli crate should live under the workspace root")
            .to_path_buf();
        let repo_root = workspace_root
            .parent()
            .expect("workspace root should live under product/")
            .parent()
            .expect("product should live under the repo root")
            .to_path_buf();
        let artifact_root = default_artifact_write_path(&repo_root);

        let mut profile = ProductBuildProfile::Production;
        let mut chart_cutline_root = repo_root
            .join("third_party")
            .join("apps4av")
            .join("chart-cutlines");
        let mut build_root = match profile {
            ProductBuildProfile::Production => artifact_root.join("published_packaged"),
            ProductBuildProfile::Validation => artifact_root.join("published_packaged_validation"),
        };
        let mut target_cycle = None;
        let mut fetch_jobs = env_usize("FETCH_JOBS").unwrap_or(4);
        let mut cpu_jobs = env_usize("CPU_JOBS").unwrap_or_else(default_cpu_jobs);
        let mut max_heavy_jobs = env_usize("MAX_HEAVY_JOBS").unwrap_or(4).max(1);
        let fetch_cache_root = env_path("FETCH_CACHE_ROOT")
            .unwrap_or_else(|| artifact_root.join("cache").join("fetch"));
        let fetch_cache_mode = env::var("FETCH_CACHE_MODE").unwrap_or_else(|_| "fill".to_string());

        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--profile" => {
                    let value = args.get(index + 1).context("missing value for --profile")?;
                    profile = ProductBuildProfile::parse(value)
                        .ok_or_else(|| anyhow::anyhow!("unsupported profile: {value}"))?;
                    build_root = match profile {
                        ProductBuildProfile::Production => artifact_root.join("published_packaged"),
                        ProductBuildProfile::Validation => {
                            artifact_root.join("published_packaged_validation")
                        }
                    };
                    index += 2;
                }
                "--chart-cutline-root" => {
                    chart_cutline_root = PathBuf::from(
                        args.get(index + 1)
                            .context("missing value for --chart-cutline-root")?,
                    );
                    index += 2;
                }
                "--build-root" => {
                    build_root = PathBuf::from(
                        args.get(index + 1)
                            .context("missing value for --build-root")?,
                    );
                    index += 2;
                }
                "--source-root" => {
                    let _ = args
                        .get(index + 1)
                        .context("missing value for --source-root")?;
                    index += 2;
                }
                "--cycle" => {
                    target_cycle = Some(
                        args.get(index + 1)
                            .context("missing value for --cycle")?
                            .to_string(),
                    );
                    index += 2;
                }
                "--fetch-jobs" => {
                    fetch_jobs = args
                        .get(index + 1)
                        .context("missing value for --fetch-jobs")?
                        .parse()
                        .context("failed to parse --fetch-jobs")?;
                    index += 2;
                }
                "--cpu-jobs" => {
                    cpu_jobs = args
                        .get(index + 1)
                        .context("missing value for --cpu-jobs")?
                        .parse()
                        .context("failed to parse --cpu-jobs")?;
                    index += 2;
                }
                "--max-heavy-jobs" => {
                    max_heavy_jobs = args
                        .get(index + 1)
                        .context("missing value for --max-heavy-jobs")?
                        .parse()
                        .context("failed to parse --max-heavy-jobs")?;
                    max_heavy_jobs = max_heavy_jobs.max(1);
                    index += 2;
                }
                "--as-of-utc" | "--bundle" => {
                    index += 2;
                }
                other => bail!("unknown cycle-build argument: {other}"),
            }
        }

        Ok(Self {
            chart_cutline_root,
            build_root,
            profile,
            target_cycle,
            fetch_jobs,
            cpu_jobs,
            max_heavy_jobs,
            fetch_cache_root,
            fetch_cache_mode,
        })
    }
}

pub(super) fn env_path(name: &str) -> Option<PathBuf> {
    env::var(name).ok().map(PathBuf::from)
}

pub(crate) fn default_artifact_write_path(repo_root: &Path) -> PathBuf {
    if let Some(path) = env_path("AEROBAG_ARTIFACT_WRITE_PATH") {
        return if path.is_absolute() {
            path
        } else {
            repo_root.join(path)
        };
    }
    {
        let config_path = repo_root.join(".aerobag-artifact-write-path");
        let raw = fs::read_to_string(&config_path).unwrap_or_else(|error| {
            panic!(
                "artifact write-path config missing at {} and AEROBAG_ARTIFACT_WRITE_PATH is unset: {error}",
                config_path.display()
            )
        });
        let configured = raw.trim();
        assert!(
            !configured.is_empty(),
            "artifact write-path config at {} is empty",
            config_path.display()
        );
        let path = PathBuf::from(configured);
        if path.is_absolute() {
            path
        } else {
            repo_root.join(path)
        }
    }
}

pub(super) fn env_usize(name: &str) -> Option<usize> {
    env::var(name).ok()?.parse().ok()
}

pub(super) fn env_flag(name: &str) -> bool {
    env::var(name)
        .ok()
        .is_some_and(|value| !matches!(value.as_str(), "" | "0" | "false" | "FALSE" | "no" | "NO"))
}

pub(super) fn default_cpu_jobs() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(8)
}
