use anyhow::{bail, Context};
use chrono::{DateTime, Duration, Utc};
use preprocessor_core::nav_kv::{
    build_nav_kv_sorted, NavKvLookup, NavKvPair, NavKvRoot, NavKvStore,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zip::ZipArchive;

const REJECTED_NAV_KEY: &str = "navref/position/navaid/SEA";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scenario {
    Success,
    Reject,
}

#[derive(Debug)]
struct Args {
    fixture_root: PathBuf,
    output_root: PathBuf,
    transition: DateTime<Utc>,
    scenario: Scenario,
}

fn main() -> anyhow::Result<()> {
    let args = parse_args()?;
    generate_lab_publication(&args)
}

fn parse_args() -> anyhow::Result<Args> {
    let mut fixture_root = None;
    let mut output_root = None;
    let mut transition = None;
    let mut scenario = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args
            .next()
            .with_context(|| format!("missing value after {arg}"))?;
        match arg.as_str() {
            "--fixture-root" => fixture_root = Some(PathBuf::from(value)),
            "--output-root" => output_root = Some(PathBuf::from(value)),
            "--transition-at" => {
                transition = Some(
                    DateTime::parse_from_rfc3339(&value)
                        .with_context(|| format!("invalid --transition-at {value}"))?
                        .with_timezone(&Utc),
                );
            }
            "--scenario" => {
                scenario = Some(match value.as_str() {
                    "success" => Scenario::Success,
                    "reject" => Scenario::Reject,
                    _ => bail!("invalid --scenario {value}; expected success or reject"),
                });
            }
            _ => bail!("unknown argument {arg}"),
        }
    }
    Ok(Args {
        fixture_root: fixture_root.context("missing --fixture-root")?,
        output_root: output_root.context("missing --output-root")?,
        transition: transition.context("missing --transition-at")?,
        scenario: scenario.context("missing --scenario")?,
    })
}

fn generate_lab_publication(args: &Args) -> anyhow::Result<()> {
    if args.output_root.exists() {
        fs::remove_dir_all(&args.output_root)
            .with_context(|| format!("remove {}", args.output_root.display()))?;
    }
    let packaged_root = args.output_root.join("packaged");
    let unpacked_root = args.output_root.join("unpacked");
    fs::create_dir_all(&packaged_root)?;
    fs::create_dir_all(&unpacked_root)?;

    let fixture: Value = read_json(&args.fixture_root.join("fixture.json"))?;
    let cycles = fixture["cycles"]
        .as_array()
        .context("fixture.json cycles must be an array")?;
    let cycle_2607 = fixture_cycle(cycles, "2607")?;
    let cycle_2608 = fixture_cycle(cycles, "2608")?;
    verify_fixture_artifact(&args.fixture_root, cycle_2607, "bundle")?;
    verify_fixture_artifact(&args.fixture_root, cycle_2607, "nav_db")?;
    verify_fixture_artifact(&args.fixture_root, cycle_2608, "bundle")?;
    verify_fixture_artifact(&args.fixture_root, cycle_2608, "nav_db")?;

    let effective_2607 = args.transition - Duration::days(28);
    let effective_2608 = args.transition;
    let expiration_2608 = args.transition + Duration::days(28);
    let prepared_2607 = prepare_cycle(
        args,
        cycle_2607,
        effective_2607,
        effective_2608,
        false,
        &packaged_root,
        &unpacked_root,
    )?;
    let prepared_2608 = prepare_cycle(
        args,
        cycle_2608,
        effective_2608,
        expiration_2608,
        args.scenario == Scenario::Reject,
        &packaged_root,
        &unpacked_root,
    )?;

    let as_of = Utc::now();
    let current_artifacts = json!([{
        "schema_version": 1,
        "contracts": {"nav-db": "NAV12"},
        "artifact_roots": {
            "packaged": "packaged",
            "unpacked": "unpacked"
        },
        "as_of_date": as_of.format("%Y-%m-%d").to_string(),
        "as_of_utc": rfc3339(as_of),
        "bundles": [
            prepared_2607.bundle_ref,
            prepared_2608.bundle_ref
        ]
    }]);
    write_json(
        &args.output_root.join("current_artifacts.json"),
        &current_artifacts,
    )?;
    write_json(
        &args.output_root.join("lab.json"),
        &json!({
            "schema_version": 1,
            "scenario": match args.scenario {
                Scenario::Success => "success",
                Scenario::Reject => "reject",
            },
            "transition_at": rfc3339(args.transition),
            "initial": prepared_2607.summary,
            "candidate": prepared_2608.summary,
            "removed_nav_key": (args.scenario == Scenario::Reject).then_some(REJECTED_NAV_KEY),
        }),
    )?;
    println!("{}", args.output_root.display());
    Ok(())
}

