use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("preprocessor-cli crate should live under the workspace root")
        .parent()
        .expect("workspace root should live under the repo root")
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

fn unique_temp_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("aerobag-data-parity-{nanos}"))
}

#[test]
fn rust_data_builder_matches_legacy_database_dump() {
    let root = repo_root();
    let input_dir = root.join("runs/20260407T053200Z-data-build/work/data");
    let legacy_db = input_dir.join("main.db");
    let output_dir = unique_temp_dir();
    fs::create_dir_all(&output_dir).expect("failed to create temp output dir");

    let output_dir_str = output_dir.display().to_string();
    let input_dir_str = input_dir.display().to_string();
    let legacy_db_str = legacy_db.display().to_string();
    let rust_db_str = output_dir.join("main.db").display().to_string();

    let build_stdout = run_cli(&[
        "build-data",
        "--input-dir",
        &input_dir_str,
        "--output-dir",
        &output_dir_str,
        "--manifest-version",
        "2604",
    ]);
    assert!(
        build_stdout.contains("table saa rows 1234"),
        "expected saa row count in build output\n{build_stdout}"
    );

    let compare_stdout = run_cli(&[
        "compare-data-db",
        "--left-db",
        &legacy_db_str,
        "--right-db",
        &rust_db_str,
    ]);
    assert!(compare_stdout.contains("status match"), "expected data parity match\n{compare_stdout}");
    assert!(
        compare_stdout.contains("table cifp_sid_star_app left=208451 right=208451 status=match"),
        "expected CIFP parity line\n{compare_stdout}"
    );

    fs::remove_dir_all(&output_dir).expect("failed to remove temp output dir");
}
