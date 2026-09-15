// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

#[derive(Debug, Deserialize)]
struct ChartQualityReport {
    schema_version: u32,
    family: String,
    cycle: String,
    source_id: String,
    publication: PublicationDecision,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
enum PublicationDecision {
    Ready(ChartPublicationInputs),
    Blocked { reason: String },
}

/// The checker owns source/metadata dependencies. Rendering receives only this
/// explicit input exclusion; no renderer may guess where a moved inset went.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChartPublicationInputs {
    pub(super) has_map_sources: bool,
    quarantined_sources: Vec<String>,
    excluded_metadata: Vec<PathBuf>,
}

impl ChartPublicationInputs {
    pub(super) fn fingerprint(&self) -> anyhow::Result<String> {
        Ok(hash_text(&serde_json::to_string(self)?))
    }

    pub(super) fn apply(&self, work_dir: &Path) -> anyhow::Result<()> {
        use std::path::Component;
        for source in &self.quarantined_sources {
            let path = Path::new(source);
            if path.components().count() != 1
                || !matches!(path.components().next(), Some(Component::Normal(_)))
                || path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_none_or(|ext| !ext.eq_ignore_ascii_case("tif"))
            {
                bail!("invalid quarantined chart source {source:?}");
            }
        }
        for path in &self.excluded_metadata {
            let parts: Vec<_> = path.components().collect();
            if parts.len() != 2
                || !parts
                    .iter()
                    .all(|part| matches!(part, Component::Normal(_)))
                || !matches!(
                    parts[0].as_os_str().to_str(),
                    Some("SEC" | "TAC" | "FLY" | "ENR_L" | "ENR_H")
                )
            {
                bail!("invalid excluded chart metadata {}", path.display());
            }
        }
        // Only the fresh, disposable node work tree is changed. Authored metadata
        // and hard-linked source-cache contents remain untouched.
        for path in &self.excluded_metadata {
            fs::remove_file(work_dir.join(path))
                .with_context(|| format!("failed to exclude {}", path.display()))?;
        }
        for entry in fs::read_dir(work_dir)? {
            let entry = entry?;
            if self.quarantined_sources.iter().any(|source| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(source)
            }) {
                fs::remove_file(entry.path())?;
            }
        }
        fs::write(
            work_dir.join("chart-publication-inputs.json"),
            serde_json::to_vec_pretty(self)?,
        )?;
        if !self.quarantined_sources.is_empty() {
            eprintln!(
                "CRITICAL chart source quarantine: {}; publishing only remaining sources",
                self.quarantined_sources.join(", ")
            );
        }
        Ok(())
    }
}

pub(super) fn check_chart_visual_references(
    config: &ProductBuildConfig,
    family: ChartFamily,
    source_root: &Path,
    supplemental: Option<&NodeRecord>,
    source_id: &str,
) -> anyhow::Result<(NodeRecord, ChartPublicationInputs)> {
    let cycle = config
        .target_cycle
        .as_deref()
        .context("chart quality requires a target cycle")?;
    let mut roots = vec![source_root.to_path_buf()];
    if let Some(record) = supplemental {
        roots.push(resolve_artifact_path(
            config,
            output_path(record, "source_root")?,
        ));
    }
    let inputs = chart_quality_inputs(config, family, cycle, source_id, &roots)?;
    let name = format!("charts-{}-quality", family_slug(family));
    let prepared = prepare_node_at(&build_shared_node_dir(config, &name)?, &name, &inputs)?;
    let verdict_root = prepared.dir.join("verdict");
    let record = run_cached_node(
        prepared,
        inputs,
        &[
            verdict_root.join("current.json"),
            verdict_root.join("reports"),
        ],
        |_| {
            run_chart_visual_references(
                config,
                family,
                source_root,
                supplemental,
                source_id,
                &verdict_root,
            )?;
            Ok(BTreeMap::from([(
                "verdict".to_string(),
                relative_artifact_path(&verdict_root, &config.build_root),
            )]))
        },
    )?;
    let report: ChartQualityReport =
        serde_json::from_slice(&fs::read(verdict_root.join("current.json"))?)?;
    if report.schema_version != 2
        || report.family != manifest_chart_name(family)
        || report.cycle != cycle
        || report.source_id != source_id
    {
        bail!("cached chart verdict does not match this build");
    }
    if record.cache_hit {
        publish_chart_verdict(config, family, &verdict_root)?;
    }
    match report.publication {
        PublicationDecision::Ready(inputs) => Ok((record, inputs)),
        PublicationDecision::Blocked { reason } => bail!("chart publication blocked: {reason}"),
    }
}

