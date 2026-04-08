use rusqlite::Connection;
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

fn unique_temp_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("aerobag-product-data-{nanos}"))
}

#[test]
fn product_data_builder_canonicalizes_airport_ids() {
    let root = repo_root();
    let input_dir = root.join("runs/20260407T053200Z-data-build/work/data");
    let output_dir = unique_temp_dir();
    fs::create_dir_all(&output_dir).expect("failed to create temp output dir");

    let output_dir_str = output_dir.display().to_string();
    let input_dir_str = input_dir.display().to_string();

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
        build_stdout.contains("table airports rows 19445"),
        "expected airports row count in build output\n{build_stdout}"
    );
    assert!(
        build_stdout.contains("table cifp_sid_star_app rows 208451"),
        "expected cifp row count in build output\n{build_stdout}"
    );

    let db = Connection::open(output_dir.join("main.db")).expect("open product main.db");

    let sea_airport_rows: i64 = db
        .query_row(
            "select count(*) from airports where LocationID = 'SEA'",
            [],
            |row| row.get(0),
        )
        .expect("count SEA airport rows");
    assert_eq!(sea_airport_rows, 0, "airport ids should be canonicalized");

    let ksea_airport_rows: i64 = db
        .query_row(
            "select count(*) from airports where LocationID = 'KSEA'",
            [],
            |row| row.get(0),
        )
        .expect("count KSEA airport rows");
    assert_eq!(ksea_airport_rows, 1, "expected canonical airport id");

    let linked_tables = [
        ("airportfreq", "LocationID"),
        ("airportrunways", "LocationID"),
        ("awos", "LocationID"),
        ("cifp_sid_star_app", "airport_identifier"),
    ];
    for (table, column) in linked_tables {
        let query = format!("select count(*) from {table} where {column} = 'KSEA'");
        let count: i64 = db
            .query_row(&query, [], |row| row.get(0))
            .unwrap_or_else(|_| panic!("count canonical ids in {table}.{column}"));
        assert!(
            count > 0,
            "expected canonical airport id in {table}.{column}"
        );
    }

    fs::remove_dir_all(&output_dir).expect("failed to remove temp output dir");
}