struct PreparedCycle {
    bundle_ref: Value,
    summary: Value,
}

#[allow(clippy::too_many_arguments)]
fn prepare_cycle(
    args: &Args,
    fixture_cycle: &Value,
    effective: DateTime<Utc>,
    expiration: DateTime<Utc>,
    remove_required_nav_key: bool,
    packaged_root: &Path,
    unpacked_root: &Path,
) -> anyhow::Result<PreparedCycle> {
    let cycle = required_str(fixture_cycle, "cycle")?;
    let source_bundle = fixture_artifact_path(&args.fixture_root, fixture_cycle, "bundle")?;
    let source_nav_db = fixture_artifact_path(&args.fixture_root, fixture_cycle, "nav_db")?;
    let source_bundle_json = read_json(&source_bundle)?;
    let source_package = source_bundle_json["packages"]
        .as_array()
        .context("source bundle packages must be an array")?
        .iter()
        .find(|package| package["family_id"].as_str() == Some("nav-db"))
        .with_context(|| format!("source cycle {cycle} has no nav-db package"))?;
    let mut package = source_package.clone();

    let source_filename = required_str(source_package, "filename")?;
    let filename = if remove_required_nav_key {
        format!(
            "{}_missing_sea.zip",
            source_filename
                .strip_suffix(".zip")
                .unwrap_or(source_filename)
        )
    } else {
        source_filename.to_string()
    };
    let package_id = if remove_required_nav_key {
        format!("{}_MISSING_SEA", required_str(source_package, "id")?)
    } else {
        required_str(source_package, "id")?.to_string()
    };
    package["id"] = json!(package_id);
    package["filename"] = json!(filename);
    package["relative_path"] = json!(filename);
    package["effective_date"] = json!(rfc3339(effective));
    package["expiration_date"] = json!(rfc3339(expiration));
    package["checksum_sha256"] = Value::Null;
    if remove_required_nav_key {
        package["size_bytes"] = Value::Null;
    }

    let package_dir_name = filename
        .strip_suffix(".zip")
        .context("generated nav-db filename must end in .zip")?;
    let package_dir = unpacked_root.join(package_dir_name);
    if remove_required_nav_key {
        rebuild_without_key(&source_nav_db, &package_dir, REJECTED_NAV_KEY)?;
    } else {
        extract_nav_db_package(&source_nav_db, &package_dir)?;
    }

    let bundle_filename = format!("bundle_cycle_{cycle}_nav_db_rollover_lab.json");
    let bundle = json!({"packages": [package]});
    let bundle_bytes = serde_json::to_vec_pretty(&bundle)?;
    fs::write(packaged_root.join(&bundle_filename), &bundle_bytes)?;
    let bundle_sha = hex_sha256(&bundle_bytes);
    let effective_text = rfc3339(effective);
    let expiration_text = rfc3339(expiration);
    Ok(PreparedCycle {
        bundle_ref: json!({
            "filename": bundle_filename,
            "relative_path": bundle_filename,
            "id": format!("cycle_{cycle}_nav_db_rollover_lab"),
            "bundle_type": "cycle",
            "cycle": cycle,
            "cycle_version": "01",
            "start_valid": effective_text,
            "end_valid": expiration_text,
            "checksum_sha256": bundle_sha,
            "size_bytes": bundle_bytes.len(),
        }),
        summary: json!({
            "cycle": cycle,
            "package_id": package_id,
            "filename": filename,
            "effective_at": effective_text,
            "expiration_at": expiration_text,
        }),
    })
}

fn extract_nav_db_package(source_zip: &Path, output_dir: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(output_dir)?;
    let file = File::open(source_zip)
        .with_context(|| format!("open source nav-db {}", source_zip.display()))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("read source nav-db {}", source_zip.display()))?;
    for index in 0..archive.len() {
        let mut member = archive.by_index(index)?;
        let name = member.name().to_string();
        if name != "manifest.json" && name != "root" && !name.starts_with("page_") {
            continue;
        }
        if name.contains('/') || name.contains('\\') {
            bail!("unexpected nested nav-db member {name}");
        }
        let mut bytes = Vec::new();
        member.read_to_end(&mut bytes)?;
        fs::write(output_dir.join(name), bytes)?;
    }
    Ok(())
}