fn chart_quality_inputs(
    config: &ProductBuildConfig,
    family: ChartFamily,
    cycle: &str,
    source_id: &str,
    roots: &[PathBuf],
) -> anyhow::Result<BTreeMap<String, String>> {
    // The fetched content identity is supplemented with actual file identities:
    // replacing a source behind an unchanged fetch record must invalidate the
    // verdict. This uses the same identity as artifact_verification: ctime is
    // deliberately excluded because creating/removing render hardlinks changes
    // ctime without changing pixels. This stats sources, not gigabytes of data.
    let mut identities = Vec::new();
    for (overlay, root) in roots.iter().enumerate() {
        let mut files = Vec::new();
        collect_files(root, root, &mut files)?;
        for (relative, path) in files {
            let stat = fs::metadata(path)?;
            identities.push(serde_json::json!([
                overlay,
                relative,
                stat.dev(),
                stat.ino(),
                stat.len(),
                stat.mtime(),
                stat.mtime_nsec(),
            ]));
        }
    }
    let versions = Command::new("python3").args(["-c", "import sys,numpy; from osgeo import gdal; print(sys.version); print(numpy.__version__); print(gdal.VersionInfo('--version'))"]).output()?;
    if !versions.status.success() {
        bail!(
            "cannot identify chart verification tools: {}",
            String::from_utf8_lossy(&versions.stderr)
        );
    }
    Ok(BTreeMap::from([
        ("family".into(), manifest_chart_name(family).into()),
        ("cycle".into(), cycle.into()),
        ("source_content".into(), source_id.into()),
        (
            "source_files".into(),
            hash_text(&serde_json::to_string(&identities)?),
        ),
        (
            "metadata_and_approvals".into(),
            hash_tree(&config.chart_metadata_root)?,
        ),
        (
            "checker".into(),
            hash_text(include_str!(
                "../../../preprocessor-charts/chart_quality.py"
            )),
        ),
        (
            "cutlines".into(),
            hash_text(include_str!(
                "../../../preprocessor-charts/chart_cutlines.py"
            )),
        ),
        (
            "tools".into(),
            hash_text(&String::from_utf8(versions.stdout)?),
        ),
    ]))
}

