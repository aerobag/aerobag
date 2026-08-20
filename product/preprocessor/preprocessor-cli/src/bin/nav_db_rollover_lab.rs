use anyhow::{bail, Context};
use chrono::{DateTime, Duration, Utc};
use preprocessor_core::nav_kv::{
    build_nav_kv_sorted, NavKvLookup, NavKvPair, NavKvRoot, NavKvStore,
};
use product_contracts::publication::{
    bundle::v2::{BundleManifest, BundlePackageArtifact, SCHEMA_VERSION as BUNDLE_SCHEMA_VERSION},
    current::v1::{
        CurrentArtifactRoots, CurrentArtifactsManifest, CurrentBundleEntry,
        SCHEMA_VERSION as CURRENT_SCHEMA_VERSION,
    },
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fs,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

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
    transition: Transition,
    scenario: Scenario,
}

#[derive(Debug, Clone, Copy)]
enum Transition {
    At(DateTime<Utc>),
    After(Duration),
}

impl Transition {
    fn resolve(self) -> DateTime<Utc> {
        match self {
            Self::At(value) => value,
            Self::After(delay) => Utc::now() + delay,
        }
    }
}

fn main() -> anyhow::Result<()> {
    let args = parse_args()?;
    generate_lab_publication(&args)
}

fn parse_args() -> anyhow::Result<Args> {
    let mut fixture_root = None;
    let mut output_root = None;
    let mut transition_at = None;
    let mut transition_delay = None;
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
                transition_at = Some(
                    DateTime::parse_from_rfc3339(&value)
                        .with_context(|| format!("invalid --transition-at {value}"))?
                        .with_timezone(&Utc),
                );
            }
            "--transition-delay-seconds" => {
                let seconds = value
                    .parse::<i64>()
                    .with_context(|| format!("invalid --transition-delay-seconds {value}"))?;
                if seconds <= 0 {
                    bail!("--transition-delay-seconds must be positive");
                }
                transition_delay = Some(Duration::seconds(seconds));
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
    let transition = match (transition_at, transition_delay) {
        (Some(value), None) => Transition::At(value),
        (None, Some(delay)) => Transition::After(delay),
        (Some(_), Some(_)) => {
            bail!("use either --transition-at or --transition-delay-seconds, not both")
        }
        (None, None) => {
            bail!("missing --transition-at or --transition-delay-seconds")
        }
    };
    Ok(Args {
        fixture_root: fixture_root.context("missing --fixture-root")?,
        output_root: output_root.context("missing --output-root")?,
        transition,
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
    if cycles.len() != 2 {
        bail!(
            "NAVDB rollover fixture must contain exactly two ordered cycles; found {}",
            cycles.len()
        );
    }
    let initial_cycle = &cycles[0];
    let candidate_cycle = &cycles[1];
    let initial_cycle_id = required_str(initial_cycle, "cycle")?;
    let candidate_cycle_id = required_str(candidate_cycle, "cycle")?;
    let nav_db_contract = required_str(initial_cycle, "contract_id")?;
    let candidate_contract = required_str(candidate_cycle, "contract_id")?;
    if candidate_contract != nav_db_contract {
        bail!(
            "NAVDB rollover fixture contracts differ: cycle {initial_cycle_id} is \
             {nav_db_contract}, cycle {candidate_cycle_id} is {candidate_contract}"
        );
    }
    if nav_db_contract != product_contracts::NAV_DB_CONTRACT_ID {
        bail!(
            "NAVDB rollover fixture provides {nav_db_contract}; client requires {}",
            product_contracts::NAV_DB_CONTRACT_ID
        );
    }
    verify_fixture_artifact(&args.fixture_root, initial_cycle, "bundle")?;
    verify_fixture_artifact(&args.fixture_root, initial_cycle, "nav_db")?;
    verify_fixture_artifact(&args.fixture_root, candidate_cycle, "bundle")?;
    verify_fixture_artifact(&args.fixture_root, candidate_cycle, "nav_db")?;

    let materialized_initial = materialize_cycle(args, initial_cycle, false, &unpacked_root)?;
    let materialized_candidate = materialize_cycle(
        args,
        candidate_cycle,
        args.scenario == Scenario::Reject,
        &unpacked_root,
    )?;

    let transition = args.transition.resolve();
    let initial_effective = transition - Duration::days(28);
    let candidate_expiration = transition + Duration::days(28);
    let prepared_initial = prepare_cycle(
        &materialized_initial,
        initial_effective,
        transition,
        &packaged_root,
    )?;
    let prepared_candidate = prepare_cycle(
        &materialized_candidate,
        transition,
        candidate_expiration,
        &packaged_root,
    )?;

    let as_of = Utc::now();
    let current_artifacts = [CurrentArtifactsManifest {
        schema_version: CURRENT_SCHEMA_VERSION,
        contracts: BTreeMap::from([("nav-db".to_string(), nav_db_contract.to_string())]),
        artifact_roots: CurrentArtifactRoots {
            packaged: "packaged".to_string(),
            unpacked: "unpacked".to_string(),
        },
        as_of_date: as_of.format("%Y-%m-%d").to_string(),
        as_of_utc: rfc3339(as_of),
        bundles: vec![prepared_initial.bundle_ref, prepared_candidate.bundle_ref],
        startup_prefetch: None,
        diagnostics: None,
    }];
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
            "transition_at": rfc3339(transition),
            "initial": prepared_initial.summary,
            "candidate": prepared_candidate.summary,
            "removed_nav_key": (args.scenario == Scenario::Reject).then_some(REJECTED_NAV_KEY),
        }),
    )?;
    println!("{}", args.output_root.display());
    Ok(())
}

