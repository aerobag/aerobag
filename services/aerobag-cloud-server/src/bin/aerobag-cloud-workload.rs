// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{env, fs, path::PathBuf};

use aerobag_cloud_server::{
    run_workload, validate_workload_report, AcsRuntimePolicy, WorkloadProfile,
};
use anyhow::{bail, Context as _};

fn usage() -> &'static str {
    "usage: aerobag-cloud-workload [--profile <ci|production>] [--policy <path>] [--output <path>]"
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    let profile = take_option(&mut args, "--profile")
        .as_deref()
        .unwrap_or("ci")
        .parse::<WorkloadProfile>()?;
    let policy_path = take_option(&mut args, "--policy")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../deploy/aerobag-cloud-policy.json")
        });
    let output = take_option(&mut args, "--output").map(PathBuf::from);
    if !args.is_empty() {
        bail!("unexpected arguments: {}\n{}", args.join(" "), usage());
    }

    let policy = AcsRuntimePolicy::load(&policy_path)?;
    let report = run_workload(profile, policy).await?;
    let validation = validate_workload_report(&report);
    let json = serde_json::to_string_pretty(&report)? + "\n";
    if let Some(output) = output {
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("create workload report directory {}", parent.display())
            })?;
        }
        fs::write(&output, &json)
            .with_context(|| format!("write workload report {}", output.display()))?;
        println!(
            "ACS {:?} workload passed in {} ms; report: {}",
            profile,
            report.total_elapsed_ms,
            output.display()
        );
    } else {
        print!("{json}");
    }
    validation
}

fn take_option(args: &mut Vec<String>, name: &str) -> Option<String> {
    let index = args.iter().position(|arg| arg == name)?;
    args.remove(index);
    if index >= args.len() {
        eprintln!("{name} requires a value\n{}", usage());
        std::process::exit(2);
    }
    Some(args.remove(index))
}
