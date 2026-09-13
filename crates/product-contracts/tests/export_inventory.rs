// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::process::Command;

fn exporter() -> Command {
    Command::new(env!("CARGO_BIN_EXE_export-contract-inventory"))
}

#[test]
fn default_export_retains_the_legacy_schema_and_exact_shape() {
    let output = exporter().output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        include_str!("../contracts/client-data-contracts.json")
    );
}

#[test]
fn separate_compatibility_export_matches_compiled_inventory() {
    let output = exporter()
        .arg("--live-feed-compatibility")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        include_str!("../contracts/live-feed-compatibility.json")
    );
}

#[test]
fn build_inventory_directory_contains_both_exports() {
    let directory =
        std::env::temp_dir().join(format!("aerobag-contract-export-{}", std::process::id()));
    let output = exporter()
        .arg("--output-dir")
        .arg(&directory)
        .output()
        .unwrap();
    assert!(output.status.success());
    for (name, expected) in [
        (
            "client-data-contracts.json",
            include_str!("../contracts/client-data-contracts.json"),
        ),
        (
            "live-feed-compatibility.json",
            include_str!("../contracts/live-feed-compatibility.json"),
        ),
    ] {
        assert_eq!(
            std::fs::read_to_string(directory.join(name)).unwrap(),
            expected
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn invalid_exporter_arguments_fail_without_output() {
    for args in [
        vec!["--unknown"],
        vec!["--output-dir"],
        vec!["--live-feed-compatibility", "extra"],
    ] {
        let output = exporter().args(args).output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}