struct PreparedCycle {
    bundle_ref: CurrentBundleEntry,
    summary: Value,
}

struct MaterializedCycle {
    cycle: String,
    package: Value,
    package_id: String,
    filename: String,
}

fn materialize_cycle(
    args: &Args,
    fixture_cycle: &Value,
    remove_required_nav_key: bool,
    unpacked_root: &Path,
) -> anyhow::Result<MaterializedCycle> {
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
    let package_dir_name = filename
        .strip_suffix(".zip")
        .context("generated nav-db filename must end in .zip")?;
    let package_dir = unpacked_root.join(package_dir_name);
    if remove_required_nav_key {
        rebuild_without_key(&source_nav_db, &package_dir, REJECTED_NAV_KEY)?;
    } else {
        extract_nav_db_package(&source_nav_db, &package_dir)?;
    }
    let packaged_path = args.output_root.join("packaged").join(&filename);
    if remove_required_nav_key {
        write_nav_db_package(&package_dir, &packaged_path)?;
    } else {
        fs::copy(&source_nav_db, &packaged_path).with_context(|| {
            format!(
                "copy fixture package {} to {}",
                source_nav_db.display(),
                packaged_path.display()
            )
        })?;
    }
    let packaged_bytes = fs::read(&packaged_path)
        .with_context(|| format!("read generated package {}", packaged_path.display()))?;
    package["checksum_sha256"] = json!(hex_sha256(&packaged_bytes));
    package["size_bytes"] = json!(packaged_bytes.len());

    Ok(MaterializedCycle {
        cycle: cycle.to_string(),
        package,
        package_id,
        filename,
    })
}

