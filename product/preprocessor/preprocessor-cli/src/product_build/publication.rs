use super::*;

pub(super) fn content_product_version_label(source_fingerprint: &str) -> String {
    source_fingerprint.chars().take(16).collect()
}

pub fn publish_discovery_manifest(
    config: &ProductBuildConfig,
    as_of_utc: DateTime<Utc>,
    bundle_filenames: &[String],
) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(&config.build_root)
        .with_context(|| format!("failed to create {}", config.build_root.display()))?;
    if bundle_filenames.is_empty() {
        bail!("publish-discovery-manifest requires at least one --bundle");
    }
    let latest_alias_path = publication_current_artifacts_path(&config.build_root);
    if !latest_alias_path.is_file() {
        bail!(
            "missing current artifacts alias {}; build-product first",
            latest_alias_path.display()
        );
    }
    let bundles = bundle_filenames
        .iter()
        .map(|filename| current_bundle_entry_from_path(&config.build_root.join(filename)))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let manifest = CurrentArtifactsManifest {
        schema_version: 1,
        artifact_roots: default_current_artifact_roots(),
        as_of_date: as_of_utc.date_naive().format("%Y-%m-%d").to_string(),
        as_of_utc: as_of_utc.to_rfc3339_opts(SecondsFormat::Secs, true),
        bundles,
        diagnostics: None,
    };
    write_current_artifacts_aliases(&config.build_root, as_of_utc, &manifest)?;
    let immutable_path = publication_root_for_packaged_root(&config.build_root)
        .join(current_artifacts_immutable_filename(as_of_utc));
    let unpacked_root = published_unpacked_root(config)?;
    fs::create_dir_all(&unpacked_root)
        .with_context(|| format!("failed to create {}", unpacked_root.display()))?;
    sync_unpacked_discovery_manifests(&config.build_root, &latest_alias_path, &unpacked_root)?;
    cleanup_published_packaged_root(&config.build_root, &latest_alias_path)?;
    cleanup_published_unpacked_root(&unpacked_root, &latest_alias_path)?;
    validate_packaged_contract(&config.build_root, &latest_alias_path)?;
    validate_unpacked_contract(&config.build_root, &unpacked_root, &latest_alias_path)?;
    Ok(immutable_path)
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

pub(super) fn current_artifacts_timestamp_string(as_of_utc: DateTime<Utc>) -> String {
    as_of_utc.format("%Y%m%dT%H%M%SZ").to_string()
}

pub(super) fn current_artifacts_immutable_filename(as_of_utc: DateTime<Utc>) -> String {
    format!(
        "current_artifacts_{}.json",
        current_artifacts_timestamp_string(as_of_utc)
    )
}

pub(super) fn current_artifacts_latest_alias_filename() -> &'static str {
    "current_artifacts.json"
}

pub(super) fn write_current_artifacts_json(
    path: &Path,
    manifest: &CurrentArtifactsManifest,
) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(manifest)
        .context("failed to encode current artifacts manifest")?;
    write_public_json_atomic(path, &bytes)
}

pub(super) fn publication_root_for_packaged_root(packaged_root: &Path) -> PathBuf {
    match packaged_root.file_name().and_then(|name| name.to_str()) {
        Some("published_packaged") => packaged_root
            .parent()
            .unwrap_or(packaged_root)
            .to_path_buf(),
        _ => packaged_root.to_path_buf(),
    }
}

pub(super) fn publication_current_artifacts_path(packaged_root: &Path) -> PathBuf {
    publication_root_for_packaged_root(packaged_root)
        .join(current_artifacts_latest_alias_filename())
}

pub(super) fn write_current_artifacts_aliases(
    packaged_root: &Path,
    as_of_utc: DateTime<Utc>,
    manifest: &CurrentArtifactsManifest,
) -> anyhow::Result<PathBuf> {
    let publication_root = publication_root_for_packaged_root(packaged_root);
    fs::create_dir_all(&publication_root)
        .with_context(|| format!("failed to create {}", publication_root.display()))?;

    let immutable_filename = current_artifacts_immutable_filename(as_of_utc);
    let latest_filename = current_artifacts_latest_alias_filename();
    let publication_immutable_path = publication_root.join(&immutable_filename);
    let publication_latest_path = publication_root.join(latest_filename);
    write_current_artifacts_json(&publication_immutable_path, manifest)?;
    write_current_artifacts_json(&publication_latest_path, manifest)?;

    Ok(publication_latest_path)
}

pub(super) fn write_current_artifacts_manifest(
    build_root: &Path,
    as_of_utc: DateTime<Utc>,
    diagnostics: Option<CurrentDiagnosticsEntry>,
) -> anyhow::Result<PathBuf> {
    let as_of_date = as_of_utc.date_naive();
    let bundles = build_current_bundle_entries(build_root, as_of_date)?;
    let manifest = CurrentArtifactsManifest {
        schema_version: 1,
        artifact_roots: default_current_artifact_roots(),
        as_of_date: as_of_date.format("%Y-%m-%d").to_string(),
        as_of_utc: as_of_utc.to_rfc3339_opts(SecondsFormat::Secs, true),
        bundles,
        diagnostics,
    };
    write_current_artifacts_aliases(build_root, as_of_utc, &manifest)
}

pub(super) fn write_build_status_html(
    config: &ProductBuildConfig,
    current_artifacts_path: &Path,
) -> anyhow::Result<()> {
    let status = build_status_document(&config.build_root, current_artifacts_path)?;
    let html = render_build_status_html(&status)?;
    let packaged_path = config.build_root.join("build-status.html");
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
    let mut paths = vec![current_artifacts_path.to_path_buf()];
    let mut seen = BTreeSet::from([current_artifacts_path.to_path_buf()]);
    if current_artifacts_path.parent() != Some(root) {
        return Ok(paths);
    }
    for entry in fs::read_dir(root)
        .with_context(|| format!("failed to read {}", root.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to iterate {}", root.display()))?
    {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let is_discovery = name == current_artifacts_latest_alias_filename()
            || (name.starts_with("current_artifacts_")
                && name.contains('T')
                && name.ends_with(".json"));
        if is_discovery && seen.insert(path.clone()) {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
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
            validate_bundle_manifest(packaged_root, &bundle_path)?;
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
    if current.artifact_roots.packaged != "published_packaged/" {
        bail!(
            "{} has unexpected artifact_roots.packaged {:?}",
            path.display(),
            current.artifact_roots.packaged
        );
    }
    if current.artifact_roots.unpacked != "published_unpacked/" {
        bail!(
            "{} has unexpected artifact_roots.unpacked {:?}",
            path.display(),
            current.artifact_roots.unpacked
        );
    }
    Ok(())
}

pub(super) fn validate_bundle_manifest(
    packaged_root: &Path,
    bundle_path: &Path,
) -> anyhow::Result<()> {
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

    for bundle in &current.bundles {
        let unpacked_bundle_path = unpacked_root.join(&bundle.filename);
        ensure_public_file_exists(&unpacked_bundle_path)?;
        validate_no_internal_paths_in_json(&unpacked_bundle_path)?;
        let bundle = load_bundle_manifest(&unpacked_bundle_path)?;
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
                "private-work/",
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
    let published_path = config.build_root.join(published_filename);
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
) -> anyhow::Result<String> {
    let cycle = package_version_from_filename(original_filename)?;
    let contract_id = product_contract_id_for_family(family_id)?;
    Ok(format!(
        "{}_{}_{}_{}_{}_{}.zip",
        family_id.replace('-', "_"),
        region_id.to_ascii_lowercase(),
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
