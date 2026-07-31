// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

pub(super) fn content_product_version_label(source_fingerprint: &str) -> String {
    source_fingerprint.chars().take(16).collect()
}

pub fn publish_discovery_manifest(
    config: &ProductBuildConfig,
    as_of_utc: DateTime<Utc>,
    bundle_filenames: &[String],
) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(&config.packaged_dir)
        .with_context(|| format!("failed to create {}", config.packaged_dir.display()))?;
    if bundle_filenames.is_empty() {
        bail!("publish-discovery-manifest requires at least one --bundle");
    }
    let bundles = bundle_filenames
        .iter()
        .map(|filename| current_bundle_entry_from_path(&config.packaged_dir.join(filename)))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let artifact_roots = current_artifact_roots_for_packaged_root(&config.packaged_dir)?;
    let startup_prefetch =
        current_startup_prefetch_manifest(&config.packaged_dir, &artifact_roots, &bundles)?;
    let contracts = current_artifacts_contracts(&config.packaged_dir, &bundles)?;
    let manifest = CurrentArtifactsManifest {
        schema_version: 1,
        contracts,
        artifact_roots,
        as_of_date: as_of_utc.date_naive().format("%Y-%m-%d").to_string(),
        as_of_utc: as_of_utc.to_rfc3339_opts(SecondsFormat::Secs, true),
        bundles,
        startup_prefetch,
        diagnostics: None,
    };
    let product_artifacts_path =
        write_product_artifacts_manifest_json(&config.publish_dir, &manifest)?;
    let unpacked_root = published_unpacked_root(config)?;
    fs::create_dir_all(&unpacked_root)
        .with_context(|| format!("failed to create {}", unpacked_root.display()))?;
    sync_unpacked_discovery_manifests(
        &config.packaged_dir,
        &product_artifacts_path,
        &unpacked_root,
    )?;
    cleanup_published_packaged_root(&config.packaged_dir, &product_artifacts_path)?;
    cleanup_published_unpacked_root(&unpacked_root, &product_artifacts_path)?;
    validate_packaged_contract(&config.packaged_dir, &product_artifacts_path)?;
    validate_unpacked_contract(
        &config.packaged_dir,
        &unpacked_root,
        &product_artifacts_path,
    )?;
    Ok(product_artifacts_path)
}

pub(super) fn publish_content_addressed_zip(
    build_root: &Path,
    zip_path: &Path,
    file_prefix: &str,
    known_sha256: Option<&str>,
    known_size_bytes: Option<u64>,
) -> anyhow::Result<(PathBuf, String, u64)> {
    let sha256 = match known_sha256 {
        Some(value) => value.to_string(),
        None => hash_file(zip_path)?,
    };
    let size_bytes = match known_size_bytes {
        Some(value) => value,
        None => fs::metadata(zip_path)
            .with_context(|| format!("failed to stat {}", zip_path.display()))?
            .len(),
    };
    let published_path = build_root.join(format!("{file_prefix}_{sha256}.zip"));
    if !published_path.is_file() {
        fs::hard_link(zip_path, &published_path).with_context(|| {
            format!(
                "failed to hardlink {} to {}",
                zip_path.display(),
                published_path.display()
            )
        })?;
    }
    Ok((published_path, sha256, size_bytes))
}

pub(super) fn build_current_bundle_entries(
    build_root: &Path,
    as_of_date: NaiveDate,
) -> anyhow::Result<Vec<CurrentBundleEntry>> {
    let mut bundle_paths = fs::read_dir(build_root)
        .with_context(|| format!("failed to read {}", build_root.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to iterate {}", build_root.display()))?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("bundle_cycle_") && name.ends_with(".json"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    bundle_paths.sort();

    let mut cycle_bundles_by_cycle =
        BTreeMap::<String, (u32, String, SystemTime, CurrentBundleEntry)>::new();
    for bundle_path in bundle_paths {
        let metadata = fs::metadata(&bundle_path)
            .with_context(|| format!("failed to stat {}", bundle_path.display()))?;
        let modified_at = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let entry = match current_bundle_entry_from_path(&bundle_path) {
            Ok(entry) => entry,
            Err(error) => {
                eprintln!(
                    "WARNING skipping invalid public bundle candidate {}: {error:#}",
                    bundle_path.display()
                );
                continue;
            }
        };
        let filename = entry.filename.clone();
        if filename.starts_with("bundle_cycle_") {
            let end_valid_date = NaiveDate::parse_from_str(&entry.end_valid, "%Y-%m-%d")
                .with_context(|| format!("failed to parse bundle end_valid {}", entry.end_valid))?;
            if end_valid_date < as_of_date {
                continue;
            }
            let bundle_manifest: serde_json::Value = match serde_json::from_slice(
                &fs::read(&bundle_path)
                    .with_context(|| format!("failed to read {}", bundle_path.display()))?,
            ) {
                Ok(value) => value,
                Err(error) => {
                    eprintln!(
                        "WARNING skipping invalid public cycle bundle candidate {}: {error:#}",
                        bundle_path.display()
                    );
                    continue;
                }
            };
            let generated_at_utc = bundle_manifest
                .get("generated_at_utc")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            let cycle_version_rank = entry.cycle_version.parse::<u32>().unwrap_or(0);
            let should_replace = match cycle_bundles_by_cycle.get(&entry.cycle) {
                Some((
                    existing_version_rank,
                    existing_generated_at_utc,
                    existing_modified_at,
                    _,
                )) => {
                    cycle_version_rank > *existing_version_rank
                        || (cycle_version_rank == *existing_version_rank
                            && generated_at_utc > *existing_generated_at_utc)
                        || (cycle_version_rank == *existing_version_rank
                            && generated_at_utc == *existing_generated_at_utc
                            && modified_at > *existing_modified_at)
                }
                None => true,
            };
            if should_replace {
                cycle_bundles_by_cycle.insert(
                    entry.cycle.clone(),
                    (cycle_version_rank, generated_at_utc, modified_at, entry),
                );
            }
            continue;
        }
    }
    let mut bundles = cycle_bundles_by_cycle
        .into_values()
        .map(|(_, _, _, entry)| entry)
        .collect::<Vec<_>>();
    bundles.sort_by(|left, right| {
        let left_key = (
            left.bundle_type != "cycle",
            left.cycle.as_str(),
            left.id.as_str(),
        );
        let right_key = (
            right.bundle_type != "cycle",
            right.cycle.as_str(),
            right.id.as_str(),
        );
        left_key.cmp(&right_key)
    });
    Ok(bundles)
}

pub(super) fn current_bundle_entry_from_path(
    bundle_path: &Path,
) -> anyhow::Result<CurrentBundleEntry> {
    let metadata = fs::metadata(bundle_path)
        .with_context(|| format!("failed to stat {}", bundle_path.display()))?;
    let filename = filename_string(bundle_path)?;
    if filename.starts_with("bundle_cycle_") {
        let bundle_manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(bundle_path)
                .with_context(|| format!("failed to read {}", bundle_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", bundle_path.display()))?;
        let bundle_cycle = bundle_manifest
            .get("cycle")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("bundle manifest missing top-level cycle"))?;
        let bundle_cycle_version = bundle_manifest
            .get("cycle_version")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let (file_cycle, file_cycle_version, file_hash) = parse_cycle_bundle_filename(bundle_path)?;
        if bundle_cycle != file_cycle || bundle_cycle_version != file_cycle_version {
            anyhow::bail!(
                "bundle cycle mismatch for {}: payload cycle {}_{} != filename cycle {}_{}",
                bundle_path.display(),
                bundle_cycle,
                bundle_cycle_version,
                file_cycle,
                file_cycle_version
            );
        }
        let bundle_sha256 = hash_file(bundle_path)?;
        if bundle_sha256 != file_hash {
            anyhow::bail!(
                "bundle hash mismatch for {}: filename hash {} != content hash {}",
                bundle_path.display(),
                file_hash,
                bundle_sha256
            );
        }
        let start_valid = bundle_manifest
            .get("start_valid")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("bundle manifest missing start_valid"))?;
        let end_valid = bundle_manifest
            .get("end_valid")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("bundle manifest missing end_valid"))?;
        return Ok(CurrentBundleEntry {
            filename: filename.clone(),
            relative_path: filename,
            id: format!("cycle_{bundle_cycle}_{bundle_cycle_version}"),
            bundle_type: "cycle".to_string(),
            cycle: bundle_cycle.to_string(),
            cycle_version: bundle_cycle_version.to_string(),
            start_valid: start_valid.to_string(),
            end_valid: end_valid.to_string(),
            checksum_sha256: bundle_sha256,
            size_bytes: metadata.len(),
        });
    }
    bail!("unsupported bundle filename {}", bundle_path.display());
}

pub(super) fn parse_cycle_bundle_filename(path: &Path) -> anyhow::Result<(String, String, String)> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("bundle path has no filename: {}", path.display()))?;
    let stem = filename
        .strip_suffix(".json")
        .ok_or_else(|| anyhow::anyhow!("bundle filename does not end in .json: {filename}"))?;
    let rest = stem.strip_prefix("bundle_cycle_").ok_or_else(|| {
        anyhow::anyhow!("bundle filename must start with bundle_cycle_: {filename}")
    })?;
    let mut parts = rest.rsplitn(3, '_').collect::<Vec<_>>();
    if parts.len() != 3 {
        anyhow::bail!("bundle filename must be bundle_cycle_YYCC_VV_<sha256>.json: {filename}");
    }
    let hash = parts.remove(0).to_string();
    let version = parts.remove(0).to_string();
    let cycle = parts.remove(0).to_string();
    if hash.len() != 64 || !hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
        anyhow::bail!("bundle filename has invalid sha256 suffix: {filename}");
    }
    Ok((cycle, version, hash))
}

