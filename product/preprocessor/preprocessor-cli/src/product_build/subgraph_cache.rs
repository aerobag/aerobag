// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Completed TPP regions can bypass graph expansion. The snapshot retains all
//! child records, so existing build manifests and GC see exactly the same roots
//! without re-running each child's cache lookup, hashing, thread, and logging.

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TppSubgraphSnapshot {
    package: NodeRecord,
    children: Vec<NodeRecord>,
}

#[derive(Debug, Clone)]
pub(super) struct CachedTppSubgraph {
    pub record: NodeRecord,
    snapshot: Arc<TppSubgraphSnapshot>,
}

impl CachedTppSubgraph {
    pub fn result(
        &self,
        config: &ProductBuildConfig,
        source_urls: &Path,
    ) -> anyhow::Result<(Vec<NodeRecord>, AssetSource, String)> {
        let package = &self.snapshot.package;
        let path =
            |key| Ok::<_, anyhow::Error>(resolve_artifact_path(config, output_path(package, key)?));
        let source = AssetSource {
            package_outputs_path: path("package_outputs")?,
            asset_root: path("metadata_root")?,
            package_root: path("package_root")?,
            unpack_source_root: path("unpack_source_root")?,
            source_urls_path: Some(source_urls.to_path_buf()),
        };
        let mut records = self.snapshot.children.clone();
        records.push(package.clone());
        records.push(self.record.clone());
        for record in &mut records {
            record.cache_hit = true;
        }
        Ok((records, source, package.fingerprint.clone()))
    }
}

pub(super) fn tpp_subgraph_inputs(
    config: &ProductBuildConfig,
    region: Region,
    source_urls: &Path,
    fetch: &NodeRecord,
    version: &str,
) -> anyhow::Result<BTreeMap<String, String>> {
    let region_id = region.code().to_ascii_lowercase();
    let mut inputs = tpp_plan_inputs(
        source_urls,
        &region_id,
        config.fetch_jobs,
        Some(tpp_source_content_fingerprint(fetch)?),
    )?;
    inputs.insert("version".into(), version.into());
    inputs.insert(
        "contract".into(),
        product_contract_id_for_family("tpp")?.into(),
    );
    inputs.insert("subgraph_recipe".into(), tpp_subgraph_recipe()?);
    Ok(inputs)
}

pub(super) fn tpp_subgraph_recipe() -> anyhow::Result<String> {
    static RECIPE: OnceLock<Result<String, String>> = OnceLock::new();
    RECIPE
        .get_or_init(|| compute_tpp_subgraph_recipe().map_err(|e| format!("{e:#}")))
        .clone()
        .map_err(anyhow::Error::msg)
}

fn compute_tpp_subgraph_recipe() -> anyhow::Result<String> {
    // Cover descendants' implementation, not just the planner's recipe.
    // A recipe miss still reuses every unchanged fine-grained child.
    let workspace = preprocessor_resources::path("product/preprocessor");
    let mut inputs = vec![
        hash_text(include_str!("cycle_nodes.rs")),
        hash_text(include_str!("cycle.rs")),
        hash_text(include_str!("product.rs")),
        hash_text(include_str!("subgraph_cache.rs")),
        hash_text(include_str!("../product_build.rs")),
        hash_text(include_str!("../../Cargo.toml")),
        hash_text(include_str!("../../../Cargo.lock")),
    ];
    for name in [
        "preprocessor-tpp",
        "preprocessor-tools",
        "preprocessor-core",
        "preprocessor-zip",
    ] {
        inputs.push(crate::compiled_sources::tree(&format!(
            "product/preprocessor/{name}/src"
        ))?);
    }
    inputs.push(tpp_script_fingerprint(
        &workspace.join("preprocessor-tpp/scripts"),
    )?);
    let python_environment = concat!(
        "import sys,importlib.metadata as m; print(sys.version); ",
        "print(sorted((d.metadata['Name'],d.version) for d in m.distributions()))"
    );
    for args in [
        vec!["gs", "--version"],
        vec!["gdalinfo", "--version"],
        vec!["mogrify", "-version"],
        vec!["exiftool", "-ver"],
        vec!["zip", "-v"],
        vec!["python3", "-c", python_environment],
    ] {
        let result = Command::new(args[0]).args(&args[1..]).output()?;
        if !result.status.success() {
            bail!("cannot identify TPP tool {}", args[0]);
        }
        inputs.push(hash_text(&String::from_utf8(result.stdout)?));
        inputs.push(hash_text(&String::from_utf8(result.stderr)?));
    }
    Ok(hash_text(&serde_json::to_string(&inputs)?))
}

