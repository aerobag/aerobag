use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("preprocessor-cli crate should live under the workspace root")
        .parent()
        .expect("workspace root should live under the product root")
        .parent()
        .expect("product root should live under the repo root")
        .to_path_buf()
}

fn run_cli(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_preprocessor-cli"))
        .args(args)
        .output()
        .expect("failed to run preprocessor-cli");

    if !output.status.success() {
        panic!(
            "preprocessor-cli failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    String::from_utf8(output.stdout).expect("stdout should be utf-8")
}

fn path_string(path: PathBuf) -> String {
    path.display().to_string()
}

#[test]
fn sec_clean_check_tile_paths_match_legacy() {
    let root = repo_root();
    let legacy = path_string(root.join("runs/20260405T154700Z/work/charts-sec"));
    let rust = path_string(root.join("rust-runs/sec-clean-check/work/charts-sec"));

    let stdout = run_cli(&[
        "compare-chart-tile-paths",
        "--family",
        "sec",
        "--legacy-work-dir",
        &legacy,
        "--rust-work-dir",
        &rust,
    ]);

    assert!(stdout.contains("SEC legacy_tile_paths=35494 rust_tile_paths=35494 status=match"));
}

#[test]
fn sec_clean_check_packages_match_legacy() {
    let root = repo_root();
    let legacy = path_string(root.join("runs/20260405T154700Z/work/charts-sec"));
    let rust = path_string(root.join("rust-runs/sec-clean-check/work/charts-sec"));

    let stdout = run_cli(&[
        "compare-chart-packages",
        "--family",
        "sec",
        "--legacy-work-dir",
        &legacy,
        "--rust-work-dir",
        &rust,
    ]);

    for region in ["AK", "EC", "NC", "NE", "NW", "PAC", "SC", "SE", "SW"] {
        assert!(
            stdout.contains(&format!(
                "{region} manifest_bytes=match manifest_entries=match"
            )),
            "missing package parity line for {region}\n{stdout}"
        );
        assert!(
            stdout.contains(&format!("members=match")),
            "expected members=match output\n{stdout}"
        );
    }
}

#[test]
fn tac_native_fixed_tile_paths_match_legacy() {
    let root = repo_root();
    let legacy = path_string(root.join("runs/20260405T154700Z/work/charts-tac"));
    let rust = path_string(root.join("rust-runs/tac-native-fixed/work/charts-tac"));

    let stdout = run_cli(&[
        "compare-chart-tile-paths",
        "--family",
        "tac",
        "--legacy-work-dir",
        &legacy,
        "--rust-work-dir",
        &rust,
    ]);

    assert!(stdout.contains("TAC legacy_tile_paths=7174 rust_tile_paths=7174 status=match"));
}

#[test]
fn enr_l_port_tile_paths_match_legacy() {
    let root = repo_root();
    let legacy = path_string(root.join("runs/20260405T154700Z/work/charts-enr-l"));
    let rust = path_string(root.join("rust-runs/enr-l-port/work/charts-enr-l"));

    let stdout = run_cli(&[
        "compare-chart-tile-paths",
        "--family",
        "enr-l",
        "--legacy-work-dir",
        &legacy,
        "--rust-work-dir",
        &rust,
    ]);

    assert!(stdout.contains("ENR_L legacy_tile_paths=27428 rust_tile_paths=27428 status=match"));
}

#[test]
fn enr_l_port_packages_match_legacy() {
    let root = repo_root();
    let legacy = path_string(root.join("runs/20260405T154700Z/work/charts-enr-l"));
    let rust = path_string(root.join("rust-runs/enr-l-port/work/charts-enr-l"));

    let stdout = run_cli(&[
        "compare-chart-packages",
        "--family",
        "enr-l",
        "--legacy-work-dir",
        &legacy,
        "--rust-work-dir",
        &rust,
    ]);

    for region in ["AK", "EC", "NC", "NE", "NW", "PAC", "SC", "SE", "SW"] {
        assert!(
            stdout.contains(&format!(
                "{region} manifest_bytes=match manifest_entries=match"
            )),
            "missing package parity line for {region}\n{stdout}"
        );
        assert!(
            stdout.contains("members=match"),
            "expected members=match output\n{stdout}"
        );
    }
}

#[test]
fn legacy_sec_provenance_self_compare_matches() {
    let root = repo_root();
    let provenance = path_string(root.join("runs/20260405T154700Z/meta/provenance/charts-sec"));

    let stdout = run_cli(&[
        "compare-provenance",
        "--left-provenance-dir",
        &provenance,
        "--right-provenance-dir",
        &provenance,
    ]);

    assert!(stdout.contains("source_urls left=55 right=55 status=match"));
    assert!(stdout.contains("downloads left=55 right=55 status=match"));
    assert!(stdout.contains("extracts left=55 right=55 status=match"));
}

#[test]
fn sec_visual_self_compare_sample_matches() {
    let root = repo_root();
    let tiles = path_string(root.join("runs/20260405T154700Z/work/charts-sec/tiles/0"));

    let stdout = run_cli(&[
        "compare-sampled-images",
        "--left-root",
        &tiles,
        "--right-root",
        &tiles,
        "--sample-percent",
        "1",
        "--rmse-threshold",
        "0.0",
        "--limit",
        "10",
    ]);

    assert!(
        stdout.contains("visual status=match"),
        "expected visual match\n{stdout}"
    );
}

#[test]
fn csup_native_dedup_packages_match_legacy_entries() {
    let root = repo_root();
    let legacy = path_string(root.join("runs/20260405T154700Z/work/csup"));
    let rust = path_string(root.join("rust-runs/csup-native-check-globorder/work/csup"));

    let stdout = run_cli(&[
        "compare-csup-packages",
        "--legacy-work-dir",
        &legacy,
        "--rust-work-dir",
        &rust,
    ]);

    for region in ["AK", "EC", "NC", "NE", "NW", "PAC", "SC", "SE", "SW"] {
        assert!(
            stdout.contains(&format!(
                "{region} manifest_bytes=match manifest_entries=match"
            )),
            "missing csup package parity line for {region}\n{stdout}"
        );
    }
    assert!(
        stdout.contains("members=match"),
        "expected members=match output\n{stdout}"
    );
}

#[test]
fn csup_native_dedup_provenance_matches_legacy() {
    let root = repo_root();
    let legacy = path_string(root.join("runs/20260405T154700Z/meta/provenance/csup"));
    let rust = path_string(root.join("rust-runs/csup-native-check-globorder/meta/provenance/csup"));

    let stdout = run_cli(&[
        "compare-provenance",
        "--left-provenance-dir",
        &legacy,
        "--right-provenance-dir",
        &rust,
    ]);

    assert!(stdout.contains("source_urls left=1 right=1 status=match"));
    assert!(stdout.contains("downloads left=1 right=1 status=match"));
    assert!(stdout.contains("extracts left=1 right=1 status=match"));
}

#[test]
fn tpp_ne_native_packages_match_legacy() {
    let root = repo_root();
    let legacy = path_string(root.join("runs/20260405T154700Z-tpp-retry/work/tpp-ne"));
    let rust = path_string(root.join("rust-runs/tpp-ne-native-check/work/tpp-ne"));

    let stdout = run_cli(&[
        "compare-tpp-packages",
        "--region",
        "NE",
        "--legacy-work-dir",
        &legacy,
        "--rust-work-dir",
        &rust,
    ]);

    assert!(
        stdout.contains("NE manifest_bytes=match manifest_entries=match legacy_members=3278 rust_members=3278 members=match"),
        "missing tpp package parity line\n{stdout}"
    );
}