pub(super) fn current_artifacts_latest_alias_filename() -> &'static str {
    "current_artifacts.json"
}

pub(super) fn product_artifacts_filename() -> &'static str {
    "product_artifacts.json"
}

pub(super) fn product_facts_filename() -> &'static str {
    "product-facts.json"
}

pub(super) fn write_current_artifacts_json(
    path: &Path,
    manifests: &[CurrentArtifactsManifest],
) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(manifests)
        .context("failed to encode current artifacts manifest")?;
    write_public_json_atomic(path, &bytes)
}

pub(super) fn write_product_artifacts_json(
    path: &Path,
    manifest: &CurrentArtifactsManifest,
) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(manifest)
        .context("failed to encode product artifacts manifest")?;
    write_public_json_atomic(path, &bytes)
}

pub(super) fn publication_root_for_packaged_root(packaged_root: &Path) -> anyhow::Result<PathBuf> {
    let publish_dir = packaged_root.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "packaged publication dir has no publish_dir parent: {}",
            packaged_root.display()
        )
    })?;
    publication_root_for_publish_dir(publish_dir)
}

pub(super) fn publication_root_for_publish_dir(publish_dir: &Path) -> anyhow::Result<PathBuf> {
    let label_dir = publish_dir.parent().ok_or_else(|| {
        anyhow::anyhow!("publish_dir has no label parent: {}", publish_dir.display())
    })?;
    let published_dir = label_dir.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "publish_dir has no published parent: {}",
            publish_dir.display()
        )
    })?;
    if published_dir.file_name().and_then(|name| name.to_str()) != Some("published") {
        bail!(
            "publish_dir must be under <build_root>/published/<label>/<timestamp>, got {}",
            publish_dir.display()
        );
    }
    Ok(published_dir.to_path_buf())
}

fn publication_root_for_unpacked_root(unpacked_root: &Path) -> anyhow::Result<PathBuf> {
    let publish_dir = unpacked_root.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "unpacked publication dir has no publish_dir parent: {}",
            unpacked_root.display()
        )
    })?;
    publication_root_for_publish_dir(publish_dir)
}

pub(super) fn current_artifact_roots_for_packaged_root(
    packaged_root: &Path,
) -> anyhow::Result<CurrentArtifactRoots> {
    let publication_root = publication_root_for_packaged_root(packaged_root)?;
    let unpacked_root = published_unpacked_root_from_packaged_dir(packaged_root)?;
    Ok(CurrentArtifactRoots {
        packaged: publication_root_url(
            &publication_root,
            packaged_root,
            "artifact_roots.packaged",
        )?,
        unpacked: publication_root_url(
            &publication_root,
            &unpacked_root,
            "artifact_roots.unpacked",
        )?,
    })
}

fn publication_root_url(
    publication_root: &Path,
    artifact_root: &Path,
    field: &str,
) -> anyhow::Result<String> {
    let publication_root = normalize_absolute_path(publication_root);
    let artifact_root = normalize_absolute_path(artifact_root);
    let relative = artifact_root
        .strip_prefix(&publication_root)
        .with_context(|| {
            format!(
                "{field} root {} is not under publication root {}",
                artifact_root.display(),
                publication_root.display()
            )
        })?;
    let value = relative.display().to_string();
    if value.is_empty() {
        bail!("{field} root must not be the publication root itself");
    }
    Ok(format!("{}/", value.trim_matches('/')))
}

pub(super) fn write_current_artifacts_aliases(
    build_root: &Path,
    _as_of_utc: DateTime<Utc>,
    manifests: &[CurrentArtifactsManifest],
) -> anyhow::Result<PathBuf> {
    let publication_root = build_root.join("published");
    fs::create_dir_all(&publication_root)
        .with_context(|| format!("failed to create {}", publication_root.display()))?;

    let latest_filename = current_artifacts_latest_alias_filename();
    let publication_latest_path = publication_root.join(latest_filename);
    write_current_artifacts_json(&publication_latest_path, manifests)?;

    Ok(publication_latest_path)
}

pub(super) fn write_product_artifacts_manifest_json(
    publish_dir: &Path,
    manifest: &CurrentArtifactsManifest,
) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(publish_dir)
        .with_context(|| format!("failed to create {}", publish_dir.display()))?;
    let product_artifacts_path = publish_dir.join(product_artifacts_filename());
    write_product_artifacts_json(&product_artifacts_path, manifest)?;
    Ok(product_artifacts_path)
}

pub(super) fn write_current_artifacts_manifest(
    packaged_dir: &Path,
    as_of_utc: DateTime<Utc>,
    diagnostics: Option<CurrentDiagnosticsEntry>,
) -> anyhow::Result<PathBuf> {
    let as_of_date = as_of_utc.date_naive();
    let bundles = build_current_bundle_entries(packaged_dir, as_of_date)?;
    let artifact_roots = current_artifact_roots_for_packaged_root(packaged_dir)?;
    let startup_prefetch =
        current_startup_prefetch_manifest(packaged_dir, &artifact_roots, &bundles)?;
    let contracts = current_artifacts_contracts(packaged_dir, &bundles)?;
    let manifest = CurrentArtifactsManifest {
        schema_version: 1,
        contracts,
        artifact_roots,
        as_of_date: as_of_date.format("%Y-%m-%d").to_string(),
        as_of_utc: as_of_utc.to_rfc3339_opts(SecondsFormat::Secs, true),
        bundles,
        startup_prefetch,
        diagnostics,
    };
    let publish_dir = packaged_dir.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "packaged publication dir has no publish_dir parent: {}",
            packaged_dir.display()
        )
    })?;
    write_product_artifacts_manifest_json(publish_dir, &manifest)
}

pub(super) fn current_artifacts_contracts(
    build_root: &Path,
    bundles: &[CurrentBundleEntry],
) -> anyhow::Result<BTreeMap<String, String>> {
    let mut contracts = BTreeMap::new();
    for bundle_ref in bundles {
        let bundle_path = build_root.join(&bundle_ref.filename);
        let bundle = load_bundle_manifest(&bundle_path)?;
        for package in bundle.packages {
            match contracts.entry(package.family_id.clone()) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(package.contract_id.clone());
                }
                std::collections::btree_map::Entry::Occupied(entry)
                    if entry.get() == &package.contract_id => {}
                std::collections::btree_map::Entry::Occupied(entry) => {
                    bail!(
                        "conflicting contracts for family {} in current artifacts: {} vs {}",
                        package.family_id,
                        entry.get(),
                        package.contract_id
                    );
                }
            }
        }
    }
    Ok(contracts)
}

