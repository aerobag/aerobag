// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::Path;
use std::process::Command;

#[test]
fn promotion_with_same_contract_sunset_uses_real_controller_and_merger() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let output = Command::new("python3")
        .arg(root.join("tools/ci/promotion_merge_smoke.py"))
        .arg(env!("CARGO_BIN_EXE_preprocessor-cli"))
        .output()
        .expect("run fixture-free promotion integration test");
    assert!(
        output.status.success(),
        "promotion integration failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
