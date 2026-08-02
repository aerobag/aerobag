// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    env,
    fs::{self, File, OpenOptions},
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
};

use aerobag_cloud_server::{run_server, AccountMode, CloudStore, ServerConfig, StoreConfig};
use anyhow::{bail, Context as _};
use fs2::FileExt as _;

fn usage() -> &'static str {
    "usage:
  aerobag-cloud-serverd serve --data-root <path> --server-secret <path> [--listen <addr>]
  aerobag-cloud-serverd gc --data-root <path> [--grace-seconds <n>]
  aerobag-cloud-serverd set-mode --data-root <path> <normal|read-only|suspended>
  aerobag-cloud-serverd set-account-mode --data-root <path> <account> <normal|read-only|suspended>
  aerobag-cloud-serverd set-account-quota --data-root <path> <account> <bytes>
  aerobag-cloud-serverd delete-account --data-root <path> <account>"
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        bail!(usage());
    }
    let command = args.remove(0);
    let data_root = take_option(&mut args, "--data-root")
        .map(PathBuf::from)
        .context("--data-root is required")?;
    match command.as_str() {
        "serve" => {
            let server_secret_path = take_option(&mut args, "--server-secret")
                .map(PathBuf::from)
                .context("--server-secret is required for serve")?;
            let listen =
                take_option(&mut args, "--listen").unwrap_or_else(|| "127.0.0.1:18096".to_string());
            reject_extra(&args)?;
            let _serve_lock = acquire_serve_lock(&data_root)?;
            let store = open_store(&data_root)?;
            run_server(
                store,
                ServerConfig {
                    listen: SocketAddr::from_str(&listen).context("invalid --listen address")?,
                    server_secret_path,
                },
            )
            .await
        }
        "gc" => {
            let grace_seconds = take_option(&mut args, "--grace-seconds")
                .map(|value| value.parse::<i64>().context("invalid --grace-seconds"))
                .transpose()?
                .unwrap_or(24 * 60 * 60);
            if grace_seconds < 0 {
                bail!("--grace-seconds must not be negative");
            }
            reject_extra(&args)?;
            let store = open_store(&data_root)?;
            let report = store.run_gc(now_epoch_ms(), grace_seconds * 1_000)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        "set-mode" => {
            let mode = parse_mode(required_arg(&mut args, "mode")?)?;
            reject_extra(&args)?;
            let store = open_store(&data_root)?;
            store.set_service_mode(mode, now_epoch_ms())?;
            Ok(())
        }
        "set-account-mode" => {
            let account = required_arg(&mut args, "account")?;
            let mode = parse_mode(required_arg(&mut args, "mode")?)?;
            reject_extra(&args)?;
            let store = open_store(&data_root)?;
            store.set_account_mode(&account, mode, now_epoch_ms())?;
            Ok(())
        }
        "set-account-quota" => {
            let account = required_arg(&mut args, "account")?;
            let quota = required_arg(&mut args, "bytes")?
                .parse::<u64>()
                .context("invalid quota bytes")?;
            reject_extra(&args)?;
            let store = open_store(&data_root)?;
            store.set_account_quota(&account, quota, now_epoch_ms())?;
            Ok(())
        }
        "delete-account" => {
            let account = required_arg(&mut args, "account")?;
            reject_extra(&args)?;
            let store = open_store(&data_root)?;
            store.delete_account(&account)?;
            Ok(())
        }
        _ => bail!(usage()),
    }
}

fn open_store(data_root: &Path) -> anyhow::Result<CloudStore> {
    Ok(CloudStore::open(StoreConfig::for_data_root(
        data_root.to_path_buf(),
    ))?)
}

fn acquire_serve_lock(data_root: &Path) -> anyhow::Result<File> {
    fs::create_dir_all(data_root).context("create cloud data root for daemon lock")?;
    let path = data_root.join("serve.lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("open ACS daemon lock {}", path.display()))?;
    file.try_lock_exclusive().with_context(|| {
        format!(
            "another Aerobag Cloud Server daemon already owns {}",
            path.display()
        )
    })?;
    Ok(file)
}

fn take_option(args: &mut Vec<String>, name: &str) -> Option<String> {
    let index = args.iter().position(|arg| arg == name)?;
    if index + 1 >= args.len() {
        return None;
    }
    args.remove(index);
    Some(args.remove(index))
}

fn required_arg(args: &mut Vec<String>, label: &str) -> anyhow::Result<String> {
    if args.is_empty() {
        bail!("missing {label}\n{}", usage());
    }
    Ok(args.remove(0))
}

fn reject_extra(args: &[String]) -> anyhow::Result<()> {
    if !args.is_empty() {
        bail!("unexpected arguments: {}\n{}", args.join(" "), usage());
    }
    Ok(())
}

fn parse_mode(value: String) -> anyhow::Result<AccountMode> {
    match value.as_str() {
        "normal" => Ok(AccountMode::Normal),
        "read-only" => Ok(AccountMode::ReadOnly),
        "suspended" => Ok(AccountMode::Suspended),
        _ => bail!("invalid mode {value:?}"),
    }
}

fn now_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn serve_lock_rejects_a_second_daemon_and_releases_on_drop() {
        let root = TempDir::new().unwrap();
        let first = acquire_serve_lock(root.path()).unwrap();
        assert!(acquire_serve_lock(root.path()).is_err());
        drop(first);
        acquire_serve_lock(root.path()).unwrap();
    }
}
