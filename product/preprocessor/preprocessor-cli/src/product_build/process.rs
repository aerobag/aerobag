use super::*;

const DEFAULT_PRODUCT_BUILD_NOFILE_LIMIT: u64 = 65_536;

#[cfg(unix)]
pub(super) fn current_nofile_limits() -> anyhow::Result<(u64, u64)> {
    let mut limits = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    let result = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limits) };
    if result != 0 {
        anyhow::bail!(
            "failed to read RLIMIT_NOFILE: {}",
            std::io::Error::last_os_error()
        );
    }
    Ok((limits.rlim_cur, limits.rlim_max))
}

#[cfg(not(unix))]
pub(super) fn current_nofile_limits() -> anyhow::Result<(u64, u64)> {
    Ok((
        DEFAULT_PRODUCT_BUILD_NOFILE_LIMIT,
        DEFAULT_PRODUCT_BUILD_NOFILE_LIMIT,
    ))
}

fn configured_nofile_limit() -> anyhow::Result<u64> {
    env::var("PRODUCT_BUILD_NOFILE_LIMIT")
        .ok()
        .map(|value| {
            value
                .parse::<u64>()
                .with_context(|| format!("invalid PRODUCT_BUILD_NOFILE_LIMIT={value}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(DEFAULT_PRODUCT_BUILD_NOFILE_LIMIT))
}

#[cfg(unix)]
pub fn ensure_nofile_limit() -> anyhow::Result<()> {
    let configured = configured_nofile_limit()?;
    let (soft, hard) = current_nofile_limits()?;
    let target = configured.min(hard);
    if soft >= target {
        return Ok(());
    }
    let limits = libc::rlimit {
        rlim_cur: target,
        rlim_max: hard,
    };
    let result = unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limits) };
    if result != 0 {
        anyhow::bail!(
            "failed to set RLIMIT_NOFILE soft limit to {target}: {}",
            std::io::Error::last_os_error()
        );
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn ensure_nofile_limit() -> anyhow::Result<()> {
    let _ = configured_nofile_limit()?;
    Ok(())
}

pub fn maybe_reexec_build_under_cgroup(
    command_name: &str,
    args: &[String],
) -> anyhow::Result<bool> {
    if env::var_os(PRODUCT_BUILD_CGROUP_ACTIVE_ENV).is_some() {
        return Ok(false);
    }
    if !command_exists("systemd-run") {
        return Ok(false);
    }
    let memory_max = env::var("PRODUCT_BUILD_MEMORY_MAX")
        .unwrap_or_else(|_| DEFAULT_PRODUCT_BUILD_MEMORY_MAX.to_string());
    let nofile_limit = configured_nofile_limit()?;
    let current_exe = env::current_exe().context("failed to resolve current executable")?;
    let status = Command::new("systemd-run")
        .args(["--quiet", "--wait", "--collect", "--pipe", "--same-dir"])
        .args(["-p", &format!("MemoryMax={memory_max}")])
        .args(["-p", &format!("LimitNOFILE={nofile_limit}")])
        .args(["-p", "MemorySwapMax=0"])
        .args(["-p", "OOMPolicy=kill"])
        .arg("env")
        .arg(format!("{PRODUCT_BUILD_CGROUP_ACTIVE_ENV}=1"))
        .args(
            env::var("AEROBAG_ARTIFACT_WRITE_PATH")
                .ok()
                .into_iter()
                .map(|value| format!("AEROBAG_ARTIFACT_WRITE_PATH={value}")),
        )
        .arg(current_exe)
        .arg(command_name)
        .args(args)
        .status()
        .context("failed to re-exec product build under systemd-run")?;
    let exit_code = status.code().unwrap_or(1);
    if exit_code == 0 {
        return Ok(true);
    }
    bail!("cycle build cgroup wrapper exited with code {exit_code}");
}

pub(super) fn command_exists(name: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name} >/dev/null 2>&1"))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