fn rebuild_without_key(
    source_zip: &Path,
    output_dir: &Path,
    removed_key: &str,
) -> anyhow::Result<()> {
    let source_bytes =
        fs::read(source_zip).with_context(|| format!("read {}", source_zip.display()))?;
    let package = nav_kv_package::read_package_bytes("nav-db rollover fixture", &source_bytes)
        .map_err(anyhow::Error::msg)?;
    let root = NavKvRoot::parse(&package.root).map_err(anyhow::Error::msg)?;
    let page_size = root.page_size();
    let expected_key_count = root.len();
    let mut store = NavKvStore::new(root);
    for (index, page) in package.pages.into_iter().enumerate() {
        store.insert_page(u32::try_from(index)?, page);
    }
    let keys = store.keys_with_prefix("");
    if keys.len() != expected_key_count {
        bail!(
            "full nav-db scan returned {} of {expected_key_count} keys",
            keys.len()
        );
    }
    let mut removed = false;
    let mut pairs = Vec::with_capacity(keys.len().saturating_sub(1));
    for key in keys {
        let value = match store.get_bytes(&key).map_err(anyhow::Error::msg)? {
            NavKvLookup::Hit(value) => value,
            NavKvLookup::MissingKey => bail!("key disappeared during full nav-db scan: {key}"),
            NavKvLookup::MissingPages(pages) => {
                bail!("full nav-db scan unexpectedly needs pages {pages:?} for {key}")
            }
        };
        if key == removed_key {
            removed = true;
        } else {
            pairs.push(NavKvPair { key, value });
        }
    }
    if !removed {
        bail!("fixture nav-db does not contain required rejection key {removed_key}");
    }
    let rebuilt = build_nav_kv_sorted(pairs, page_size).map_err(anyhow::Error::msg)?;
    fs::create_dir_all(output_dir)?;
    fs::write(output_dir.join("root"), &rebuilt.root_bytes)?;
    for (index, page) in rebuilt.pages.iter().enumerate() {
        fs::write(output_dir.join(format!("page_{index:04}")), page)?;
    }
    let mut manifest: Value = serde_json::from_slice(&package.manifest)?;
    manifest["logical_bytes_len"] = json!(rebuilt.logical_bytes_len);
    manifest["page_count"] = json!(rebuilt.pages.len());
    manifest["page_size"] = json!(rebuilt.page_size);
    manifest["value_bytes_len"] = json!(rebuilt.value_bytes_len);
    write_json(&output_dir.join("manifest.json"), &manifest)?;
    Ok(())
}

fn fixture_cycle<'a>(cycles: &'a [Value], cycle: &str) -> anyhow::Result<&'a Value> {
    cycles
        .iter()
        .find(|entry| entry["cycle"].as_str() == Some(cycle))
        .with_context(|| format!("fixture has no cycle {cycle}"))
}

fn verify_fixture_artifact(
    fixture_root: &Path,
    cycle: &Value,
    artifact_name: &str,
) -> anyhow::Result<()> {
    let path = fixture_artifact_path(fixture_root, cycle, artifact_name)?;
    let expected = required_str(&cycle[artifact_name], "sha256")?;
    let actual = hex_sha256(&fs::read(&path)?);
    if actual != expected {
        bail!(
            "fixture checksum mismatch for {}: expected {expected}, got {actual}",
            path.display()
        );
    }
    Ok(())
}

fn fixture_artifact_path(
    fixture_root: &Path,
    cycle: &Value,
    artifact_name: &str,
) -> anyhow::Result<PathBuf> {
    let filename = required_str(&cycle[artifact_name], "filename")?;
    let relative = filename.strip_prefix("source/").unwrap_or(filename);
    Ok(fixture_root.join("source").join(relative))
}

fn required_str<'a>(value: &'a Value, field: &str) -> anyhow::Result<&'a str> {
    value[field]
        .as_str()
        .with_context(|| format!("missing string field {field}"))
}

fn read_json(path: &Path) -> anyhow::Result<Value> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("read {}", path.display()))?)
        .with_context(|| format!("parse {}", path.display()))
}

fn write_json(path: &Path, value: &Value) -> anyhow::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    let mut file = File::create(path).with_context(|| format!("create {}", path.display()))?;
    file.write_all(&bytes)?;
    Ok(())
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn rfc3339(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
