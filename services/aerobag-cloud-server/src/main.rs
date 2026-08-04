// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    env,
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
};

use aerobag_cloud_server::{
    create_backup, create_backup_if_due, restore_backup, run_server, verify_backup, AccountMode,
    AcsRuntimePolicy, CloudStore, ServerConfig, StorageLayout,
};
use anyhow::{bail, Context as _};

fn usage() -> &'static str {
    "usage:
  aerobag-cloud-serverd serve --storage-root <path> --policy <path> --server-secret <path> [--listen <addr>]
  aerobag-cloud-serverd gc --storage-root <path> --policy <path> [--grace-seconds <n>]
  aerobag-cloud-serverd backup-now --storage-root <path> --policy <path>
  aerobag-cloud-serverd backup-if-due --storage-root <path> --policy <path>
  aerobag-cloud-serverd verify-backup --storage-root <path> --policy <path> <snapshot-path>
  aerobag-cloud-serverd restore --storage-root <path> --policy <path> <snapshot-path>
  aerobag-cloud-serverd set-mode --storage-root <path> --policy <path> <read-only|suspended>
  aerobag-cloud-serverd resume-writes --storage-root <path> --policy <path>
  aerobag-cloud-serverd force-resume-writes --storage-root <path> --policy <path> --reason <text>
  aerobag-cloud-serverd set-account-mode --storage-root <path> --policy <path> <account> <normal|read-only|suspended>
  aerobag-cloud-serverd set-account-quota --storage-root <path> --policy <path> <account> <bytes>
  aerobag-cloud-serverd delete-account --storage-root <path> --policy <path> <account>"
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        bail!(usage());
    }
    let command = args.remove(0);
    let storage_root = take_option(&mut args, "--storage-root")
        .map(PathBuf::from)
        .context("--storage-root is required")?;
    let policy_path = take_option(&mut args, "--policy")
        .map(PathBuf::from)
        .context("--policy is required")?;
    let policy = AcsRuntimePolicy::load(&policy_path)?;
    match command.as_str() {
        "serve" => {
            let server_secret_path = take_option(&mut args, "--server-secret")
                .map(PathBuf::from)
                .context("--server-secret is required for serve")?;
            let listen =
                take_option(&mut args, "--listen").unwrap_or_else(|| "127.0.0.1:18096".to_string());
            reject_extra(&args)?;
            let layout = StorageLayout::new(storage_root.clone());
            layout.ensure()?;
            let _serve_lock = layout.acquire_serve_lock()?;
            let store = CloudStore::open(policy.store_config(storage_root.clone()))?;
            run_server(
                store,
                ServerConfig {
                    listen: SocketAddr::from_str(&listen).context("invalid --listen address")?,
                    server_secret_path,
                    policy,
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
            let store = open_store(&storage_root, &policy)?;
            let report = store.run_gc(now_epoch_ms(), grace_seconds * 1_000)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        "backup-now" => {
            reject_extra(&args)?;
            let report = create_backup(&policy.store_config(storage_root.clone()), now_epoch_ms())?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        "backup-if-due" => {
            reject_extra(&args)?;
            let report =
                create_backup_if_due(&policy.store_config(storage_root.clone()), now_epoch_ms())?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        "verify-backup" => {
            let snapshot = PathBuf::from(required_arg(&mut args, "snapshot path")?);
            reject_extra(&args)?;
            let manifest = verify_backup(&snapshot)?;
            println!("{}", serde_json::to_string_pretty(&manifest)?);
            Ok(())
        }
        "restore" => {
            let snapshot = PathBuf::from(required_arg(&mut args, "snapshot path")?);
            reject_extra(&args)?;
            let report = restore_backup(&storage_root, &snapshot, now_epoch_ms())?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        "set-mode" => {
            let mode = parse_read_only_mode(required_arg(&mut args, "mode")?)?;
            reject_extra(&args)?;
            let store = open_store(&storage_root, &policy)?;
            store.set_service_mode(mode, now_epoch_ms())?;
            Ok(())
        }
        "resume-writes" => {
            reject_extra(&args)?;
            let store = open_store(&storage_root, &policy)?;
            let report = store.resume_writes(now_epoch_ms())?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        "force-resume-writes" => {
            let reason = take_option(&mut args, "--reason").context("--reason is required")?;
            reject_extra(&args)?;
            let store = open_store(&storage_root, &policy)?;
            let report = store.force_resume_writes(&reason, now_epoch_ms())?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        "set-account-mode" => {
            let account = required_arg(&mut args, "account")?;
            let mode = parse_mode(required_arg(&mut args, "mode")?)?;
            reject_extra(&args)?;
            let store = open_store(&storage_root, &policy)?;
            store.set_account_mode(&account, mode, now_epoch_ms())?;
            Ok(())
        }
        "set-account-quota" => {
            let account = required_arg(&mut args, "account")?;
            let quota = required_arg(&mut args, "bytes")?
                .parse::<u64>()
                .context("invalid quota bytes")?;
            reject_extra(&args)?;
            let store = open_store(&storage_root, &policy)?;
            store.set_account_quota(&account, quota, now_epoch_ms())?;
            Ok(())
        }
        "delete-account" => {
            let account = required_arg(&mut args, "account")?;
            reject_extra(&args)?;
            let store = open_store(&storage_root, &policy)?;
            store.delete_account(&account)?;
            Ok(())
        }
        _ => bail!(usage()),
    }
}

fn open_store(storage_root: &Path, policy: &AcsRuntimePolicy) -> anyhow::Result<CloudStore> {
    Ok(CloudStore::open(
        policy.store_config(storage_root.to_path_buf()),
    )?)
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

fn parse_read_only_mode(value: String) -> anyhow::Result<AccountMode> {
    match value.as_str() {
        "read-only" => Ok(AccountMode::ReadOnly),
        "suspended" => Ok(AccountMode::Suspended),
        _ => bail!("invalid service mode {value:?}; use resume-writes to return to normal"),
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
        let layout = StorageLayout::new(root.path().to_path_buf());
        let first = layout.acquire_serve_lock().unwrap();
        assert!(layout.acquire_serve_lock().is_err());
        drop(first);
        layout.acquire_serve_lock().unwrap();
    }
}