pub fn merge_current_artifacts_manifests(
    build_root: &Path,
    as_of_utc: DateTime<Utc>,
    manifest_paths: &[PathBuf],
) -> anyhow::Result<PathBuf> {
    if manifest_paths.is_empty() {
        bail!("merge-current-artifacts requires at least one --manifest");
    }
    let manifests = manifest_paths
        .iter()
        .map(|path| {
            let manifest = load_current_artifacts_manifest(path)?;
            validate_current_artifacts_manifest(&manifest, path)?;
            Ok(manifest)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    validate_merged_current_artifacts(build_root, &manifests)?;
    write_current_artifacts_aliases(build_root, as_of_utc, &manifests)
}

pub(super) fn current_startup_prefetch_manifest(
    build_root: &Path,
    artifact_roots: &CurrentArtifactRoots,
    bundles: &[CurrentBundleEntry],
) -> anyhow::Result<Option<CurrentStartupPrefetchManifest>> {
    let mut cycle_resources = Vec::new();
    for bundle_ref in bundles {
        let bundle = load_bundle_manifest(&build_root.join(&bundle_ref.filename))?;
        let mut resources = Vec::new();
        let mut seen_urls = BTreeSet::new();
        for package in bundle
            .packages
            .iter()
            .filter(|package| package.family_id == "nav-db")
        {
            let Some(members_value) = package
                .metadata
                .get(NAV_DB_STARTUP_PREFETCH_MEMBERS_METADATA_KEY)
            else {
                continue;
            };
            let members = serde_json::from_value::<Vec<String>>(members_value.clone())
                .with_context(|| {
                    format!(
                        "package {} metadata.{} must be a string array",
                        package.id, NAV_DB_STARTUP_PREFETCH_MEMBERS_METADATA_KEY
                    )
                })?;
            let package_dir = zip_stem(&package.filename)?;
            for member in members {
                validate_public_package_member(
                    &member,
                    "bundle.packages[].metadata.startup_prefetch_members[]",
                )?;
                let url = join_publication_url(&[
                    artifact_roots.unpacked.as_str(),
                    package_dir.as_str(),
                    member.as_str(),
                ]);
                if seen_urls.insert(url.clone()) {
                    resources.push(CurrentStartupPrefetchResource { url });
                }
            }
        }
        if !resources.is_empty() {
            cycle_resources.push(CurrentStartupPrefetchCycleResources {
                bundle_id: bundle.bundle_id,
                cycle: bundle.cycle,
                cycle_version: bundle.cycle_version,
                start_valid: bundle.start_valid,
                end_valid: bundle.end_valid,
                resources,
            });
        }
    }
    Ok(
        (!cycle_resources.is_empty()).then_some(CurrentStartupPrefetchManifest {
            schema_version: 1,
            cycle_resources,
        }),
    )
}

fn join_publication_url(parts: &[&str]) -> String {
    parts
        .iter()
        .filter_map(|part| {
            let trimmed = part.trim_matches('/');
            (!trimmed.is_empty()).then_some(trimmed)
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn write_build_status_html(
    config: &ProductBuildConfig,
    current_artifacts_path: &Path,
) -> anyhow::Result<()> {
    let status = build_status_document(&config.packaged_dir, current_artifacts_path)?;
    write_product_facts_json(&config.packaged_dir, current_artifacts_path, &status)?;
    let html = render_build_status_html(&status)?;
    let packaged_path = config.packaged_dir.join("build-status.html");
    fs::write(&packaged_path, &html)
        .with_context(|| format!("failed to write {}", packaged_path.display()))?;
    let unpacked_root = published_unpacked_root(config)?;
    if unpacked_root.is_dir() {
        let unpacked_path = unpacked_root.join("build-status.html");
        fs::write(&unpacked_path, html)
            .with_context(|| format!("failed to write {}", unpacked_path.display()))?;
    }
    Ok(())
}

pub(super) fn write_product_facts_json(
    build_root: &Path,
    current_artifacts_path: &Path,
    status: &BuildStatusDocument,
) -> anyhow::Result<()> {
    let facts =
        product_facts_document(build_root, current_artifacts_path, &status.generated_at_utc)?;
    let facts_path = build_root.join(product_facts_filename());
    write_public_json_atomic(
        &facts_path,
        &serde_json::to_vec_pretty(&facts).context("failed to encode product facts")?,
    )
}

pub(super) fn product_facts_document(
    build_root: &Path,
    current_artifacts_path: &Path,
    generated_at_utc: &str,
) -> anyhow::Result<ProductFactsDocument> {
    let current = load_current_artifacts_manifest(current_artifacts_path)?;
    let build_diagnostics = load_build_diagnostics(build_root, &current)?;
    let mut products = Vec::new();
    for bundle_ref in &current.bundles {
        let bundle_path = build_root.join(&bundle_ref.filename);
        let bundle = load_bundle_manifest(&bundle_path)?;
        for package in &bundle.packages {
            let diagnostics = product_diagnostic_counts(&bundle, package, &build_diagnostics);
            products.push(product_facts_product(&bundle, package, diagnostics));
        }
    }
    products.sort_by(|left, right| {
        (
            left.cycle.as_deref().unwrap_or(""),
            left.family.as_str(),
            left.region_id.as_deref().unwrap_or(""),
            left.product_id.as_str(),
        )
            .cmp(&(
                right.cycle.as_deref().unwrap_or(""),
                right.family.as_str(),
                right.region_id.as_deref().unwrap_or(""),
                right.product_id.as_str(),
            ))
    });
    Ok(ProductFactsDocument {
        schema_version: 1,
        generated_at_utc: generated_at_utc.to_string(),
        build: ProductFactsBuild {
            status: "pass".to_string(),
            completed_at_utc: generated_at_utc.to_string(),
            current_artifacts: filename_string(current_artifacts_path)?,
        },
        products,
    })
}

fn product_facts_product(
    bundle: &BundleManifest,
    package: &BundlePackageArtifact,
    mut diagnostics: ProductFactsDiagnostics,
) -> ProductFactsProduct {
    if package.family_id == "nav-db" {
        diagnostics.procedure_geometry_warning_count +=
            package_metadata_usize(package, "procedure_geometry_warning_count");
        diagnostics.procedure_geometry_error_count +=
            package_metadata_usize(package, "procedure_geometry_error_count");
    }
    ProductFactsProduct {
        product_id: package.id.clone(),
        family: package.family_id.clone(),
        contract: package.contract_id.clone(),
        region_id: package.region_id.clone(),
        cycle: package.cycle.clone().or_else(|| non_empty(&bundle.cycle)),
        cycle_version: package
            .cycle_version
            .clone()
            .or_else(|| non_empty(&bundle.cycle_version)),
        effective_date: package.effective_date.clone(),
        expiration_date: package.expiration_date.clone(),
        source_generated_at_utc: package.source_generated_at_utc.clone(),
        source_fetched_at_utc: package.source_fetched_at_utc.clone(),
        published_at_utc: package.published_at_utc.clone(),
        error_count: diagnostics.error_count(),
        warning_count: diagnostics.warning_count(),
        diagnostics,
    }
}

fn package_metadata_usize(package: &BundlePackageArtifact, key: &str) -> usize {
    package
        .metadata
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(0)
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

fn load_build_diagnostics(
    build_root: &Path,
    current: &CurrentArtifactsManifest,
) -> anyhow::Result<Vec<BuildDiagnosticEntry>> {
    let Some(diagnostics) = &current.diagnostics else {
        return Ok(Vec::new());
    };
    let path = build_root.join(&diagnostics.filename);
    let manifest: BuildDiagnosticsManifest = serde_json::from_slice(
        &fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(manifest.errors)
}

fn product_diagnostic_counts(
    bundle: &BundleManifest,
    package: &BundlePackageArtifact,
    diagnostics: &[BuildDiagnosticEntry],
) -> ProductFactsDiagnostics {
    let mut counts = ProductFactsDiagnostics::default();
    for diagnostic in diagnostics
        .iter()
        .filter(|diagnostic| diagnostic_applies_to_package(diagnostic, bundle, package))
    {
        let is_error = diagnostic.severity.eq_ignore_ascii_case("ERROR");
        let product = diagnostic.product.as_str();
        let code = diagnostic.code.as_str();
        if product == "vectors" {
            if is_error {
                counts.vector_validator_error_count += 1;
            } else {
                counts.vector_validator_warning_count += 1;
            }
        } else if product == "procedure_geometry"
            || product == "procedure-geometry"
            || code.starts_with("procedure_geometry")
            || code.starts_with("procedure-geometry")
        {
            if is_error {
                counts.procedure_geometry_error_count += 1;
            } else {
                counts.procedure_geometry_warning_count += 1;
            }
        } else if is_error {
            counts.other_error_count += 1;
        } else {
            counts.other_warning_count += 1;
        }
    }
    counts
}

fn diagnostic_applies_to_package(
    diagnostic: &BuildDiagnosticEntry,
    bundle: &BundleManifest,
    package: &BundlePackageArtifact,
) -> bool {
    let package_cycle = package.cycle.as_deref().unwrap_or(&bundle.cycle);
    if diagnostic
        .cycle
        .as_deref()
        .is_some_and(|cycle| cycle != package_cycle)
    {
        return false;
    }
    match diagnostic.product.as_str() {
        "vectors" | "procedure_geometry" | "procedure-geometry" => package.family_id == "nav-db",
        product => product == package.id || product == package.family_id,
    }
}

impl ProductFactsDiagnostics {
    fn error_count(&self) -> usize {
        self.procedure_geometry_error_count
            + self.vector_validator_error_count
            + self.other_error_count
    }

    fn warning_count(&self) -> usize {
        self.procedure_geometry_warning_count
            + self.vector_validator_warning_count
            + self.other_warning_count
    }
}

pub(super) fn build_status_document(
    build_root: &Path,
    current_artifacts_path: &Path,
) -> anyhow::Result<BuildStatusDocument> {
    let current = load_current_artifacts_manifest(current_artifacts_path)?;
    let mut products = Vec::new();
    for bundle_ref in &current.bundles {
        let bundle_path = build_root.join(&bundle_ref.filename);
        let bundle = load_bundle_manifest(&bundle_path)?;
        let bundle_id = if bundle.bundle_id.is_empty() {
            bundle_ref.id.clone()
        } else {
            bundle.bundle_id.clone()
        };
        for package in bundle.packages {
            products.push(build_status_product(
                "cycle",
                &bundle_id,
                Some(bundle.cycle.as_str()),
                package,
            ));
        }
    }
    products.sort_by(|left, right| {
        (
            left.bundle_type.as_str(),
            left.cycle.as_deref().unwrap_or(""),
            left.family_id.as_str(),
            left.region_id.as_deref().unwrap_or(""),
            left.id.as_str(),
        )
            .cmp(&(
                right.bundle_type.as_str(),
                right.cycle.as_deref().unwrap_or(""),
                right.family_id.as_str(),
                right.region_id.as_deref().unwrap_or(""),
                right.id.as_str(),
            ))
    });
    Ok(BuildStatusDocument {
        schema_version: 1,
        generated_at_utc: utc_now_string(),
        build_root: build_root.display().to_string(),
        current_artifacts: filename_string(current_artifacts_path)?,
        disk: build_status_disk(build_root)?,
        warnings: build_status_warnings(build_root)?,
        products,
    })
}

pub(super) fn build_status_warnings(build_root: &Path) -> anyhow::Result<Vec<BuildStatusWarning>> {
    let mut warnings = Vec::new();
    for entry in fs::read_dir(build_root)
        .with_context(|| format!("failed to read {}", build_root.display()))?
    {
        let path = entry?.path();
        let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !(filename.starts_with("bundle_cycle_") && filename.ends_with(".json")) {
            continue;
        }
        if let Err(error) = load_bundle_manifest(&path) {
            warnings.push(BuildStatusWarning {
                severity: "WARNING".to_string(),
                code: "invalid_public_bundle_manifest".to_string(),
                path: filename.to_string(),
                message: error.to_string(),
            });
        }
    }
    warnings.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(warnings)
}

pub(super) fn build_status_product(
    bundle_type: &str,
    bundle_id: &str,
    bundle_cycle: Option<&str>,
    package: BundlePackageArtifact,
) -> BuildStatusProduct {
    let declared_time = package
        .source_generated_at_utc
        .clone()
        .or_else(|| package.effective_date.clone());
    let fetch_time = package
        .source_fetched_at_utc
        .clone()
        .or_else(|| package.published_at_utc.clone());
    BuildStatusProduct {
        bundle_type: bundle_type.to_string(),
        bundle_id: bundle_id.to_string(),
        cycle: package
            .cycle
            .clone()
            .or_else(|| bundle_cycle.map(str::to_string)),
        id: package.id,
        family_id: package.family_id,
        region_id: package.region_id,
        filename: package.filename,
        size_bytes: package.size_bytes,
        declared_time,
        fetch_time,
        effective_date: package.effective_date,
        expiration_date: package.expiration_date,
        source_generated_at_utc: package.source_generated_at_utc,
        source_fetched_at_utc: package.source_fetched_at_utc,
        published_at_utc: package.published_at_utc,
    }
}

pub(super) fn build_status_disk(path: &Path) -> anyhow::Result<BuildStatusDisk> {
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .with_context(|| format!("failed to encode path {}", path.display()))?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    let result = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("failed to stat filesystem {}", path.display()));
    }
    let stat = unsafe { stat.assume_init() };
    let block_size = stat.f_frsize as u128;
    let total_bytes = block_size.saturating_mul(stat.f_blocks as u128);
    let free_bytes = block_size.saturating_mul(stat.f_bfree as u128);
    let available_bytes = block_size.saturating_mul(stat.f_bavail as u128);
    let used_bytes = total_bytes.saturating_sub(free_bytes);
    let percent_free = if total_bytes == 0 {
        0.0
    } else {
        (available_bytes as f64 / total_bytes as f64) * 100.0
    };
    Ok(BuildStatusDisk {
        path: path.display().to_string(),
        total_bytes: u64::try_from(total_bytes).unwrap_or(u64::MAX),
        used_bytes: u64::try_from(used_bytes).unwrap_or(u64::MAX),
        free_bytes: u64::try_from(free_bytes).unwrap_or(u64::MAX),
        available_bytes: u64::try_from(available_bytes).unwrap_or(u64::MAX),
        percent_free,
    })
}

pub(super) fn render_build_status_html(status: &BuildStatusDocument) -> anyhow::Result<String> {
    let json = serde_json::to_string(status).context("failed to encode build status JSON")?;
    Ok(format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Aerobag Build Status</title>
<style>
:root {{ color-scheme: light dark; font-family: ui-sans-serif, system-ui, sans-serif; }}
body {{ margin: 2rem; line-height: 1.35; }}
table {{ border-collapse: collapse; width: 100%; font-size: 0.9rem; }}
th, td {{ border-bottom: 1px solid #9996; padding: 0.35rem 0.5rem; text-align: left; vertical-align: top; }}
th {{ position: sticky; top: 0; background: Canvas; }}
.summary {{ display: flex; flex-wrap: wrap; gap: 1rem; margin: 1rem 0 1.5rem; }}
.card {{ border: 1px solid #9996; border-radius: 0.5rem; padding: 0.75rem 1rem; }}
.muted {{ color: #777; }}
.warn {{ color: #9a6700; font-weight: 700; }}
.ok {{ color: #1a7f37; font-weight: 700; }}
</style>
</head>
<body>
<h1>Aerobag Build Status</h1>
<div id="app"></div>
<script id="status-data" type="application/json">{json}</script>
<script>
const status = JSON.parse(document.getElementById('status-data').textContent);
const app = document.getElementById('app');
const fmtBytes = (value) => {{
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let n = Number(value || 0);
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {{ n /= 1024; i++; }}
  return `${{n.toFixed(i === 0 ? 0 : 1)}} ${{units[i]}}`;
}};
const parseTime = (value) => {{
  if (!value) return null;
  if (/^\d{{4}}-\d{{2}}-\d{{2}}$/.test(value)) return new Date(`${{value}}T00:00:00Z`);
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : date;
}};
const fmtAge = (value) => {{
  const date = parseTime(value);
  if (!date) return '';
  const seconds = Math.max(0, (Date.now() - date.getTime()) / 1000);
  const units = [['d', 86400], ['h', 3600], ['m', 60]];
  for (const [label, size] of units) {{
    if (seconds >= size) return `${{Math.floor(seconds / size)}}${{label}} ago`;
  }}
  return `${{Math.floor(seconds)}}s ago`;
}};
const text = (value) => value == null || value === '' ? '' : String(value);
const esc = (value) => text(value).replace(/[&<>"']/g, (ch) => ({{'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}})[ch]);
const timeCell = (value) => value ? `${{esc(value)}} <span class="muted">(${{fmtAge(value)}})</span>` : '<span class="muted">n/a</span>';
const diskClass = status.disk.percent_free < 10 ? 'warn' : '';
const warningClass = status.warnings.length > 0 ? 'warn' : 'ok';
const warningText = status.warnings.length > 0 ? `${{status.warnings.length}} warning(s)` : 'clean';
app.innerHTML = `
  <div class="summary">
    <div class="card"><b>Generated</b><br>${{esc(status.generated_at_utc)}} <span class="muted">(${{fmtAge(status.generated_at_utc)}})</span></div>
    <div class="card"><b>Current Artifacts</b><br>${{esc(status.current_artifacts)}}</div>
    <div class="card"><b>Build Root</b><br>${{esc(status.build_root)}}</div>
    <div class="card"><b>Diagnostics</b><br><span class="${{warningClass}}">${{warningText}}</span></div>
    <div class="card"><b>Disk</b><br>
      used ${{fmtBytes(status.disk.used_bytes)}} / total ${{fmtBytes(status.disk.total_bytes)}}<br>
      free ${{fmtBytes(status.disk.available_bytes)}} <span class="${{diskClass}}">(${{status.disk.percent_free.toFixed(1)}}% free)</span>
    </div>
  </div>
  ${{status.warnings.length > 0 ? `
    <h2>Warnings</h2>
    <table>
      <thead><tr><th>Severity</th><th>Code</th><th>Path</th><th>Message</th></tr></thead>
      <tbody>
        ${{status.warnings.map((warning) => `
          <tr>
            <td class="warn">${{esc(warning.severity)}}</td>
            <td>${{esc(warning.code)}}</td>
            <td><code>${{esc(warning.path)}}</code></td>
            <td>${{esc(warning.message)}}</td>
          </tr>
        `).join('')}}
      </tbody>
    </table>
  ` : ''}}
  <h2>Products</h2>
  <table>
    <thead><tr>
      <th>Build</th><th>Product</th><th>Region</th><th>Cycle</th><th>Declared Time</th><th>Fetch Time</th><th>Size</th><th>File</th>
    </tr></thead>
    <tbody>
      ${{status.products.map((p) => `
        <tr>
          <td>${{esc(p.bundle_type)}}</td>
          <td>${{esc(p.id || p.family_id)}}</td>
          <td>${{esc(p.region_id)}}</td>
          <td>${{esc(p.cycle)}}</td>
          <td>${{timeCell(p.declared_time)}}</td>
          <td>${{timeCell(p.fetch_time)}}</td>
          <td>${{fmtBytes(p.size_bytes)}}</td>
          <td><code>${{esc(p.filename)}}</code></td>
        </tr>
      `).join('')}}
    </tbody>
  </table>
`;
</script>
</body>
</html>
"#
    ))
}

pub(super) fn write_product_build_diagnostics(
    build_root: &Path,
    as_of_date: NaiveDate,
    task_values: &BTreeMap<String, ProductTaskValue>,
) -> anyhow::Result<Option<CurrentDiagnosticsEntry>> {
    let mut errors = Vec::new();
    for (task_id, task_value) in task_values {
        if !task_id.ends_with(":vectors") {
            continue;
        }
        let cycle = task_id.trim_end_matches(":vectors").to_string();
        let ProductTaskValue::VectorHad {
            errors: errors_path,
            ..
        } = task_value
        else {
            continue;
        };
        let payload: serde_json::Value = serde_json::from_slice(
            &fs::read(errors_path)
                .with_context(|| format!("failed to read {}", errors_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", errors_path.display()))?;
        let product = payload
            .get("product")
            .and_then(|value| value.as_str())
            .unwrap_or("vectors")
            .to_string();
        for error in payload
            .get("errors")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
        {
            errors.push(BuildDiagnosticEntry {
                product: product.clone(),
                cycle: Some(cycle.clone()),
                severity: error
                    .get("severity")
                    .and_then(|value| value.as_str())
                    .unwrap_or("ERROR")
                    .to_string(),
                code: error
                    .get("code")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                message: error
                    .get("message")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unspecified build diagnostic")
                    .to_string(),
                expected: error
                    .get("expected")
                    .and_then(|value| value.as_u64())
                    .map(|value| value as usize),
                actual: error
                    .get("actual")
                    .and_then(|value| value.as_u64())
                    .map(|value| value as usize),
            });
        }
    }
    let error_count = errors
        .iter()
        .filter(|error| error.severity == "ERROR")
        .count();
    let filename = format!("build_errors_{}.json", as_of_date.format("%Y%m%d"));
    let path = build_root.join(&filename);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&BuildDiagnosticsManifest {
            schema_version: 1,
            generated_at_utc: utc_now_string(),
            error_count,
            errors,
        })
        .context("failed to encode build diagnostics manifest")?,
    )
    .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(Some(CurrentDiagnosticsEntry {
        filename,
        error_count,
    }))
}

pub(super) fn cleanup_published_packaged_root(
    packaged_root: &Path,
    current_artifacts_path: &Path,
) -> anyhow::Result<()> {
    let keep = collect_reachable_packaged_entries(packaged_root, current_artifacts_path)?;
    prune_root_to_keep_set(packaged_root, &keep)
}

pub(super) fn cleanup_published_unpacked_root(
    unpacked_root: &Path,
    current_artifacts_path: &Path,
) -> anyhow::Result<()> {
    let keep = collect_reachable_unpacked_entries(unpacked_root, current_artifacts_path)?;
    prune_root_to_keep_set(unpacked_root, &keep)
}

pub(super) fn collect_reachable_packaged_entries(
    packaged_root: &Path,
    current_artifacts_path: &Path,
) -> anyhow::Result<BTreeSet<String>> {
    let mut keep = BTreeSet::new();
    for discovery_path in discovery_manifest_paths(packaged_root, current_artifacts_path)? {
        let is_current_discovery = same_path(&discovery_path, current_artifacts_path);
        match collect_reachable_packaged_entries_for_discovery(packaged_root, &discovery_path) {
            Ok(entries) => keep.extend(entries),
            Err(error) if !is_current_discovery => {
                eprintln!(
                    "WARNING dropping stale historical discovery {} from packaged publication: {error:#}",
                    discovery_path.display()
                );
            }
            Err(error) => return Err(error),
        }
    }
    Ok(keep)
}

pub(super) fn collect_reachable_packaged_entries_for_discovery(
    packaged_root: &Path,
    discovery_path: &Path,
) -> anyhow::Result<BTreeSet<String>> {
    let current = load_current_artifacts_manifest(discovery_path)?;
    let mut keep = BTreeSet::new();
    if discovery_path.parent() == Some(packaged_root) {
        keep.insert(filename_string(discovery_path)?);
    }
    if let Some(diagnostics) = &current.diagnostics {
        ensure_public_file_exists(&packaged_root.join(&diagnostics.filename))?;
        keep.insert(diagnostics.filename.clone());
    }
    for bundle_ref in &current.bundles {
        let bundle_path = packaged_root.join(&bundle_ref.filename);
        ensure_public_file_exists(&bundle_path)?;
        keep.insert(bundle_ref.filename.clone());
        let bundle = load_bundle_manifest(&bundle_path)?;
        for artifact in &bundle.ancillary {
            ensure_public_file_exists(&packaged_root.join(&artifact.filename))?;
            keep.insert(artifact.filename.clone());
        }
        for package in &bundle.packages {
            ensure_public_file_exists(&packaged_root.join(&package.filename))?;
            keep.insert(package.filename.clone());
        }
    }
    Ok(keep)
}

pub(super) fn collect_reachable_unpacked_entries(
    unpacked_root: &Path,
    current_artifacts_path: &Path,
) -> anyhow::Result<BTreeSet<String>> {
    let mut keep = BTreeSet::new();
    for discovery_path in discovery_manifest_paths(unpacked_root, current_artifacts_path)? {
        let is_current_discovery = same_path(&discovery_path, current_artifacts_path);
        match collect_reachable_unpacked_entries_for_discovery(unpacked_root, &discovery_path) {
            Ok(entries) => keep.extend(entries),
            Err(error) if !is_current_discovery => {
                eprintln!(
                    "WARNING dropping stale historical discovery {} from unpacked publication: {error:#}",
                    discovery_path.display()
                );
            }
            Err(error) => return Err(error),
        }
    }
    Ok(keep)
}

pub(super) fn collect_reachable_unpacked_entries_for_discovery(
    unpacked_root: &Path,
    discovery_path: &Path,
) -> anyhow::Result<BTreeSet<String>> {
    let current = load_current_artifacts_manifest(discovery_path)?;
    let mut keep = BTreeSet::new();
    if discovery_path.parent() == Some(unpacked_root) {
        keep.insert(filename_string(discovery_path)?);
    }
    if let Some(diagnostics) = &current.diagnostics {
        ensure_public_file_exists(&unpacked_root.join(&diagnostics.filename))?;
        keep.insert(diagnostics.filename.clone());
    }
    for bundle_ref in &current.bundles {
        let bundle_path = unpacked_root.join(&bundle_ref.filename);
        ensure_public_file_exists(&bundle_path)?;
        keep.insert(bundle_ref.filename.clone());
        let bundle = load_bundle_manifest(&bundle_path)?;
        for artifact in &bundle.ancillary {
            if artifact.filename.ends_with(".zip") {
                let stem = zip_stem(&artifact.filename)?;
                ensure_public_dir_exists(&unpacked_root.join(&stem))?;
                keep.insert(stem);
            } else {
                ensure_public_file_exists(&unpacked_root.join(&artifact.filename))?;
                keep.insert(artifact.filename.clone());
            }
        }
        for package in &bundle.packages {
            let stem = zip_stem(&package.filename)?;
            ensure_public_dir_exists(&unpacked_root.join(&stem))?;
            keep.insert(stem);
        }
    }
    Ok(keep)
}

pub(super) fn discovery_manifest_paths(
    root: &Path,
    current_artifacts_path: &Path,
) -> anyhow::Result<Vec<PathBuf>> {
    let _ = root;
    Ok(vec![current_artifacts_path.to_path_buf()])
}

pub(super) fn same_path(left: &Path, right: &Path) -> bool {
    left == right
        || (left.exists()
            && right.exists()
            && fs::canonicalize(left).ok() == fs::canonicalize(right).ok())
}

pub(super) fn prune_root_to_keep_set(root: &Path, keep: &BTreeSet<String>) -> anyhow::Result<()> {
    if !root.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy().to_string();
        if keep.contains(&name) {
            continue;
        }
        if path.is_dir() {
            fs::remove_dir_all(&path)
                .with_context(|| format!("failed to remove stale directory {}", path.display()))?;
        } else {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove stale file {}", path.display()))?;
        }
    }
    Ok(())
}

pub(super) fn load_current_artifacts_manifest(
    path: &Path,
) -> anyhow::Result<CurrentArtifactsManifest> {
    serde_json::from_slice(
        &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))
}

pub(super) fn load_bundle_manifest(path: &Path) -> anyhow::Result<BundleManifest> {
    let filename = filename_string(path)?;
    if filename.starts_with("bundle_cycle_") {
        let bundle: BundleManifest = serde_json::from_slice(
            &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", path.display()))?;
        return Ok(bundle);
    }
    bail!("unrecognized bundle filename: {filename}")
}

pub(super) fn filename_string(path: &Path) -> anyhow::Result<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .context("path has no filename")
}

pub(super) fn validate_packaged_contract(
    packaged_root: &Path,
    current_artifacts_path: &Path,
) -> anyhow::Result<()> {
    for discovery_path in discovery_manifest_paths(packaged_root, current_artifacts_path)? {
        validate_no_internal_paths_in_json(&discovery_path)?;
        let current = load_current_artifacts_manifest(&discovery_path)?;
        validate_current_artifacts_manifest(&current, &discovery_path)?;

        for bundle in &current.bundles {
            validate_public_filename(&bundle.filename, "current_artifacts.bundles[].filename")?;
            if !bundle.relative_path.is_empty() {
                validate_public_filename(
                    &bundle.relative_path,
                    "current_artifacts.bundles[].relative_path",
                )?;
                if bundle.filename != bundle.relative_path {
                    bail!(
                        "bundle filename/relative_path mismatch in current_artifacts: {} != {}",
                        bundle.filename,
                        bundle.relative_path
                    );
                }
            }
            let bundle_path = packaged_root.join(&bundle.filename);
            ensure_public_file_exists(&bundle_path)?;
            validate_embedded_sha256_filename(&bundle.filename, &bundle.checksum_sha256)?;
            let bundle_manifest = validate_bundle_manifest(packaged_root, &bundle_path)?;
            validate_bundle_contracts_match_current(&bundle_manifest, &current)?;
        }
        if let Some(diagnostics) = &current.diagnostics {
            validate_public_filename(
                &diagnostics.filename,
                "current_artifacts.diagnostics.filename",
            )?;
            let diagnostics_path = packaged_root.join(&diagnostics.filename);
            ensure_public_file_exists(&diagnostics_path)?;
            validate_no_internal_paths_in_json(&diagnostics_path)?;
        }
    }

    Ok(())
}

pub(super) fn validate_current_artifacts_manifest(
    current: &CurrentArtifactsManifest,
    path: &Path,
) -> anyhow::Result<()> {
    if current.schema_version != 1 {
        bail!(
            "{} has unexpected schema_version {}",
            path.display(),
            current.schema_version
        );
    }
    for (family_id, contract_id) in &current.contracts {
        validate_required_manifest_string(family_id, "current_artifacts.contracts key")?;
        validate_required_manifest_string(contract_id, "current_artifacts.contracts value")?;
    }
    validate_publication_root_url(
        &current.artifact_roots.packaged,
        "current_artifacts.artifact_roots.packaged",
    )?;
    validate_publication_root_url(
        &current.artifact_roots.unpacked,
        "current_artifacts.artifact_roots.unpacked",
    )?;
    if current.artifact_roots.packaged == current.artifact_roots.unpacked {
        bail!(
            "{} uses the same packaged and unpacked artifact roots: {}",
            path.display(),
            current.artifact_roots.packaged
        );
    }
    if let Some(prefetch) = &current.startup_prefetch {
        if prefetch.schema_version != 1 {
            bail!(
                "{} has unexpected startup_prefetch.schema_version {}",
                path.display(),
                prefetch.schema_version
            );
        }
        for cycle in &prefetch.cycle_resources {
            validate_required_manifest_string(
                &cycle.bundle_id,
                "current_artifacts.startup_prefetch.cycle_resources[].bundle_id",
            )?;
            validate_required_manifest_string(
                &cycle.cycle,
                "current_artifacts.startup_prefetch.cycle_resources[].cycle",
            )?;
            validate_required_manifest_string(
                &cycle.cycle_version,
                "current_artifacts.startup_prefetch.cycle_resources[].cycle_version",
            )?;
            validate_required_manifest_string(
                &cycle.start_valid,
                "current_artifacts.startup_prefetch.cycle_resources[].start_valid",
            )?;
            validate_required_manifest_string(
                &cycle.end_valid,
                "current_artifacts.startup_prefetch.cycle_resources[].end_valid",
            )?;
            for resource in &cycle.resources {
                validate_publication_resource_url(
                    &resource.url,
                    "current_artifacts.startup_prefetch.cycle_resources[].resources[].url",
                )?;
                if !resource.url.starts_with(&current.artifact_roots.unpacked) {
                    bail!(
                        "{} startup prefetch URL is not under artifact_roots.unpacked: {}",
                        path.display(),
                        resource.url
                    );
                }
            }
            if cycle.resources.is_empty() {
                bail!(
                    "{} startup prefetch cycle {} has no resources",
                    path.display(),
                    cycle.bundle_id
                );
            }
        }
    }
    Ok(())
}

fn validate_required_manifest_string(value: &str, field: &str) -> anyhow::Result<()> {
    if value.is_empty() {
        bail!("{field} must not be empty");
    }
    Ok(())
}

pub(super) fn validate_publication_resource_url(value: &str, field: &str) -> anyhow::Result<()> {
    if value.is_empty() {
        bail!("{field} must not be empty");
    }
    if value.starts_with('/') || value.contains('\\') || value.contains("://") {
        bail!("{field} must be a relative publication URL, got {value}");
    }
    for component in value.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            bail!("{field} has invalid path component in {value}");
        }
    }
    Ok(())
}

pub(super) fn validate_publication_root_url(value: &str, field: &str) -> anyhow::Result<()> {
    if !value.ends_with('/') {
        bail!("{field} must end with '/', got {value}");
    }
    let trimmed = value.trim_end_matches('/');
    validate_publication_resource_url(trimmed, field)
}

pub(super) fn validate_bundle_manifest(
    packaged_root: &Path,
    bundle_path: &Path,
) -> anyhow::Result<BundleManifest> {
    validate_no_internal_paths_in_json(bundle_path)?;
    let (_, _, filename_hash) = parse_cycle_bundle_filename(bundle_path)?;
    let bundle_hash = hash_file(bundle_path)?;
    if bundle_hash != filename_hash {
        bail!(
            "bundle filename hash mismatch for {}: filename {} != content {}",
            bundle_path.display(),
            filename_hash,
            bundle_hash
        );
    }
    let bundle: BundleManifest = serde_json::from_slice(
        &fs::read(bundle_path)
            .with_context(|| format!("failed to read {}", bundle_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", bundle_path.display()))?;

    for package in &bundle.packages {
        validate_public_filename(&package.filename, "bundle.packages[].filename")?;
        validate_public_filename(&package.relative_path, "bundle.packages[].relative_path")?;
        validate_embedded_sha256_filename(&package.filename, &package.checksum_sha256)?;
        if package.cycle.is_some()
            && package.cycle_version.as_deref() != Some(PACKAGE_CYCLE_VERSION)
        {
            bail!(
                "package {} has unexpected cycle_version {:?}",
                package.id,
                package.cycle_version
            );
        }
        if package.filename != package.relative_path {
            bail!(
                "package filename/relative_path mismatch in {}: {} != {}",
                bundle_path.display(),
                package.filename,
                package.relative_path
            );
        }
        if package.cycle.is_none() {
            if package.cycle_version.is_some() {
                bail!(
                    "stable package {} unexpectedly carries cycle_version {:?}",
                    package.id,
                    package.cycle_version
                );
            }
            if package.effective_date.is_none() {
                bail!("stable package {} is missing effective_date", package.id);
            }
            if package.expiration_date.is_some() {
                bail!(
                    "stable package {} unexpectedly carries expiration_date {:?}",
                    package.id,
                    package.expiration_date
                );
            }
        }
        ensure_public_file_exists(&packaged_root.join(&package.filename))?;
    }
    for artifact in &bundle.ancillary {
        validate_bundle_artifact_ref(packaged_root, artifact)?;
    }
    validate_bundle_contract_split(&bundle, bundle_path)?;
    Ok(bundle)
}

pub(super) fn validate_bundle_contracts_match_current(
    bundle: &BundleManifest,
    current: &CurrentArtifactsManifest,
) -> anyhow::Result<()> {
    for package in &bundle.packages {
        match current.contracts.get(&package.family_id) {
            Some(contract_id) if contract_id == &package.contract_id => {}
            Some(contract_id) => bail!(
                "package {} contract {} does not match current_artifacts.contracts[{}] {}",
                package.id,
                package.contract_id,
                package.family_id,
                contract_id
            ),
            None => bail!(
                "package {} family {} missing from current_artifacts.contracts",
                package.id,
                package.family_id
            ),
        }
    }
    Ok(())
}

pub(super) fn validate_merged_current_artifacts(
    build_root: &Path,
    manifests: &[CurrentArtifactsManifest],
) -> anyhow::Result<()> {
    let publication_root = build_root.join("published");
    let mut seen_contract_sets = BTreeSet::new();
    for manifest in manifests {
        let contract_set_key = manifest
            .contracts
            .iter()
            .map(|(family_id, contract_id)| format!("{family_id}={contract_id}"))
            .collect::<Vec<_>>()
            .join(",");
        if !seen_contract_sets.insert(contract_set_key) {
            bail!(
                "merge-current-artifacts contains duplicate contract set {:?}",
                manifest.contracts
            );
        }
        let manifest_packaged_root =
            publication_root.join(manifest.artifact_roots.packaged.trim_end_matches('/'));
        for bundle_ref in &manifest.bundles {
            validate_public_filename(
                &bundle_ref.filename,
                "current_artifacts[].bundles[].filename",
            )?;
            let bundle_path = manifest_packaged_root.join(&bundle_ref.filename);
            ensure_public_file_exists(&bundle_path)?;
            let bundle = validate_bundle_manifest(&manifest_packaged_root, &bundle_path)?;
            validate_bundle_contracts_match_current(&bundle, manifest)?;
        }
        if let Some(diagnostics) = &manifest.diagnostics {
            validate_public_filename(
                &diagnostics.filename,
                "current_artifacts[].diagnostics.filename",
            )?;
            ensure_public_file_exists(&manifest_packaged_root.join(&diagnostics.filename))?;
        }
    }
    Ok(())
}

pub(super) fn validate_unpacked_contract(
    packaged_root: &Path,
    unpacked_root: &Path,
    current_artifacts_path: &Path,
) -> anyhow::Result<()> {
    validate_packaged_contract(packaged_root, current_artifacts_path)?;
    for discovery_path in discovery_manifest_paths(packaged_root, current_artifacts_path)? {
        let is_current_discovery = same_path(&discovery_path, current_artifacts_path);
        match validate_unpacked_contract_for_discovery(unpacked_root, &discovery_path) {
            Ok(()) => {}
            Err(error) if !is_current_discovery => {
                eprintln!(
                    "WARNING skipping stale historical unpacked discovery {} during validation: {error:#}",
                    discovery_path.display()
                );
            }
            Err(error) => return Err(error),
        }
    }

    Ok(())
}

pub(super) fn validate_unpacked_contract_for_discovery(
    unpacked_root: &Path,
    discovery_path: &Path,
) -> anyhow::Result<()> {
    // Discovery manifests are hoisted to the publication root. The unpacked tree is
    // only the unpacked artifact root referenced by `artifact_roots.unpacked`.
    validate_no_internal_paths_in_json(discovery_path)?;
    let current = load_current_artifacts_manifest(discovery_path)?;
    validate_current_artifacts_manifest(&current, discovery_path)?;
    if let Some(prefetch) = &current.startup_prefetch {
        let publication_root = publication_root_for_unpacked_root(unpacked_root)?;
        for cycle in &prefetch.cycle_resources {
            for resource in &cycle.resources {
                ensure_public_file_exists(&publication_root.join(&resource.url))?;
            }
        }
    }

    for bundle in &current.bundles {
        let unpacked_bundle_path = unpacked_root.join(&bundle.filename);
        ensure_public_file_exists(&unpacked_bundle_path)?;
        validate_no_internal_paths_in_json(&unpacked_bundle_path)?;
        let bundle = load_bundle_manifest(&unpacked_bundle_path)?;
        validate_bundle_contracts_match_current(&bundle, &current)?;
        for artifact in &bundle.ancillary {
            if artifact.filename.ends_with(".zip") {
                ensure_public_dir_exists(&unpacked_root.join(zip_stem(&artifact.filename)?))?;
            } else {
                ensure_public_file_exists(&unpacked_root.join(&artifact.filename))?;
            }
        }
        for package in &bundle.packages {
            ensure_public_dir_exists(&unpacked_root.join(zip_stem(&package.filename)?))?;
        }
    }
    Ok(())
}

pub(super) fn validate_bundle_artifact_ref(
    packaged_root: &Path,
    artifact: &BundleArtifact,
) -> anyhow::Result<()> {
    validate_public_filename(&artifact.filename, "bundle artifact filename")?;
    validate_public_filename(&artifact.relative_path, "bundle artifact relative_path")?;
    validate_embedded_sha256_filename(&artifact.filename, &artifact.checksum_sha256)?;
    if artifact.filename != artifact.relative_path {
        bail!(
            "bundle artifact filename/relative_path mismatch: {} != {}",
            artifact.filename,
            artifact.relative_path
        );
    }
    ensure_public_file_exists(&packaged_root.join(&artifact.filename))
}

pub(super) fn validate_bundle_contract_split(
    bundle: &BundleManifest,
    bundle_path: &Path,
) -> anyhow::Result<()> {
    let has_nav_db_package = bundle
        .packages
        .iter()
        .any(|package| package.family_id == "nav-db" && package.region_id.is_none());
    if !has_nav_db_package {
        bail!(
            "bundle {} missing nav-db package row in packages[]",
            bundle_path.display()
        );
    }

    for package in &bundle.packages {
        if bundle
            .ancillary
            .iter()
            .any(|artifact| artifact.filename == package.filename)
        {
            bail!(
                "bundle {} lists {} in both packages[] and ancillary[]",
                bundle_path.display(),
                package.filename
            );
        }
    }
    for forbidden in ["resource_index_", "catalog_", "data_", "vectors_data_"] {
        if bundle
            .packages
            .iter()
            .any(|package| package.filename.starts_with(forbidden))
        {
            bail!(
                "bundle {} contains transitional artifact prefix {} in packages[]",
                bundle_path.display(),
                forbidden
            );
        }
    }
    if bundle
        .ancillary
        .iter()
        .any(|artifact| artifact.filename.starts_with("data_"))
    {
        bail!(
            "bundle {} still publishes data zip in ancillary[]",
            bundle_path.display()
        );
    }
    if bundle
        .ancillary
        .iter()
        .any(|artifact| artifact.filename.starts_with("catalog_"))
    {
        bail!(
            "bundle {} still publishes catalog in ancillary[]",
            bundle_path.display()
        );
    }
    if bundle
        .ancillary
        .iter()
        .any(|artifact| artifact.filename.starts_with("resource_index_"))
    {
        bail!(
            "bundle {} still publishes resource_index in ancillary[]",
            bundle_path.display()
        );
    }
    for forbidden in ["nav_kv_"] {
        if bundle
            .ancillary
            .iter()
            .any(|artifact| artifact.filename.starts_with(forbidden))
        {
            bail!(
                "bundle {} contains unpacked-only artifact prefix {} in ancillary[]",
                bundle_path.display(),
                forbidden
            );
        }
    }
    Ok(())
}

pub(super) fn validate_embedded_sha256_filename(
    filename: &str,
    checksum_sha256: &str,
) -> anyhow::Result<()> {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| anyhow::anyhow!("filename has no stem: {filename}"))?;
    if let Some(suffix) = stem.rsplit('_').next() {
        if suffix.len() == 64 && suffix.chars().all(|ch| ch.is_ascii_hexdigit()) {
            if suffix != checksum_sha256 {
                bail!(
                    "embedded sha256 mismatch for {filename}: filename {suffix} != checksum {checksum_sha256}"
                );
            }
        }
    }
    Ok(())
}

pub(super) fn validate_public_filename(value: &str, field: &str) -> anyhow::Result<()> {
    if value
        != Path::new(value)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
    {
        bail!("{field} must be a basename, got {value}");
    }
    if value.contains('/') || value.contains('\\') {
        bail!("{field} must not contain path separators: {value}");
    }
    Ok(())
}

pub(super) fn validate_public_package_member(value: &str, field: &str) -> anyhow::Result<()> {
    if value.is_empty() {
        bail!("{field} must not be empty");
    }
    if value.starts_with('/') || value.contains('\\') {
        bail!("{field} must be a relative public member path, got {value}");
    }
    for component in value.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            bail!("{field} has invalid path component in {value}");
        }
    }
    Ok(())
}

pub(super) fn ensure_public_file_exists(path: &Path) -> anyhow::Result<()> {
    let meta =
        fs::metadata(path).with_context(|| format!("missing published file {}", path.display()))?;
    if !meta.is_file() {
        bail!(
            "expected published file, found non-file at {}",
            path.display()
        );
    }
    Ok(())
}

pub(super) fn ensure_public_dir_exists(path: &Path) -> anyhow::Result<()> {
    let meta =
        fs::metadata(path).with_context(|| format!("missing published dir {}", path.display()))?;
    if !meta.is_dir() {
        bail!(
            "expected published dir, found non-dir at {}",
            path.display()
        );
    }
    Ok(())
}

pub(super) fn zip_stem(filename: &str) -> anyhow::Result<String> {
    let path = Path::new(filename);
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default();
    if extension != "zip" {
        bail!("expected zip filename, got {filename}");
    }
    Ok(path
        .file_stem()
        .and_then(|name| name.to_str())
        .context("zip filename missing stem")?
        .to_string())
}

pub(super) fn validate_no_internal_paths_in_json(path: &Path) -> anyhow::Result<()> {
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))?;
    validate_no_internal_paths_in_value(path, "$", &value)
}

pub(super) fn validate_no_internal_paths_in_value(
    path: &Path,
    json_path: &str,
    value: &serde_json::Value,
) -> anyhow::Result<()> {
    match value {
        serde_json::Value::String(text) => {
            for forbidden in [
                "cache/",
                "locks/",
                "logs/",
                "private-work/",
                "scratch/",
                "state/",
                "worktrees/",
                "work/",
                "published_packaged/production",
            ] {
                if text.contains(forbidden) {
                    bail!(
                        "{} contains forbidden internal path fragment at {}: {}",
                        path.display(),
                        json_path,
                        text
                    );
                }
            }
            Ok(())
        }
        serde_json::Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                validate_no_internal_paths_in_value(path, &format!("{json_path}[{index}]"), item)?;
            }
            Ok(())
        }
        serde_json::Value::Object(map) => {
            for (key, item) in map {
                validate_no_internal_paths_in_value(path, &format!("{json_path}.{key}"), item)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub(super) fn bundle_artifact(
    absolute_path: &Path,
    published_filename: &str,
) -> anyhow::Result<BundleArtifact> {
    Ok(BundleArtifact {
        filename: published_filename.to_string(),
        relative_path: published_filename.to_string(),
        checksum_sha256: hash_file(absolute_path)?,
        size_bytes: fs::metadata(absolute_path)
            .with_context(|| format!("failed to stat {}", absolute_path.display()))?
            .len(),
    })
}

pub(super) fn write_hashed_bundle_manifest(
    build_root: &Path,
    bundle_manifest: &BundleManifest,
) -> anyhow::Result<PathBuf> {
    let bytes =
        serde_json::to_vec_pretty(bundle_manifest).context("failed to encode bundle manifest")?;
    let sha256 = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let bundle_manifest_path = build_root.join(format!(
        "bundle_cycle_{}_{}_{sha256}.json",
        bundle_manifest.cycle, bundle_manifest.cycle_version
    ));
    write_public_json_atomic(&bundle_manifest_path, &bytes)?;
    Ok(bundle_manifest_path)
}

pub(super) fn write_public_json_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, bytes)
        .with_context(|| format!("failed to write {}", temp_path.display()))?;
    fs::rename(&temp_path, path).with_context(|| {
        format!(
            "failed to rename {} to {}",
            temp_path.display(),
            path.display()
        )
    })
}

pub(super) fn publish_bundle_artifact(
    config: &ProductBuildConfig,
    absolute_path: &Path,
    published_filename: &str,
) -> anyhow::Result<BundleArtifact> {
    let published_path = config.packaged_dir.join(published_filename);
    publish_flat_artifact(absolute_path, &published_path)?;
    bundle_artifact(absolute_path, published_filename)
}

pub(super) fn publish_flat_artifact(
    source_path: &Path,
    published_path: &Path,
) -> anyhow::Result<()> {
    if published_path.exists() {
        fs::remove_file(published_path)
            .with_context(|| format!("failed to remove {}", published_path.display()))?;
    }
    fs::hard_link(source_path, published_path).with_context(|| {
        format!(
            "failed to hardlink {} to {}",
            source_path.display(),
            published_path.display()
        )
    })?;
    Ok(())
}

pub(super) fn canonical_package_filename(
    family_id: &str,
    region_id: &str,
    original_filename: &str,
) -> anyhow::Result<String> {
    let cycle = package_version_from_filename(original_filename)?;
    let contract_id = product_contract_id_for_family(family_id)?;
    Ok(format!(
        "{}_{}_{}_{}.zip",
        family_id.replace('-', "_"),
        region_id.to_ascii_lowercase(),
        contract_id,
        cycle
    ))
}

pub(super) fn canonical_package_filename_hashed(
    family_id: &str,
    region_id: &str,
    original_filename: &str,
    checksum_sha256: &str,
    tier: Option<ChartPackageTier>,
) -> anyhow::Result<String> {
    let cycle = package_version_from_filename(original_filename)?;
    let contract_id = product_contract_id_for_family(family_id)?;
    let tier_token = match tier {
        Some(ChartPackageTier::Detail) => "_detail",
        Some(ChartPackageTier::Wide | ChartPackageTier::Regional) | None => "",
    };
    Ok(format!(
        "{}_{}{}_{}_{}_{}_{}.zip",
        family_id.replace('-', "_"),
        region_id.to_ascii_lowercase(),
        tier_token,
        contract_id,
        cycle,
        PACKAGE_CYCLE_VERSION,
        checksum_sha256
    ))
}

pub(super) fn package_version_from_filename(original_filename: &str) -> anyhow::Result<String> {
    Path::new(original_filename)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.rsplit('_').next())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            anyhow::anyhow!("failed to derive cycle from package filename {original_filename}")
        })
}