fn tpp_script_fingerprint(root: &Path) -> anyhow::Result<String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    let mut inputs = BTreeMap::new();
    for (relative, path) in files {
        // Importing Python scripts must not change the recipe we are running.
        if Path::new(&relative)
            .components()
            .any(|part| part.as_os_str() == "__pycache__")
            || Path::new(&relative)
                .extension()
                .is_some_and(|ext| ext == "pyc")
        {
            continue;
        }
        inputs.insert(relative, hash_file(&path)?);
    }
    Ok(hash_text(&serde_json::to_string(&inputs)?))
}

fn prepare_tpp_subgraph(
    config: &ProductBuildConfig,
    region: Region,
    inputs: &BTreeMap<String, String>,
) -> anyhow::Result<PreparedNode> {
    let name = format!("tpp-{}-subgraph", region.code().to_ascii_lowercase());
    prepare_node_at(&build_shared_node_dir(config, &name)?, &name, inputs)
}

pub(super) fn lookup_tpp_subgraph(
    config: &ProductBuildConfig,
    region: Region,
    inputs: &BTreeMap<String, String>,
) -> anyhow::Result<Option<CachedTppSubgraph>> {
    let prepared = prepare_tpp_subgraph(config, region, inputs)?;
    let snapshot_path = prepared.dir.join("snapshot.json");
    let Some(record) = try_load_node_record(&prepared, std::slice::from_ref(&snapshot_path))?
    else {
        return Ok(None);
    };
    let snapshot: TppSubgraphSnapshot = serde_json::from_slice(&fs::read(snapshot_path)?)?;
    // Only the region's externally consumed outputs are needed on a hit. The
    // retained child records protect leaf caches; GC remains responsible for
    // their closure, rather than every publication restatting all 91k leaves.
    for key in [
        "metadata_root",
        "package_root",
        "unpack_source_root",
        "package_outputs",
        "zip",
        "manifest",
    ] {
        if !resolve_artifact_path(config, output_path(&snapshot.package, key)?).exists() {
            return Ok(None);
        }
    }
    Ok(Some(CachedTppSubgraph {
        record,
        snapshot: Arc::new(snapshot),
    }))
}