fn publish_chart_verdict(
    config: &ProductBuildConfig,
    family: ChartFamily,
    verdict_root: &Path,
) -> anyhow::Result<()> {
    let result = Command::new("python3")
        .args([
            "-c",
            include_str!("../../../preprocessor-charts/chart_quality.py"),
            "--publish-cached",
        ])
        .arg(verdict_root)
        .arg("--output")
        .arg(
            config
                .build_root
                .join("state/chart-quality")
                .join(manifest_chart_name(family)),
        )
        .output()?;
    if !result.status.success() {
        bail!(
            "cannot publish chart verdict: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    Ok(())
}

fn run_chart_visual_references(
    config: &ProductBuildConfig,
    family: ChartFamily,
    source_root: &Path,
    supplemental: Option<&NodeRecord>,
    source_id: &str,
    verdict_root: &Path,
) -> anyhow::Result<()> {
    let report_root = config
        .build_root
        .join("state/chart-quality")
        .join(manifest_chart_name(family));
    let cycle = config
        .target_cycle
        .as_deref()
        .context("chart quality requires a target cycle")?;
    // Keep invocation scripts and stdout private to this attempt. Simultaneous
    // release/cycle builds must not consume each other's monitoring pointer.
    let attempt = config.build_root.join("logs/chart-quality").join(format!(
        "{}-{}-{}",
        family_slug(family),
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_nanos()
    ));
    fs::create_dir_all(&attempt)?;
    fs::write(
        attempt.join("chart_cutlines.py"),
        include_str!("../../../preprocessor-charts/chart_cutlines.py"),
    )?;
    let checker = attempt.join("chart_quality.py");
    fs::write(
        &checker,
        include_str!("../../../preprocessor-charts/chart_quality.py"),
    )?;
    let mut args = vec![
        checker.display().to_string(),
        "--source-root".to_string(),
        source_root.display().to_string(),
        "--metadata-root".to_string(),
        config.chart_metadata_root.display().to_string(),
        "--family".to_string(),
        manifest_chart_name(family).to_string(),
        "--cycle".to_string(),
        cycle.to_string(),
        "--source-id".to_string(),
        source_id.to_string(),
        "--output".to_string(),
        verdict_root.display().to_string(),
        "--monitor-output".to_string(),
        report_root.display().to_string(),
    ];
    if let Some(record) = supplemental {
        args.extend([
            "--source-root".to_string(),
            resolve_artifact_path(config, output_path(record, "source_root")?)
                .display()
                .to_string(),
        ]);
    }
    let invocation = preprocessor_tools::ToolInvocation {
        program: "python3".to_string(),
        args,
        cwd: config.chart_metadata_root.clone(),
        label: "check".to_string(),
        env: Vec::new(),
        stdin_text: None,
    };
    let outcome = invocation.run_logged(&attempt)?;
    // The checker publishes this real attempt (including failures) separately
    // from its cache output. Only successful runs become reusable node entries.
    invocation.ensure_success(
        &outcome,
        &format!(
            "Chart visual reference check blocked {}: inspect {}",
            family.capture_label(),
            report_root.display()
        ),
    )?;
    let report: ChartQualityReport = serde_json::from_slice(&fs::read(&outcome.logs.stdout)?)
        .context("invalid chart publication decision")?;
    if report.schema_version != 2
        || report.family != manifest_chart_name(family)
        || report.cycle != cycle
        || report.source_id != source_id
    {
        bail!("chart publication decision does not match this build");
    }
    match report.publication {
        PublicationDecision::Ready(_) => Ok(()),
        PublicationDecision::Blocked { reason } => bail!("chart publication blocked: {reason}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn quarantine_removes_work_links_and_metadata_not_original_inputs() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source.tif");
        fs::write(&source, "original source pixels").unwrap();
        let work = temp.path().join("work");
        fs::create_dir_all(work.join("SEC")).unwrap();
        fs::hard_link(&source, work.join("TEST SEC.TIF")).unwrap();
        fs::write(work.join("Other SEC.tif"), "healthy").unwrap();
        fs::write(work.join("SEC/Test SEC.geojson"), "definition").unwrap();
        let inputs = ChartPublicationInputs {
            has_map_sources: true,
            quarantined_sources: vec!["Test SEC.tif".to_string()],
            excluded_metadata: vec![PathBuf::from("SEC/Test SEC.geojson")],
        };
        inputs.apply(&work).unwrap();
        assert!(!work.join("TEST SEC.TIF").exists());
        assert!(!work.join("SEC/Test SEC.geojson").exists());
        assert_eq!(
            fs::read_to_string(&source).unwrap(),
            "original source pixels"
        );
        assert_eq!(
            fs::read_to_string(work.join("Other SEC.tif")).unwrap(),
            "healthy"
        );
        let saved: ChartPublicationInputs =
            serde_json::from_slice(&fs::read(work.join("chart-publication-inputs.json")).unwrap())
                .unwrap();
        assert_eq!(saved.fingerprint().unwrap(), inputs.fingerprint().unwrap());
    }

    #[test]
    fn publication_decision_requires_explicit_state_and_confined_exclusions() {
        for json in ["{}", r#"{"state":"ready"}"#, r#"{"state":"guessed"}"#] {
            assert!(serde_json::from_str::<PublicationDecision>(json).is_err());
        }
        let temp = tempdir().unwrap();
        for path in [
            "../outside",
            "/tmp/outside",
            "SEC/../../outside",
            "unknown/file",
        ] {
            let inputs = ChartPublicationInputs {
                has_map_sources: false,
                quarantined_sources: Vec::new(),
                excluded_metadata: vec![PathBuf::from(path)],
            };
            assert!(inputs.apply(temp.path()).is_err(), "{path}");
        }
    }
}
