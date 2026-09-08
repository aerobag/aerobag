use anyhow::{bail, Context};
use chrono::{DateTime, Duration, Utc};
use nav_db_fixture::{Generation, REJECTED_NAV_KEY};
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
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scenario {
    Success,
    Reject,
}

#[derive(Debug)]
struct Args {
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

    let nav_db_contract = product_contracts::NAV_DB_CONTRACT_ID;
    let materialized_initial = materialize_cycle(args, Generation::Initial, &unpacked_root)?;
    let materialized_candidate = materialize_cycle(
        args,
        if args.scenario == Scenario::Reject {
            Generation::Rejected
        } else {
            Generation::Candidate
        },
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
            "source_sha256": hex_sha256(nav_db_fixture::SOURCE.as_bytes()),
            "changed_airport": {"airport_id": "KRNT", "initial_name": nav_db_fixture::INITIAL_AIRPORT_NAME, "candidate_name": nav_db_fixture::CANDIDATE_AIRPORT_NAME},
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
    generation: Generation,
    unpacked_root: &Path,
) -> anyhow::Result<MaterializedCycle> {
    let built = nav_db_fixture::build(generation).map_err(anyhow::Error::msg)?;
    let zip_bytes = nav_kv_package::write_stored_xz_package_bytes_with_encoder(
        &built.manifest,
        &built.root,
        &built.pages,
        |page| {
            preprocessor_core::xz_compress_bytes_with_system_xz(page)
                .map_err(|error| error.to_string())
        },
    )
    .map_err(anyhow::Error::msg)?;
    let checksum = hex_sha256(&zip_bytes);
    let package_id = generation.package_id();
    let filename = format!("{package_id}_{checksum}.zip");
    let package_dir = unpacked_root.join(filename.strip_suffix(".zip").expect("zip suffix"));
    fs::create_dir(&package_dir)?;
    fs::write(package_dir.join("manifest.json"), &built.manifest)?;
    fs::write(package_dir.join("root"), &built.root)?;
    for (index, page) in built.pages.iter().enumerate() {
        fs::write(
            package_dir.join(format!("page_{index:04}")),
            preprocessor_core::xz_compress_bytes_with_system_xz(page)?,
        )?;
    }
    fs::write(
        args.output_root.join("packaged").join(&filename),
        &zip_bytes,
    )?;
    Ok(MaterializedCycle {
        cycle: generation.cycle().to_string(),
        package: json!({
            "id": package_id, "family_id": "nav-db",
            "contract_id": product_contracts::NAV_DB_CONTRACT_ID,
            "filename": filename, "relative_path": filename,
            "cycle": generation.cycle(), "cycle_version": "01",
            "checksum_sha256": checksum, "size_bytes": zip_bytes.len(),
            "effective_date": null, "expiration_date": null,
        }),
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

fn write_json(path: &Path, value: &impl serde::Serialize) -> anyhow::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, &bytes).with_context(|| format!("write {}", path.display()))?;
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
    fn both_scenarios_work_across_calendar_rollovers_without_changing_navdb_bytes() {
        for scenario in [Scenario::Success, Scenario::Reject] {
            let mut first_package_names = None;
            for date in [
                "2026-10-29T09:01:00Z",
                "2036-02-29T09:01:00Z",
                "2099-12-31T09:01:00Z",
            ] {
                let temp = tempfile::tempdir().unwrap();
                let output_root = temp.path().join("publication");
                let transition = DateTime::parse_from_rfc3339(date)
                    .unwrap()
                    .with_timezone(&Utc);
                generate_lab_publication(&Args {
                    output_root: output_root.clone(),
                    transition: Transition::At(transition),
                    scenario,
                })
                .unwrap();
                let current: Vec<CurrentArtifactsManifest> = serde_json::from_slice(
                    &fs::read(output_root.join("current_artifacts.json")).unwrap(),
                )
                .unwrap();
                let bundles = &current[0].bundles;
                assert_eq!(bundles.len(), 2);
                assert_eq!(bundles[0].cycle, nav_db_fixture::INITIAL_CYCLE);
                assert_eq!(bundles[1].cycle, nav_db_fixture::CANDIDATE_CYCLE);
                assert_eq!(
                    bundles[0].start_valid,
                    rfc3339(transition - Duration::days(28))
                );
                assert_eq!(bundles[0].end_valid, rfc3339(transition));
                assert_eq!(bundles[1].start_valid, rfc3339(transition));
                assert_eq!(
                    bundles[1].end_valid,
                    rfc3339(transition + Duration::days(28))
                );
                let mut package_names = Vec::new();
                for entry in bundles {
                    let bytes =
                        fs::read(output_root.join("packaged").join(&entry.filename)).unwrap();
                    assert_eq!(entry.checksum_sha256, hex_sha256(&bytes));
                    let bundle: BundleManifest = serde_json::from_slice(&bytes).unwrap();
                    assert_eq!(bundle.packages.len(), 1);
                    let package = &bundle.packages[0];
                    assert_eq!(package.contract_id, product_contracts::NAV_DB_CONTRACT_ID);
                    assert_eq!(
                        package.effective_date.as_deref(),
                        Some(entry.start_valid.as_str())
                    );
                    assert_eq!(
                        package.expiration_date.as_deref(),
                        Some(entry.end_valid.as_str())
                    );
                    let zip =
                        fs::read(output_root.join("packaged").join(&package.filename)).unwrap();
                    assert_eq!(package.checksum_sha256, hex_sha256(&zip));
                    package_names.push(package.filename.clone());
                }
                if let Some(first) = &first_package_names {
                    assert_eq!(first, &package_names, "calendar changed encoded payloads");
                } else {
                    first_package_names = Some(package_names);
                }
            }
        }
    }

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