fn prepare_cycle(
    materialized: &MaterializedCycle,
    effective: DateTime<Utc>,
    expiration: DateTime<Utc>,
    packaged_root: &Path,
) -> anyhow::Result<PreparedCycle> {
    let cycle = &materialized.cycle;
    let mut package = materialized.package.clone();
    package["effective_date"] = json!(rfc3339(effective));
    package["expiration_date"] = json!(rfc3339(expiration));

    let bundle_filename = format!("bundle_cycle_{cycle}_nav_db_rollover_lab.json");
    let effective_text = rfc3339(effective);
    let expiration_text = rfc3339(expiration);
    let bundle_id = format!("cycle_{cycle}_nav_db_rollover_lab");
    let package = serde_json::from_value::<BundlePackageArtifact>(package)
        .with_context(|| format!("decode generated cycle {cycle} package"))?;
    let bundle = BundleManifest {
        schema_version: BUNDLE_SCHEMA_VERSION,
        bundle_id: bundle_id.clone(),
        bundle_type: "cycle".to_string(),
        cycle: cycle.to_string(),
        cycle_version: "01".to_string(),
        generated_at_utc: rfc3339(Utc::now()),
        effective_date: effective_text.clone(),
        expiration_date: expiration_text.clone(),
        start_valid: effective_text.clone(),
        end_valid: expiration_text.clone(),
        packages: vec![package],
        ancillary: Vec::new(),
    };
    let bundle_bytes = serde_json::to_vec_pretty(&bundle)?;
    fs::write(packaged_root.join(&bundle_filename), &bundle_bytes)?;
    let bundle_sha = hex_sha256(&bundle_bytes);
    Ok(PreparedCycle {
        bundle_ref: CurrentBundleEntry {
            filename: bundle_filename.clone(),
            relative_path: bundle_filename,
            id: bundle_id,
            bundle_type: "cycle".to_string(),
            cycle: cycle.to_string(),
            cycle_version: "01".to_string(),
            start_valid: effective_text.clone(),
            end_valid: expiration_text.clone(),
            checksum_sha256: bundle_sha,
            size_bytes: bundle_bytes.len() as u64,
        },
        summary: json!({
            "cycle": cycle,
            "package_id": materialized.package_id,
            "filename": materialized.filename,
            "effective_at": effective_text,
            "expiration_at": expiration_text,
        }),
    })
}

fn write_nav_db_package(source_dir: &Path, output_path: &Path) -> anyhow::Result<()> {
    let mut entries = fs::read_dir(source_dir)?
        .map(|entry| entry.map(|value| value.path()))
        .collect::<Result<Vec<_>, _>>()?;
    entries.retain(|path| path.is_file());
    entries.sort();

    let file = File::create(output_path)
        .with_context(|| format!("create generated package {}", output_path.display()))?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for path in entries {
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .context("generated nav-db member name is not UTF-8")?;
        archive.start_file(name, options)?;
        let mut input = File::open(&path)?;
        std::io::copy(&mut input, &mut archive)?;
    }
    archive.finish()?;
    Ok(())
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

fn write_json(path: &Path, value: &impl serde::Serialize) -> anyhow::Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_bundle_uses_the_canonical_versioned_contract() {
        let temp = tempfile::tempdir().unwrap();
        let materialized = MaterializedCycle {
            cycle: "2607".to_string(),
            package: json!({
                "id": "nav-db-cycle-2607",
                "family_id": "nav-db",
                "contract_id": product_contracts::NAV_DB_CONTRACT_ID,
                "filename": "nav-db-cycle-2607.zip",
                "relative_path": "nav-db-cycle-2607.zip",
                "cycle": "2607",
                "cycle_version": "01",
                "checksum_sha256": "fixture-checksum",
                "size_bytes": 42,
                "effective_date": null,
                "expiration_date": null
            }),
            package_id: "nav-db-cycle-2607".to_string(),
            filename: "nav-db-cycle-2607.zip".to_string(),
        };
        let effective = Utc::now();
        let prepared = prepare_cycle(
            &materialized,
            effective,
            effective + Duration::days(28),
            temp.path(),
        )
        .unwrap();
        let bundle_bytes = fs::read(temp.path().join(prepared.bundle_ref.filename)).unwrap();
        let bundle = product_contracts::versioned_json::decode_exact::<BundleManifest>(
            "generated rollover bundle",
            &bundle_bytes,
            BUNDLE_SCHEMA_VERSION,
        )
        .unwrap();

        assert_eq!(bundle.schema_version, BUNDLE_SCHEMA_VERSION);
        assert_eq!(bundle.packages.len(), 1);
        assert_eq!(
            bundle.packages[0].contract_id,
            product_contracts::NAV_DB_CONTRACT_ID
        );
    }
}