pub(super) fn save_tpp_subgraph(
    config: &ProductBuildConfig,
    region: Region,
    inputs: BTreeMap<String, String>,
    package: &NodeRecord,
    task_records: &BTreeMap<String, Vec<NodeRecord>>,
) -> anyhow::Result<NodeRecord> {
    let prepared = prepare_tpp_subgraph(config, region, &inputs)?;
    let snapshot_path = prepared.dir.join("snapshot.json");
    // Use the existing per-node ownership protocol. Missing package output must
    // also invalidate the snapshot, so rebuilding it can repair partial loss.
    let mut expected = vec![snapshot_path.clone()];
    for key in [
        "metadata_root",
        "package_root",
        "unpack_source_root",
        "package_outputs",
        "zip",
        "manifest",
    ] {
        expected.push(resolve_artifact_path(config, output_path(package, key)?));
    }
    let prefix = format!("tpp-{}-", region.code().to_ascii_lowercase());
    run_cached_node(prepared, inputs, &expected, |_| {
        let mut children = BTreeMap::new();
        for (task_id, records) in task_records {
            if !task_id.starts_with(&prefix) {
                continue;
            }
            for record in records {
                if record.name.ends_with("-subgraph") {
                    continue;
                }
                children.insert(
                    (record.name.clone(), record.fingerprint.clone()),
                    record.clone(),
                );
            }
        }
        let snapshot = TppSubgraphSnapshot {
            package: package.clone(),
            children: children.into_values().collect(),
        };
        fs::write(&snapshot_path, serde_json::to_vec(&snapshot)?)?;
        Ok(BTreeMap::from([(
            "snapshot".into(),
            relative_artifact_path(&snapshot_path, &config.build_root),
        )]))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn config(root: &Path) -> ProductBuildConfig {
        ProductBuildConfig {
            chart_metadata_root: root.join("metadata"),
            build_root: root.into(),
            publish_dir: root.join("published/test/now"),
            packaged_dir: root.join("published/test/now/packaged"),
            publish_label: "test".into(),
            publish_timestamp: "now".into(),
            target_cycle: Some("2609".into()),
            fetch_jobs: 1,
            cpu_jobs: 1,
            max_heavy_jobs: 1,
            fetch_cache_root: root.join("cache/fetch"),
            fetch_cache_mode: "cache-first".into(),
        }
    }

    fn leaf(config: &ProductBuildConfig, id: &str) -> NodeRecord {
        let inputs = BTreeMap::from([("id".into(), id.into())]);
        let name = "tpp-nw-render-unit";
        let prepared =
            prepare_node_at(&build_shared_node_dir(config, name).unwrap(), name, &inputs).unwrap();
        let path = prepared.dir.join("plate.png");
        run_cached_node(prepared, inputs, std::slice::from_ref(&path), |_| {
            fs::write(&path, id)?;
            Ok(BTreeMap::from([(
                "plate".into(),
                relative_artifact_path(&path, &config.build_root),
            )]))
        })
        .unwrap()
    }

    fn package(config: &ProductBuildConfig) -> NodeRecord {
        let name = "tpp-nw-package";
        let inputs = BTreeMap::from([("version".into(), "2609".into())]);
        let prepared =
            prepare_node_at(&build_shared_node_dir(config, name).unwrap(), name, &inputs).unwrap();
        let root = prepared.dir.clone();
        run_cached_node(prepared, inputs, &[root.join("zip")], |_| {
            let mut outputs = BTreeMap::new();
            for key in [
                "metadata_root",
                "package_root",
                "unpack_source_root",
                "package_outputs",
                "zip",
                "manifest",
            ] {
                let path = root.join(key);
                if key.ends_with("root") {
                    fs::create_dir_all(&path)?;
                } else {
                    fs::write(&path, "published")?;
                }
                outputs.insert(
                    key.into(),
                    relative_artifact_path(&path, &config.build_root),
                );
            }
            Ok(outputs)
        })
        .unwrap()
    }

    #[test]
    fn warm_subgraph_reuses_all_child_records_without_reading_leaf_cache_records() {
        let root = tempdir().unwrap();
        let config = config(root.path());
        let inputs = BTreeMap::from([("recipe".into(), "v1".into())]);
        let children = BTreeMap::from([
            ("tpp-nw-first".into(), vec![leaf(&config, "a")]),
            ("tpp-nw-second".into(), vec![leaf(&config, "b")]),
            ("tpp-se-unrelated".into(), vec![leaf(&config, "unrelated")]),
        ]);
        let package = package(&config);
        let record =
            save_tpp_subgraph(&config, Region::Nw, inputs.clone(), &package, &children).unwrap();
        assert!(!record.cache_hit);
        // A completed parent needs its published outputs, not the child lookup
        // files. The dependency snapshots still preserve all child GC roots.
        for key in ["tpp-nw-first", "tpp-nw-second"] {
            let child = &children[key][0];
            let dir = config
                .build_root
                .join("cache/nodes")
                .join(&child.name)
                .join(&child.fingerprint);
            set_tree_readonly(&dir, false).unwrap();
            fs::remove_file(dir.join("build-record.json")).unwrap();
        }
        let cached = lookup_tpp_subgraph(&config, Region::Nw, &inputs)
            .unwrap()
            .unwrap();
        let (records, source, fingerprint) = cached
            .result(&config, Path::new("current/source_urls.jsonl"))
            .unwrap();
        assert_eq!(records.len(), 4);
        assert!(records.iter().all(|r| r.cache_hit));
        assert_eq!(fingerprint, package.fingerprint);
        assert_eq!(
            source.source_urls_path.unwrap(),
            Path::new("current/source_urls.jsonl")
        );
        let tasks = BTreeMap::from([("tpp-nw-package".into(), records)]);
        record_gc_roots(&config, "test", &tasks).unwrap();
        let roots = load_gc_roots(&gc_roots_path(&config), &config).unwrap();
        assert_eq!(roots.node_roots.len(), 4);
        let gc = gc_build_cache(&BuildCacheGcConfig {
            build_root: config.build_root.clone(),
            mode: BuildCacheGcMode::Execute,
            grace_hours: 0,
            bootstrap_from_build_manifests: false,
        })
        .unwrap();
        assert_eq!(
            gc.evictable_nodes, 1,
            "only the unrelated region's unrooted node is disposable"
        );
        for key in ["tpp-nw-first", "tpp-nw-second"] {
            assert!(resolve_artifact_path(&config, &children[key][0].outputs["plate"]).is_file());
        }
        assert!(lookup_tpp_subgraph(&config, Region::Nw, &inputs)
            .unwrap()
            .is_some());
    }

    #[test]
    fn input_changes_miss_parent_but_keep_fine_grained_hits() {
        let root = tempdir().unwrap();
        let config = config(root.path());
        let inputs = BTreeMap::from([
            ("source".into(), "a".into()),
            ("recipe".into(), "v1".into()),
        ]);
        let children = BTreeMap::from([("tpp-nw-first".into(), vec![leaf(&config, "a")])]);
        save_tpp_subgraph(
            &config,
            Region::Nw,
            inputs.clone(),
            &package(&config),
            &children,
        )
        .unwrap();
        for key in ["source", "recipe", "contract", "version"] {
            let mut changed = inputs.clone();
            changed.insert(key.into(), "new".into());
            assert!(lookup_tpp_subgraph(&config, Region::Nw, &changed)
                .unwrap()
                .is_none());
            assert!(
                leaf(&config, "a").cache_hit,
                "parent miss must not discard valid children"
            );
        }
        assert!(lookup_tpp_subgraph(&config, Region::Se, &inputs)
            .unwrap()
            .is_none());
    }

    #[test]
    fn missing_published_output_requires_expansion_and_can_be_repaired() {
        let root = tempdir().unwrap();
        let config = config(root.path());
        let inputs = BTreeMap::from([("recipe".into(), "v1".into())]);
        let children = BTreeMap::from([("tpp-nw-first".into(), vec![leaf(&config, "a")])]);
        let package = package(&config);
        save_tpp_subgraph(&config, Region::Nw, inputs.clone(), &package, &children).unwrap();
        let zip = resolve_artifact_path(&config, &package.outputs["zip"]);
        // Test-owned immutable cache directory; simulate an out-of-band loss.
        set_tree_readonly(zip.parent().unwrap(), false).unwrap();
        fs::remove_file(&zip).unwrap();
        assert!(lookup_tpp_subgraph(&config, Region::Nw, &inputs)
            .unwrap()
            .is_none());
        assert!(leaf(&config, "a").cache_hit);
        fs::write(zip, "repaired").unwrap();
        save_tpp_subgraph(&config, Region::Nw, inputs.clone(), &package, &children).unwrap();
        assert!(lookup_tpp_subgraph(&config, Region::Nw, &inputs)
            .unwrap()
            .is_some());
    }

    #[test]
    fn parent_recipe_covers_runtime_tools_and_all_descendant_sources() {
        assert_eq!(tpp_subgraph_recipe().unwrap().len(), 64);
        let root = tempdir().unwrap();
        let config = config(root.path());
        let urls = root.path().join("urls");
        fs::write(&urls, "source").unwrap();
        let mut fetch = leaf(&config, "fetch");
        fetch
            .outputs
            .insert("source_content_fingerprint".into(), "a".into());
        let before = tpp_subgraph_inputs(&config, Region::Nw, &urls, &fetch, "2609_01").unwrap();
        fetch
            .outputs
            .insert("source_content_fingerprint".into(), "b".into());
        assert_ne!(
            before,
            tpp_subgraph_inputs(&config, Region::Nw, &urls, &fetch, "2609_01").unwrap()
        );
        assert!(before.contains_key("subgraph_recipe"));
        assert!(before.contains_key("contract"));
    }

    #[test]
    fn python_imports_do_not_invalidate_script_recipe_but_source_edits_do() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("render.py"), "original").unwrap();
        let original = tpp_script_fingerprint(root.path()).unwrap();
        fs::create_dir(root.path().join("__pycache__")).unwrap();
        fs::write(root.path().join("__pycache__/render.pyc"), "bytecode").unwrap();
        assert_eq!(original, tpp_script_fingerprint(root.path()).unwrap());
        fs::write(root.path().join("render.py"), "changed").unwrap();
        assert_ne!(original, tpp_script_fingerprint(root.path()).unwrap());
    }

    #[test]
    fn real_tpp_region_cold_build_then_lazy_hit_and_fine_grained_fallback() {
        let root = tempdir().unwrap();
        let config = config(root.path());
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("avare_aptdiags.php"), "").unwrap();
        fs::write(source.join("d-TPP_Metafile.xml"), r#"
<digital_tpp><state_code ID="WA"><city_name ID="SEATTLE"><airport_name apt_ident="SEA">
<record><chart_code>HOT</chart_code><chart_name>HOT SPOTS</chart_name><pdf_name>TEST.PDF</pdf_name></record>
</airport_name></city_name></state_code></digital_tpp>"#).unwrap();
        let output = Command::new("gs")
            .args(["-q", "-dBATCH", "-dNOPAUSE", "-sDEVICE=pdfwrite"])
            .arg(format!("-sOutputFile={}", source.join("TEST.PDF").display()))
            .args(["-c", "<< /PageSize [300 400] >> setpagedevice /Helvetica findfont 12 scalefont setfont 40 200 moveto (TPP cache fixture) show showpage"])
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let urls = source.join("source_urls.jsonl");
        fs::write(&urls, "").unwrap();
        let mut fetch = leaf(&config, "fetch");
        fetch.outputs.insert(
            "source_root".into(),
            relative_artifact_path(&source, &config.build_root),
        );
        fetch.outputs.insert(
            "source_content_fingerprint".into(),
            hash_tree(&source).unwrap(),
        );
        let inputs = tpp_subgraph_inputs(&config, Region::Nw, &urls, &fetch, "2609_01").unwrap();
        assert!(lookup_tpp_subgraph(&config, Region::Nw, &inputs)
            .unwrap()
            .is_none());
        let (planner, source, plan, content) =
            build_tpp_plan_node(&config, Region::Nw, &urls, 1, Some(&fetch)).unwrap();
        assert_eq!(plan.units().len(), 1);
        let cold_tasks = cycle::tpp_tasks_for_plan("tpp-nw-plan", Region::Nw, Some(&plan));
        assert_eq!(
            cold_tasks.len(),
            3,
            "render unit, assemble, package planner"
        );
        assert!(matches!(
            cold_tasks[0].kind,
            ScheduledTaskKind::TppRenderUnit { .. }
        ));
        assert_eq!(cold_tasks[2].deps, vec![cold_tasks[1].id.clone()]);
        let unit = &plan.units()[0];
        let rendered = build_tpp_render_unit_node(&config, "nw", &content, &source, unit).unwrap();
        let assembled = build_tpp_render_assemble_node(
            &config,
            Region::Nw,
            &planner,
            std::slice::from_ref(&rendered),
        )
        .unwrap();
        let (package_plan, metadata, plates, package_recipe) =
            build_tpp_package_plan_node(&config, Region::Nw, &urls, "2609_01", &assembled).unwrap();
        assert!(!package_recipe.thumbnails.is_empty());
        let thumbnails: Vec<_> = package_recipe
            .thumbnails
            .iter()
            .map(|thumbnail| {
                build_tpp_thumbnail_node(
                    &config,
                    Region::Nw,
                    &plates[&thumbnail.asset_path],
                    thumbnail,
                )
                .unwrap()
            })
            .collect();
        let (package, asset_source) = build_tpp_package_assemble_node(TppPackageAssembleInput {
            config: &config,
            region: Region::Nw,
            source_urls_path: &urls,
            plan_record: &package_plan,
            metadata_root: &metadata,
            plate_sources: &plates,
            plan: &package_recipe,
            thumbnail_records: &thumbnails,
        })
        .unwrap();
        let children = BTreeMap::from([
            ("tpp-nw-plan".into(), vec![planner]),
            (tpp_render_unit_task_name(Region::Nw, unit), vec![rendered]),
            (tpp_render_assemble_task_name(Region::Nw), vec![assembled]),
            (tpp_package_plan_task_name(Region::Nw), vec![package_plan]),
            ("tpp-nw-thumbnails".into(), thumbnails),
        ]);
        save_tpp_subgraph(&config, Region::Nw, inputs.clone(), &package, &children).unwrap();
        let cached = lookup_tpp_subgraph(&config, Region::Nw, &inputs)
            .unwrap()
            .unwrap();
        let warm_tasks = cycle::tpp_tasks_for_plan("tpp-nw-plan", Region::Nw, None);
        assert_eq!(
            warm_tasks.len(),
            1,
            "no render, assemble, or thumbnail expansion on a hit"
        );
        assert!(matches!(
            warm_tasks[0].kind,
            ScheduledTaskKind::TppCachedPackage { .. }
        ));
        assert_eq!(warm_tasks[0].id, "tpp-nw-package");
        assert_eq!(warm_tasks[0].deps, vec!["tpp-nw-plan"]);
        let (records, warm_source, fingerprint) = cached.result(&config, &urls).unwrap();
        assert_eq!(fingerprint, package.fingerprint);
        assert_eq!(warm_source.package_root, asset_source.package_root);
        assert_eq!(
            warm_source.unpack_source_root,
            asset_source.unpack_source_root
        );
        assert_eq!(
            records.len(),
            children.values().map(Vec::len).sum::<usize>() + 2
        );
        let changed = tpp_subgraph_inputs(&config, Region::Nw, &urls, &fetch, "2609_02").unwrap();
        assert!(lookup_tpp_subgraph(&config, Region::Nw, &changed)
            .unwrap()
            .is_none());
        assert!(
            build_tpp_render_unit_node(&config, "nw", &content, &source, unit)
                .unwrap()
                .cache_hit,
            "a parent version miss must retain the real rendered plate"
        );
    }
}
