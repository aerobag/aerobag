use super::*;

pub fn build_product(config: &ProductBuildConfig) -> anyhow::Result<ProductBuildResult> {
    fs::create_dir_all(&config.packaged_dir)
        .with_context(|| format!("failed to create {}", config.packaged_dir.display()))?;
    let log_root = orchestrator_log_root(config)?;
    let mut master_log = MasterLog::create(&log_root.join("master.log"))?;
    master_log.log(format!(
        "begin pid={} profile={} build_root={} publish_dir={} publish_label={} publish_timestamp={} scheduler=product_weighted_dag scheduler_version=2 fetch_jobs={} cpu_jobs={} max_heavy_jobs={} fetch_cache_mode={}",
        std::process::id(),
        config.profile.as_str(),
        config.build_root.display(),
        config.publish_dir.display(),
        config.publish_label,
        config.publish_timestamp,
        config.fetch_jobs,
        config.cpu_jobs,
        config.max_heavy_jobs,
        config.fetch_cache_mode,
    ))?;
    #[derive(Debug, Clone)]
    enum ProductScheduledTaskKind {
        SourceUrls { cycle: String },
        ChartFetch { cycle: String, family: ChartFamily },
        ChartProcess { cycle: String, family: ChartFamily },
        ChartPackage { cycle: String, family: ChartFamily },
        CsupFetch { cycle: String },
        CsupProcess { cycle: String },
        CsupRender { cycle: String, region: Region },
        CsupPackage { cycle: String },
        TppFetch { cycle: String },
        TppRender { cycle: String, region: Region },
        TppPackage { cycle: String, region: Region },
        DataBase { cycle: String },
        DataMatch { cycle: String },
        Vectors { cycle: String },
        WmmSource,
        ResourceIndex { cycle: String },
        NavDb { cycle: String },
        BundleManifest { cycle: String },
        WorldBasemapBuild,
        WorldBasemapPublish,
        TerrainDiscovery,
        GeoidSource,
        TerrainBuild { region: Region },
        TerrainWideBuild,
        TerrainPublish { region: Region },
        TerrainWidePublish,
        WaterMaskBuild { region: Region },
        ShadedReliefBuild { region: Region },
        ShadedReliefWideBuild,
        ShadedReliefPublish { region: Region },
        ShadedReliefWidePublish,
    }

    fn cycle_task_id(cycle: &str, name: &str) -> String {
        format!("{cycle}:{name}")
    }

    fn product_task_requires_publication_lock(kind: &ProductScheduledTaskKind) -> bool {
        matches!(
            kind,
            ProductScheduledTaskKind::NavDb { .. }
                | ProductScheduledTaskKind::BundleManifest { .. }
                | ProductScheduledTaskKind::WorldBasemapPublish
                | ProductScheduledTaskKind::TerrainPublish { .. }
                | ProductScheduledTaskKind::TerrainWidePublish
                | ProductScheduledTaskKind::ShadedReliefPublish { .. }
                | ProductScheduledTaskKind::ShadedReliefWidePublish
        )
    }

    fn product_task_failure_scope(task_id: &str) -> Option<String> {
        let (cycle, _) = task_id.split_once(':')?;
        if cycle.chars().all(|ch| ch.is_ascii_digit()) {
            Some(cycle.to_string())
        } else {
            None
        }
    }

    fn static_build_record_task_ids(config: &ProductBuildConfig) -> Vec<String> {
        let mut task_ids = vec!["wmm-source".to_string(), "build-world-basemap".to_string()];
        if include_static_terrain_products() {
            task_ids.push("geoid-source".to_string());
            task_ids.push("terrain-discovery".to_string());
            for region in config.profile.terrain_regions() {
                let region_id = region.code().to_ascii_lowercase();
                task_ids.push(format!("build-terrain-{region_id}"));
                task_ids.push(format!("build-water-mask-{region_id}"));
                task_ids.push(format!("build-shaded-relief-{region_id}"));
            }
            task_ids.push(format!("build-terrain-{WIDE_ANGLE_REGION_ID}"));
            task_ids.push(format!("build-shaded-relief-{WIDE_ANGLE_REGION_ID}"));
        }
        task_ids
    }

    let result = (|| -> anyhow::Result<ProductBuildResult> {
        let cycles = product_cycles_to_build(config)?;
        let chart_families = [
            ChartFamily::Sec,
            ChartFamily::Tac,
            ChartFamily::EnrL,
            ChartFamily::EnrH,
        ];
        let work_unit_budget = config.max_heavy_jobs.max(1) * 4 + 3;
        let mut pending_tasks = Vec::new();
        pending_tasks.push(GraphScheduledTask {
            id: "wmm-source".to_string(),
            deps: vec![],
            weight: 1,
            kind: ProductScheduledTaskKind::WmmSource,
        });

        for cycle in &cycles {
            pending_tasks.push(GraphScheduledTask {
                id: cycle_task_id(cycle, "source-urls"),
                deps: vec![],
                weight: 1,
                kind: ProductScheduledTaskKind::SourceUrls {
                    cycle: cycle.clone(),
                },
            });
            for family in chart_families {
                let family_id = family_slug(family);
                let fetch_id = cycle_task_id(cycle, &format!("charts-{family_id}-fetch"));
                let process_id = cycle_task_id(cycle, &format!("charts-{family_id}-process"));
                let package_id = cycle_task_id(cycle, &format!("charts-{family_id}-package"));
                pending_tasks.push(GraphScheduledTask {
                    id: fetch_id.clone(),
                    deps: vec![cycle_task_id(cycle, "source-urls")],
                    weight: 1,
                    kind: ProductScheduledTaskKind::ChartFetch {
                        cycle: cycle.clone(),
                        family,
                    },
                });
                pending_tasks.push(GraphScheduledTask {
                    id: process_id.clone(),
                    deps: vec![fetch_id.clone()],
                    weight: 4,
                    kind: ProductScheduledTaskKind::ChartProcess {
                        cycle: cycle.clone(),
                        family,
                    },
                });
                pending_tasks.push(GraphScheduledTask {
                    id: package_id.clone(),
                    deps: vec![process_id, fetch_id],
                    weight: 1,
                    kind: ProductScheduledTaskKind::ChartPackage {
                        cycle: cycle.clone(),
                        family,
                    },
                });
            }

            pending_tasks.push(GraphScheduledTask {
                id: cycle_task_id(cycle, "csup-fetch"),
                deps: vec![cycle_task_id(cycle, "source-urls")],
                weight: 1,
                kind: ProductScheduledTaskKind::CsupFetch {
                    cycle: cycle.clone(),
                },
            });
            pending_tasks.push(GraphScheduledTask {
                id: cycle_task_id(cycle, "csup-process"),
                deps: vec![cycle_task_id(cycle, "csup-fetch")],
                weight: 1,
                kind: ProductScheduledTaskKind::CsupProcess {
                    cycle: cycle.clone(),
                },
            });
            let mut csup_render_ids = Vec::new();
            for region in Region::ALL {
                let task_id = cycle_task_id(
                    cycle,
                    &format!("csup-render-{}", region.code().to_ascii_lowercase()),
                );
                csup_render_ids.push(task_id.clone());
                pending_tasks.push(GraphScheduledTask {
                    id: task_id,
                    deps: vec![cycle_task_id(cycle, "csup-process")],
                    weight: 2,
                    kind: ProductScheduledTaskKind::CsupRender {
                        cycle: cycle.clone(),
                        region,
                    },
                });
            }
            pending_tasks.push(GraphScheduledTask {
                id: cycle_task_id(cycle, "csup-package"),
                deps: csup_render_ids
                    .iter()
                    .cloned()
                    .chain(std::iter::once(cycle_task_id(cycle, "csup-fetch")))
                    .collect(),
                weight: 1,
                kind: ProductScheduledTaskKind::CsupPackage {
                    cycle: cycle.clone(),
                },
            });

            pending_tasks.push(GraphScheduledTask {
                id: cycle_task_id(cycle, "tpp-fetch"),
                deps: vec![cycle_task_id(cycle, "source-urls")],
                weight: TPP_RENDER_WEIGHT,
                kind: ProductScheduledTaskKind::TppFetch {
                    cycle: cycle.clone(),
                },
            });
            let mut tpp_package_ids = Vec::new();
            for region in config.profile.tpp_regions() {
                let region_id = region.code().to_ascii_lowercase();
                let render_id = cycle_task_id(cycle, &format!("tpp-{region_id}"));
                let package_id = cycle_task_id(cycle, &format!("tpp-{region_id}-package"));
                pending_tasks.push(GraphScheduledTask {
                    id: render_id.clone(),
                    deps: vec![
                        cycle_task_id(cycle, "source-urls"),
                        cycle_task_id(cycle, "tpp-fetch"),
                    ],
                    weight: TPP_RENDER_WEIGHT,
                    kind: ProductScheduledTaskKind::TppRender {
                        cycle: cycle.clone(),
                        region: *region,
                    },
                });
                pending_tasks.push(GraphScheduledTask {
                    id: package_id.clone(),
                    deps: vec![render_id, cycle_task_id(cycle, "tpp-fetch")],
                    weight: 1,
                    kind: ProductScheduledTaskKind::TppPackage {
                        cycle: cycle.clone(),
                        region: *region,
                    },
                });
                tpp_package_ids.push(package_id);
            }

            pending_tasks.push(GraphScheduledTask {
                id: cycle_task_id(cycle, "data-base"),
                deps: vec![cycle_task_id(cycle, "source-urls")],
                weight: 4,
                kind: ProductScheduledTaskKind::DataBase {
                    cycle: cycle.clone(),
                },
            });
            pending_tasks.push(GraphScheduledTask {
                id: cycle_task_id(cycle, "data"),
                deps: {
                    let mut deps = vec![cycle_task_id(cycle, "data-base")];
                    deps.extend(tpp_package_ids.iter().cloned());
                    deps
                },
                weight: 1,
                kind: ProductScheduledTaskKind::DataMatch {
                    cycle: cycle.clone(),
                },
            });
            pending_tasks.push(GraphScheduledTask {
                id: cycle_task_id(cycle, "vectors"),
                deps: vec![cycle_task_id(cycle, "data")],
                weight: 1,
                kind: ProductScheduledTaskKind::Vectors {
                    cycle: cycle.clone(),
                },
            });
            let mut resource_index_deps = chart_families
                .iter()
                .map(|family| {
                    cycle_task_id(cycle, &format!("charts-{}-package", family_slug(*family)))
                })
                .collect::<Vec<_>>();
            resource_index_deps.push(cycle_task_id(cycle, "csup-package"));
            resource_index_deps.extend(tpp_package_ids.iter().cloned());
            resource_index_deps.push(cycle_task_id(cycle, "data"));
            pending_tasks.push(GraphScheduledTask {
                id: cycle_task_id(cycle, "resource-index"),
                deps: resource_index_deps,
                weight: 2,
                kind: ProductScheduledTaskKind::ResourceIndex {
                    cycle: cycle.clone(),
                },
            });
            let mut nav_db_deps = vec![
                cycle_task_id(cycle, "data"),
                cycle_task_id(cycle, "resource-index"),
                cycle_task_id(cycle, "vectors"),
                "wmm-source".to_string(),
            ];
            nav_db_deps.extend(static_product_task_ids(config));
            if include_static_terrain_products() {
                nav_db_deps.extend(config.profile.terrain_regions().iter().map(|region| {
                    format!("build-shaded-relief-{}", region.code().to_ascii_lowercase())
                }));
            }
            pending_tasks.push(GraphScheduledTask {
                id: cycle_task_id(cycle, "nav-db"),
                deps: nav_db_deps,
                weight: 1,
                kind: ProductScheduledTaskKind::NavDb {
                    cycle: cycle.clone(),
                },
            });
            pending_tasks.push(GraphScheduledTask {
                id: cycle_task_id(cycle, "bundle-manifest"),
                deps: vec![cycle_task_id(cycle, "nav-db")],
                weight: 1,
                kind: ProductScheduledTaskKind::BundleManifest {
                    cycle: cycle.clone(),
                },
            });
        }

        pending_tasks.push(GraphScheduledTask {
            id: "build-world-basemap".to_string(),
            deps: vec![],
            weight: 1,
            kind: ProductScheduledTaskKind::WorldBasemapBuild,
        });
        pending_tasks.push(GraphScheduledTask {
            id: "publish-world-basemap".to_string(),
            deps: vec!["build-world-basemap".to_string()],
            weight: 1,
            kind: ProductScheduledTaskKind::WorldBasemapPublish,
        });
        if include_static_terrain_products() {
            pending_tasks.push(GraphScheduledTask {
                id: "geoid-source".to_string(),
                deps: vec![],
                weight: 1,
                kind: ProductScheduledTaskKind::GeoidSource,
            });
            pending_tasks.push(GraphScheduledTask {
                id: "terrain-discovery".to_string(),
                deps: vec![],
                weight: 1,
                kind: ProductScheduledTaskKind::TerrainDiscovery,
            });
            for region in config.profile.terrain_regions() {
                let region_id = region.code().to_ascii_lowercase();
                pending_tasks.push(GraphScheduledTask {
                    id: format!("build-terrain-{region_id}"),
                    deps: vec!["terrain-discovery".to_string(), "geoid-source".to_string()],
                    weight: 6,
                    kind: ProductScheduledTaskKind::TerrainBuild { region: *region },
                });
                pending_tasks.push(GraphScheduledTask {
                    id: format!("publish-terrain-{region_id}"),
                    deps: vec![format!("build-terrain-{region_id}")],
                    weight: 1,
                    kind: ProductScheduledTaskKind::TerrainPublish { region: *region },
                });
                pending_tasks.push(GraphScheduledTask {
                    id: format!("build-water-mask-{region_id}"),
                    deps: vec![],
                    weight: 4,
                    kind: ProductScheduledTaskKind::WaterMaskBuild { region: *region },
                });
                pending_tasks.push(GraphScheduledTask {
                    id: format!("build-shaded-relief-{region_id}"),
                    deps: vec![
                        "terrain-discovery".to_string(),
                        format!("build-water-mask-{region_id}"),
                    ],
                    weight: 6,
                    kind: ProductScheduledTaskKind::ShadedReliefBuild { region: *region },
                });
                pending_tasks.push(GraphScheduledTask {
                    id: format!("publish-shaded-relief-{region_id}"),
                    deps: vec![format!("build-shaded-relief-{region_id}")],
                    weight: 1,
                    kind: ProductScheduledTaskKind::ShadedReliefPublish { region: *region },
                });
            }
            let terrain_wide_deps = config
                .profile
                .terrain_regions()
                .iter()
                .map(|region| format!("build-terrain-{}", region.code().to_ascii_lowercase()))
                .collect::<Vec<_>>();
            pending_tasks.push(GraphScheduledTask {
                id: format!("build-terrain-{WIDE_ANGLE_REGION_ID}"),
                deps: terrain_wide_deps,
                weight: 1,
                kind: ProductScheduledTaskKind::TerrainWideBuild,
            });
            pending_tasks.push(GraphScheduledTask {
                id: format!("publish-terrain-{WIDE_ANGLE_REGION_ID}"),
                deps: vec![format!("build-terrain-{WIDE_ANGLE_REGION_ID}")],
                weight: 1,
                kind: ProductScheduledTaskKind::TerrainWidePublish,
            });
            let shaded_relief_wide_deps = config
                .profile
                .terrain_regions()
                .iter()
                .map(|region| format!("build-shaded-relief-{}", region.code().to_ascii_lowercase()))
                .collect::<Vec<_>>();
            pending_tasks.push(GraphScheduledTask {
                id: format!("build-shaded-relief-{WIDE_ANGLE_REGION_ID}"),
                deps: shaded_relief_wide_deps,
                weight: 1,
                kind: ProductScheduledTaskKind::ShadedReliefWideBuild,
            });
            pending_tasks.push(GraphScheduledTask {
                id: format!("publish-shaded-relief-{WIDE_ANGLE_REGION_ID}"),
                deps: vec![format!("build-shaded-relief-{WIDE_ANGLE_REGION_ID}")],
                weight: 1,
                kind: ProductScheduledTaskKind::ShadedReliefWidePublish,
            });
        }
        let mut current_artifacts_deps = cycles
            .iter()
            .map(|cycle| cycle_task_id(cycle, "bundle-manifest"))
            .chain(std::iter::once("publish-world-basemap".to_string()))
            .collect::<Vec<_>>();
        if include_static_terrain_products() {
            current_artifacts_deps.extend(
                config.profile.terrain_regions().iter().map(|region| {
                    format!("publish-terrain-{}", region.code().to_ascii_lowercase())
                }),
            );
            current_artifacts_deps.push(format!("publish-terrain-{WIDE_ANGLE_REGION_ID}"));
            current_artifacts_deps.extend(config.profile.terrain_regions().iter().map(|region| {
                format!(
                    "publish-shaded-relief-{}",
                    region.code().to_ascii_lowercase()
                )
            }));
        }
        let publish_ready_task_ids = current_artifacts_deps;

        master_log.log(format!(
            "product-scheduler-ready tasks={} work_unit_budget={} chart_and_data_weight=4 csup_weight=2 tpp_weight={} tpp_render_jobs_per_run={} light_weight=1 resource_index_weight=2",
            pending_tasks.len(), work_unit_budget, TPP_RENDER_WEIGHT, TPP_RENDER_JOBS_PER_RUN
        ))?;

        let config_for_tasks = config.clone();
        let graph_outcome = run_weighted_task_graph_fail_slow_with_failure_scopes(
            "product-scheduler",
            pending_tasks,
            work_unit_budget,
            product_task_failure_scope,
            |message| master_log.log(message),
            move |kind, task_values_snapshot, task_node_records_snapshot| {
                let config = config_for_tasks.clone();
                let _publication_lock = if product_task_requires_publication_lock(&kind) {
                    Some(acquire_publication_lock(&config.publish_dir, |message| {
                        eprintln!("{message}");
                    })?)
                } else {
                    None
                };
                match kind {
                    ProductScheduledTaskKind::SourceUrls { cycle } => {
                        let mut cycle_config = config.clone();
                        cycle_config.target_cycle = Some(cycle.clone());
                        let (source_urls_dir, source_urls_record) =
                            build_source_urls_node(&cycle_config)?;
                        let chart_versions = [
                            (
                                "sec".to_string(),
                                chart_family_version_label(&source_urls_dir, ChartFamily::Sec)?,
                            ),
                            (
                                "tac".to_string(),
                                chart_family_version_label(&source_urls_dir, ChartFamily::Tac)?,
                            ),
                            (
                                "enr-l".to_string(),
                                chart_family_version_label(&source_urls_dir, ChartFamily::EnrL)?,
                            ),
                            (
                                "enr-h".to_string(),
                                chart_family_version_label(&source_urls_dir, ChartFamily::EnrH)?,
                            ),
                        ]
                        .into_iter()
                        .collect::<BTreeMap<_, _>>();
                        let csup_version = csup_version_label(&source_urls_dir)?;
                        let tpp_versions = config
                            .profile
                            .tpp_regions()
                            .iter()
                            .map(|region| {
                                Ok((
                                    region.code().to_ascii_lowercase(),
                                    tpp_region_version_label(&source_urls_dir, *region)?,
                                ))
                            })
                            .collect::<anyhow::Result<BTreeMap<_, _>>>()?;
                        let data_version = data_version_label(&source_urls_dir)?;
                        let bundle_cycle = data_manifest_cycle(&source_urls_dir)?;
                        let completion_detail = format!(
                                "cycle bundle={} charts=sec:{} tac:{} enr-l:{} enr-h:{} csup:{} tpp={} data:{}",
                                bundle_cycle,
                                chart_versions["sec"],
                                chart_versions["tac"],
                                chart_versions["enr-l"],
                                chart_versions["enr-h"],
                                csup_version,
                                config
                                    .profile
                                    .tpp_regions()
                                    .iter()
                                    .map(|region| {
                                        let key = region.code().to_ascii_lowercase();
                                        format!("{}:{}", key, tpp_versions[&key])
                                    })
                                    .collect::<Vec<_>>()
                                    .join(","),
                                data_version,
                            );
                        Ok(ProductTaskCompletion {
                            node_records: vec![normalize_node_record_paths(
                                source_urls_record,
                                &cycle_config.packaged_dir,
                            )],
                            value: ProductTaskValue::SourceUrls {
                                dir: source_urls_dir,
                                chart_versions,
                                csup_version,
                                tpp_versions,
                                data_version,
                                bundle_cycle: bundle_cycle.clone(),
                            },
                            completion_detail,
                        })
                    }
                    ProductScheduledTaskKind::ChartFetch { cycle, family } => {
                        let source_urls =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "source-urls")) {
                                Some(ProductTaskValue::SourceUrls { dir, .. }) => dir.clone(),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                        let mut cycle_config = config.clone();
                        cycle_config.target_cycle = Some(cycle);
                        let family_id = family_slug(family).to_string();
                        let record = build_chart_fetch_node(
                            &cycle_config,
                            family,
                            &source_urls.join(format!("charts-{family_id}/source_urls.jsonl")),
                            cycle_config.fetch_jobs,
                        )?;
                        let cache_hit = record.cache_hit;
                        Ok(ProductTaskCompletion {
                            node_records: vec![normalize_node_record_paths(
                                record.clone(),
                                &cycle_config.packaged_dir,
                            )],
                            value: ProductTaskValue::ChartFetch { record },
                            completion_detail: format!("cache_hit={cache_hit}"),
                        })
                    }
                    ProductScheduledTaskKind::ChartProcess { cycle, family } => {
                        let source_urls =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "source-urls")) {
                                Some(ProductTaskValue::SourceUrls { dir, .. }) => dir.clone(),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                        let mut cycle_config = config.clone();
                        cycle_config.target_cycle = Some(cycle.clone());
                        let family_id = family_slug(family).to_string();
                        let source_fetch = match task_values_snapshot
                            .get(&cycle_task_id(&cycle, &format!("charts-{family_id}-fetch")))
                        {
                            Some(ProductTaskValue::ChartFetch { record }) => record,
                            _ => bail!("missing chart fetch for cycle {cycle} family {family_id}"),
                        };
                        let record = build_chart_process_node(
                            &cycle_config,
                            family,
                            &cycle_config.chart_cutline_root,
                            &source_urls.join(format!("charts-{family_id}/source_urls.jsonl")),
                            source_fetch,
                            cycle_config.cpu_jobs.min(8).max(1),
                        )?;
                        let cache_hit = record.cache_hit;
                        Ok(ProductTaskCompletion {
                            node_records: vec![normalize_node_record_paths(
                                record,
                                &cycle_config.packaged_dir,
                            )],
                            value: ProductTaskValue::None,
                            completion_detail: format!("cache_hit={cache_hit}"),
                        })
                    }
                    ProductScheduledTaskKind::CsupFetch { cycle } => {
                        let source_urls =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "source-urls")) {
                                Some(ProductTaskValue::SourceUrls { dir, .. }) => dir.clone(),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                        let mut cycle_config = config.clone();
                        cycle_config.target_cycle = Some(cycle);
                        let record = build_csup_fetch_node(
                            &cycle_config,
                            &source_urls.join("csup/source_urls.jsonl"),
                            cycle_config.fetch_jobs,
                        )?;
                        let cache_hit = record.cache_hit;
                        Ok(ProductTaskCompletion {
                            node_records: vec![normalize_node_record_paths(
                                record.clone(),
                                &cycle_config.packaged_dir,
                            )],
                            value: ProductTaskValue::CsupFetch { record },
                            completion_detail: format!("cache_hit={cache_hit}"),
                        })
                    }
                    ProductScheduledTaskKind::CsupProcess { cycle } => {
                        let source_urls =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "source-urls")) {
                                Some(ProductTaskValue::SourceUrls { dir, .. }) => dir.clone(),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                        let source_fetch =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "csup-fetch")) {
                                Some(ProductTaskValue::CsupFetch { record }) => record,
                                _ => bail!("missing csup-fetch for cycle {cycle}"),
                            };
                        let mut cycle_config = config.clone();
                        cycle_config.target_cycle = Some(cycle);
                        let record = build_csup_process_node(
                            &cycle_config,
                            Path::new(""),
                            &source_urls.join("csup/source_urls.jsonl"),
                            source_fetch,
                        )?;
                        let cache_hit = record.cache_hit;
                        let work_dir =
                            resolve_artifact_path(&cycle_config, output_path(&record, "work_dir")?);
                        Ok(ProductTaskCompletion {
                            node_records: vec![normalize_node_record_paths(
                                record.clone(),
                                &cycle_config.packaged_dir,
                            )],
                            value: ProductTaskValue::CsupProcess { record, work_dir },
                            completion_detail: format!("cache_hit={cache_hit}"),
                        })
                    }
                    ProductScheduledTaskKind::CsupRender { cycle, region } => {
                        let source_urls_key = cycle_task_id(&cycle, "source-urls");
                        let source_urls = match task_values_snapshot.get(&source_urls_key) {
                            Some(ProductTaskValue::SourceUrls { csup_version, .. }) => {
                                csup_version.clone()
                            }
                            _ => bail!("missing source urls for cycle {cycle}"),
                        };
                        let process = match task_values_snapshot
                            .get(&cycle_task_id(&cycle, "csup-process"))
                        {
                            Some(ProductTaskValue::CsupProcess { record, work_dir }) => {
                                (record.clone(), work_dir.clone())
                            }
                            _ => bail!("missing csup process for cycle {cycle}"),
                        };
                        let mut cycle_config = config.clone();
                        cycle_config.target_cycle = Some(cycle);
                        let record = build_csup_render_node(
                            &cycle_config,
                            region,
                            &process.1,
                            &process.0.fingerprint,
                            &source_urls,
                            cycle_config.cpu_jobs.max(1),
                        )?;
                        let cache_hit = record.cache_hit;
                        Ok(ProductTaskCompletion {
                            node_records: vec![normalize_node_record_paths(
                                record,
                                &cycle_config.packaged_dir,
                            )],
                            value: ProductTaskValue::None,
                            completion_detail: format!("cache_hit={cache_hit}"),
                        })
                    }
                    ProductScheduledTaskKind::TppRender { cycle, region } => {
                        let source_urls =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "source-urls")) {
                                Some(ProductTaskValue::SourceUrls { dir, .. }) => dir.clone(),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                        let source_fetch =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "tpp-fetch")) {
                                Some(ProductTaskValue::TppFetch { record }) => record,
                                _ => bail!("missing tpp-fetch output for cycle {cycle}"),
                            };
                        let mut cycle_config = config.clone();
                        cycle_config.target_cycle = Some(cycle);
                        let region_id = region.code().to_ascii_lowercase();
                        let request = NativeTppRunRequest {
                            region,
                            source_repo: PathBuf::new(),
                            run_root: PathBuf::new(),
                            prefetch_source_urls: Some(
                                source_urls.join(format!("tpp-{region_id}/source_urls.jsonl")),
                            ),
                            fetch_jobs: cycle_config.fetch_jobs,
                            render_jobs: TPP_RENDER_JOBS_PER_RUN,
                            fetch_cache: Some(static_source_fetch_cache_config(&cycle_config)?),
                        };
                        let record =
                            build_tpp_render_node(&cycle_config, &request, Some(source_fetch))?;
                        let cache_hit = record.cache_hit;
                        Ok(ProductTaskCompletion {
                            node_records: vec![normalize_node_record_paths(
                                record,
                                &cycle_config.packaged_dir,
                            )],
                            value: ProductTaskValue::None,
                            completion_detail: format!("cache_hit={cache_hit}"),
                        })
                    }
                    ProductScheduledTaskKind::TppFetch { cycle } => {
                        let source_urls =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "source-urls")) {
                                Some(ProductTaskValue::SourceUrls { dir, .. }) => dir.clone(),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                        let mut cycle_config = config.clone();
                        cycle_config.target_cycle = Some(cycle);
                        let record = build_tpp_fetch_node(&cycle_config, &source_urls)?;
                        let cache_hit = record.cache_hit;
                        Ok(ProductTaskCompletion {
                            node_records: vec![normalize_node_record_paths(
                                record.clone(),
                                &cycle_config.packaged_dir,
                            )],
                            value: ProductTaskValue::TppFetch { record },
                            completion_detail: format!("cache_hit={cache_hit}"),
                        })
                    }
                    ProductScheduledTaskKind::DataBase { cycle } => {
                        let source_urls =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "source-urls")) {
                                Some(ProductTaskValue::SourceUrls { dir, .. }) => dir.clone(),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                        let mut cycle_config = config.clone();
                        cycle_config.target_cycle = Some(cycle);
                        let records = build_data_nodes(&cycle_config, &source_urls, "data-base")?;
                        let data_record = records
                            .iter()
                            .find(|record| record.name == "data-base")
                            .cloned()
                            .context("data-base task missing data node record")?;
                        let staging_record = records
                            .iter()
                            .find(|record| record.name == "data-input-staging")
                            .cloned()
                            .context("data-base task missing data input staging node record")?;
                        let zip =
                            resolve_artifact_path(&cycle_config, output_path(&data_record, "zip")?);
                        let intermediate_sqlite_db =
                            resolve_artifact_path(&cycle_config, sqlite_output_path(&data_record)?);
                        let source_input_dir = resolve_artifact_path(
                            &cycle_config,
                            output_path(&staging_record, "staged_input_dir")?,
                        );
                        Ok(ProductTaskCompletion {
                            node_records: records
                                .into_iter()
                                .map(|record| {
                                    normalize_node_record_paths(record, &cycle_config.build_root)
                                })
                                .collect(),
                            value: ProductTaskValue::FingerprintedData {
                                intermediate_sqlite_db,
                                source_input_dir,
                                zip,
                                fingerprint: data_record.fingerprint,
                            },
                            completion_detail: "cache_or_rebuild".to_string(),
                        })
                    }
                    ProductScheduledTaskKind::ChartPackage { cycle, family } => {
                        let source_urls =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "source-urls")) {
                                Some(ProductTaskValue::SourceUrls {
                                    dir,
                                    chart_versions,
                                    ..
                                }) => (dir.clone(), chart_versions.clone()),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                        let mut cycle_config = config.clone();
                        cycle_config.target_cycle = Some(cycle.clone());
                        let family_id = family_slug(family).to_string();
                        let source_fetch = match task_values_snapshot
                            .get(&cycle_task_id(&cycle, &format!("charts-{family_id}-fetch")))
                        {
                            Some(ProductTaskValue::ChartFetch { record }) => record,
                            _ => bail!("missing chart fetch for cycle {cycle} family {family_id}"),
                        };
                        let started = Instant::now();
                        let (records, source) = build_chart_package_nodes(
                            &cycle_config,
                            family,
                            &source_urls.0,
                            source_urls
                                .1
                                .get(&family_id)
                                .expect("chart family version should exist"),
                            source_fetch,
                        )?;
                        let summary = summarize_package_records(&records);
                        Ok(ProductTaskCompletion {
                            node_records: records
                                .into_iter()
                                .map(|record| {
                                    normalize_node_record_paths(record, &cycle_config.build_root)
                                })
                                .collect(),
                            value: ProductTaskValue::ChartSource(source),
                            completion_detail: format!(
                                "elapsed_ms={} regions={} cache_hits={} rebuilt={}",
                                started.elapsed().as_millis(),
                                summary.total,
                                summary.cache_hits,
                                summary.rebuilt,
                            ),
                        })
                    }
                    ProductScheduledTaskKind::CsupPackage { cycle } => {
                        let source_urls =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "source-urls")) {
                                Some(ProductTaskValue::SourceUrls {
                                    dir, csup_version, ..
                                }) => (dir.clone(), csup_version.clone()),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                        let mut cycle_config = config.clone();
                        cycle_config.target_cycle = Some(cycle.clone());
                        let source_fetch =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "csup-fetch")) {
                                Some(ProductTaskValue::CsupFetch { record }) => record,
                                _ => bail!("missing csup-fetch for cycle {cycle}"),
                            };
                        let started = Instant::now();
                        let (records, source) = build_csup_package_nodes(
                            &cycle_config,
                            &source_urls.0,
                            &source_urls.1,
                            source_fetch,
                        )?;
                        let summary = summarize_package_records(&records);
                        Ok(ProductTaskCompletion {
                            node_records: records
                                .into_iter()
                                .map(|record| {
                                    normalize_node_record_paths(record, &cycle_config.build_root)
                                })
                                .collect(),
                            value: ProductTaskValue::CsupSource(source),
                            completion_detail: format!(
                                "elapsed_ms={} regions={} cache_hits={} rebuilt={}",
                                started.elapsed().as_millis(),
                                summary.total,
                                summary.cache_hits,
                                summary.rebuilt,
                            ),
                        })
                    }
                    ProductScheduledTaskKind::TppPackage { cycle, region } => {
                        let source_urls =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "source-urls")) {
                                Some(ProductTaskValue::SourceUrls {
                                    dir, tpp_versions, ..
                                }) => (dir.clone(), tpp_versions.clone()),
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                        let source_fetch =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "tpp-fetch")) {
                                Some(ProductTaskValue::TppFetch { record }) => record,
                                _ => bail!("missing tpp-fetch output for cycle {cycle}"),
                            };
                        let mut cycle_config = config.clone();
                        cycle_config.target_cycle = Some(cycle);
                        let region_id = region.code().to_ascii_lowercase();
                        let started = Instant::now();
                        let (record, source) = build_tpp_package_node(
                            &cycle_config,
                            region,
                            &source_urls
                                .0
                                .join(format!("tpp-{region_id}/source_urls.jsonl")),
                            source_urls
                                .1
                                .get(&region_id)
                                .expect("tpp region version should exist"),
                            Some(source_fetch),
                        )?;
                        let cache_hit = record.cache_hit;
                        let fingerprint = record.fingerprint.clone();
                        Ok(ProductTaskCompletion {
                            node_records: vec![normalize_node_record_paths(
                                record,
                                &cycle_config.packaged_dir,
                            )],
                            value: ProductTaskValue::FingerprintedTppSource {
                                source,
                                fingerprint,
                            },
                            completion_detail: format!(
                                "elapsed_ms={} cache_hit={}",
                                started.elapsed().as_millis(),
                                cache_hit,
                            ),
                        })
                    }
                    ProductScheduledTaskKind::DataMatch { cycle } => {
                        let source_urls =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "source-urls")) {
                                Some(ProductTaskValue::SourceUrls { data_version, .. }) => {
                                    data_version.clone()
                                }
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                        let raw_data =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "data-base")) {
                                Some(ProductTaskValue::FingerprintedData {
                                    intermediate_sqlite_db,
                                    source_input_dir,
                                    zip,
                                    fingerprint,
                                }) => (
                                    intermediate_sqlite_db.clone(),
                                    source_input_dir.clone(),
                                    zip.clone(),
                                    fingerprint.clone(),
                                ),
                                _ => bail!("missing data-base output for cycle {cycle}"),
                            };
                        let tpp_sources = config
                            .profile
                            .tpp_regions()
                            .iter()
                            .map(|region| {
                                let key = cycle_task_id(
                                    &cycle,
                                    &format!("tpp-{}-package", region.code().to_ascii_lowercase()),
                                );
                                match task_values_snapshot.get(&key) {
                                    Some(ProductTaskValue::FingerprintedTppSource {
                                        source,
                                        fingerprint,
                                    }) => Ok((*region, source.clone(), fingerprint.clone())),
                                    _ => bail!("missing tpp package source for {}", region.code()),
                                }
                            })
                            .collect::<anyhow::Result<Vec<_>>>()?;
                        let mut cycle_config = config.clone();
                        cycle_config.target_cycle = Some(cycle);
                        let record = build_data_match_node(
                            &cycle_config,
                            &raw_data.0,
                            &raw_data.2,
                            &source_urls,
                            &raw_data.3,
                            &tpp_sources,
                        )?;
                        let cache_hit = record.cache_hit;
                        let zip =
                            resolve_artifact_path(&cycle_config, output_path(&record, "zip")?);
                        let intermediate_sqlite_db =
                            resolve_artifact_path(&cycle_config, sqlite_output_path(&record)?);
                        let fingerprint = record.fingerprint.clone();
                        Ok(ProductTaskCompletion {
                            node_records: vec![normalize_node_record_paths(
                                record,
                                &cycle_config.packaged_dir,
                            )],
                            value: ProductTaskValue::FingerprintedData {
                                intermediate_sqlite_db,
                                source_input_dir: raw_data.1,
                                zip,
                                fingerprint,
                            },
                            completion_detail: format!("cache_hit={}", cache_hit),
                        })
                    }
                    ProductScheduledTaskKind::Vectors { cycle } => {
                        let (data, source_input_dir, data_fingerprint) =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "data")) {
                                Some(ProductTaskValue::FingerprintedData {
                                    intermediate_sqlite_db,
                                    source_input_dir,
                                    fingerprint,
                                    ..
                                }) => (
                                    intermediate_sqlite_db.clone(),
                                    source_input_dir.clone(),
                                    fingerprint.clone(),
                                ),
                                _ => bail!("missing data output for cycle {cycle}"),
                            };
                        let data_version =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "source-urls")) {
                                Some(ProductTaskValue::SourceUrls { data_version, .. }) => {
                                    data_version.clone()
                                }
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                        let mut cycle_config = config.clone();
                        cycle_config.target_cycle = Some(cycle);
                        let record = build_vectors_node(
                            &cycle_config,
                            &data,
                            &source_input_dir,
                            &data_fingerprint,
                            &data_version,
                        )?;
                        let cache_hit = record.cache_hit;
                        let pairs = resolve_artifact_path(
                            &cycle_config,
                            output_path(&record, "had_pairs")?,
                        );
                        let errors =
                            resolve_artifact_path(&cycle_config, output_path(&record, "errors")?);
                        Ok(ProductTaskCompletion {
                            node_records: vec![normalize_node_record_paths(
                                record,
                                &cycle_config.packaged_dir,
                            )],
                            value: ProductTaskValue::VectorHad { pairs, errors },
                            completion_detail: format!("cache_hit={}", cache_hit),
                        })
                    }
                    ProductScheduledTaskKind::ResourceIndex { cycle } => {
                        let data_zip = match task_values_snapshot
                            .get(&cycle_task_id(&cycle, "data"))
                        {
                            Some(ProductTaskValue::FingerprintedData { zip, .. }) => zip.clone(),
                            _ => bail!("missing data output for cycle {cycle}"),
                        };
                        let chart_sources = ["sec", "tac", "enr-l", "enr-h"]
                            .iter()
                            .map(|family_id| {
                                let key =
                                    cycle_task_id(&cycle, &format!("charts-{family_id}-package"));
                                match task_values_snapshot.get(&key) {
                                    Some(ProductTaskValue::ChartSource(source)) => {
                                        Ok(source.clone())
                                    }
                                    _ => bail!(
                                        "missing chart source for cycle {cycle} family {family_id}"
                                    ),
                                }
                            })
                            .collect::<anyhow::Result<Vec<_>>>()?;
                        let csup_sources = vec![match task_values_snapshot
                            .get(&cycle_task_id(&cycle, "csup-package"))
                        {
                            Some(ProductTaskValue::CsupSource(source)) => source.clone(),
                            _ => bail!("missing csup package source for cycle {cycle}"),
                        }];
                        let tpp_sources = config
                            .profile
                            .tpp_regions()
                            .iter()
                            .map(|region| {
                                let key = cycle_task_id(
                                    &cycle,
                                    &format!("tpp-{}-package", region.code().to_ascii_lowercase()),
                                );
                                match task_values_snapshot.get(&key) {
                                    Some(ProductTaskValue::FingerprintedTppSource {
                                        source,
                                        ..
                                    }) => Ok(source.clone()),
                                    _ => bail!(
                                        "missing tpp package source for cycle {cycle} {}",
                                        region.code()
                                    ),
                                }
                            })
                            .collect::<anyhow::Result<Vec<_>>>()?;
                        let mut cycle_config = config.clone();
                        cycle_config.target_cycle = Some(cycle);
                        let record = build_resource_index_node(
                            &cycle_config,
                            &data_zip,
                            chart_sources,
                            tpp_sources,
                            csup_sources,
                        )?;
                        let cache_hit = record.cache_hit;
                        Ok(ProductTaskCompletion {
                            node_records: vec![normalize_node_record_paths(
                                record,
                                &cycle_config.packaged_dir,
                            )],
                            value: ProductTaskValue::None,
                            completion_detail: format!("cache_hit={}", cache_hit),
                        })
                    }
                    ProductScheduledTaskKind::WmmSource => {
                        let source = build_wmm_source_node(&config)?;
                        let cache_hit = source.node_record.cache_hit;
                        Ok(ProductTaskCompletion {
                            node_records: vec![normalize_node_record_paths(
                                source.node_record,
                                &config.packaged_dir,
                            )],
                            value: ProductTaskValue::WmmSource {
                                cof_path: source.cof_path,
                                metadata_path: source.metadata_path,
                            },
                            completion_detail: format!("cache_hit={}", cache_hit),
                        })
                    }
                    ProductScheduledTaskKind::NavDb { cycle } => {
                        let resource_index_path = match task_values_snapshot
                            .get(&cycle_task_id(&cycle, "resource-index"))
                        {
                            Some(ProductTaskValue::None) => {
                                let resource_index_record = task_node_records_snapshot
                                    .get(&cycle_task_id(&cycle, "resource-index"))
                                    .and_then(|records| {
                                        records
                                            .iter()
                                            .find(|record| record.name == "resource-index")
                                    })
                                    .cloned()
                                    .context("missing resource-index node record")?;
                                resolve_artifact_path(
                                    &config,
                                    output_path(&resource_index_record, "resource_index")?,
                                )
                            }
                            _ => bail!("missing resource-index output for cycle {cycle}"),
                        };
                        let intermediate_sqlite_db =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "data")) {
                                Some(ProductTaskValue::FingerprintedData {
                                    intermediate_sqlite_db,
                                    ..
                                }) => intermediate_sqlite_db.clone(),
                                _ => bail!("missing data output for cycle {cycle}"),
                            };
                        let mut cycle_config = config.clone();
                        cycle_config.target_cycle = Some(cycle.clone());
                        let static_raster_tile_levels = collect_static_raster_tile_levels(
                            &task_values_snapshot,
                            &cycle_config,
                        )?;
                        let stable_packages = static_product_task_ids(&cycle_config)
                            .iter()
                            .map(|task_id| match task_values_snapshot.get(task_id) {
                                Some(ProductTaskValue::PublishedStandaloneProduct {
                                    id,
                                    published_zip,
                                    sha256,
                                    size_bytes,
                                    source_version,
                                    source_fetched_at_utc,
                                    ..
                                }) => build_stable_bundle_package_artifact(
                                    id,
                                    published_zip,
                                    sha256,
                                    *size_bytes,
                                    source_version,
                                    source_fetched_at_utc.clone(),
                                ),
                                _ => {
                                    bail!("missing published stable product output for {}", task_id)
                                }
                            })
                            .collect::<anyhow::Result<Vec<_>>>()?;
                        let vector_had_pairs_path =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "vectors")) {
                                Some(ProductTaskValue::VectorHad { pairs, .. }) => pairs.clone(),
                                _ => bail!("missing vectors HAD output for cycle {cycle}"),
                            };
                        let (wmm_cof_path, wmm_metadata_path) =
                            match task_values_snapshot.get("wmm-source") {
                                Some(ProductTaskValue::WmmSource {
                                    cof_path,
                                    metadata_path,
                                }) => (cof_path.clone(), metadata_path.clone()),
                                _ => bail!("missing WMM source output"),
                            };
                        let built = build_nav_kv_artifact(
                            &cycle_config,
                            &resource_index_path,
                            &intermediate_sqlite_db,
                            &cycle,
                            &vector_had_pairs_path,
                            &wmm_cof_path,
                            &wmm_metadata_path,
                            &stable_packages,
                            &static_raster_tile_levels,
                        )?;
                        let unpack_source_root = resolve_nav_db_unpack_source_root_from_record(
                            &cycle_config,
                            &built.node_record,
                        )
                        .with_context(|| {
                            format!("nav-db node for cycle {cycle} missing unpack source root")
                        })?;
                        Ok(ProductTaskCompletion {
                            node_records: vec![normalize_node_record_paths(
                                built.node_record,
                                &cycle_config.packaged_dir,
                            )],
                            value: ProductTaskValue::PublishedNavDb {
                                package: built.package,
                                unpack_source_root,
                            },
                            completion_detail: "cache_or_rebuild".to_string(),
                        })
                    }
                    ProductScheduledTaskKind::BundleManifest { cycle } => {
                        let mut cycle_config = config.clone();
                        cycle_config.target_cycle = Some(cycle.clone());
                        let source_urls =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "source-urls")) {
                                Some(ProductTaskValue::SourceUrls { bundle_cycle, .. }) => {
                                    bundle_cycle.clone()
                                }
                                _ => bail!("missing source urls for cycle {cycle}"),
                            };
                        let static_build_record_task_ids =
                            static_build_record_task_ids(&cycle_config);
                        let mut node_records = task_node_records_snapshot
                            .iter()
                            .filter(|(task_id, _)| {
                                task_id.starts_with(&format!("{cycle}:"))
                                    || static_build_record_task_ids.contains(task_id)
                            })
                            .flat_map(|(_, records)| records.clone())
                            .collect::<Vec<_>>();
                        node_records.sort_by(|left, right| {
                            left.name
                                .cmp(&right.name)
                                .then_with(|| left.fingerprint.cmp(&right.fingerprint))
                        });
                        node_records.dedup_by(|left, right| {
                            left.name == right.name && left.fingerprint == right.fingerprint
                        });
                        let build_manifest = BuildManifest {
                            schema_version: 1,
                            profile: cycle_config.profile.as_str().to_string(),
                            cycle: source_urls.clone(),
                            build_root: cycle_config.build_root.display().to_string(),
                            generated_at_utc: manifest_generated_at(&node_records),
                            fetch_cache_root: relative_artifact_path(
                                &cycle_config.fetch_cache_root,
                                &cycle_config.build_root,
                            ),
                            fetch_cache_mode: cycle_config.fetch_cache_mode.clone(),
                            nodes: node_records,
                        };
                        let build_manifest_path =
                            internal_build_manifest_path(&cycle_config, &source_urls)?;
                        fs::write(
                            &build_manifest_path,
                            serde_json::to_vec_pretty(&build_manifest)
                                .context("failed to encode product build manifest")?,
                        )
                        .with_context(|| {
                            format!("failed to write {}", build_manifest_path.display())
                        })?;
                        let stable_packages = static_product_task_ids(&cycle_config)
                            .iter()
                            .map(|task_id| match task_values_snapshot.get(task_id) {
                                Some(ProductTaskValue::PublishedStandaloneProduct {
                                    id,
                                    published_zip,
                                    sha256,
                                    size_bytes,
                                    source_version,
                                    source_fetched_at_utc,
                                    ..
                                }) => build_stable_bundle_package_artifact(
                                    id,
                                    published_zip,
                                    sha256,
                                    *size_bytes,
                                    source_version,
                                    source_fetched_at_utc.clone(),
                                ),
                                _ => {
                                    bail!("missing published stable product output for {}", task_id)
                                }
                            })
                            .collect::<anyhow::Result<Vec<_>>>()?;
                        let nav_db_package =
                            match task_values_snapshot.get(&cycle_task_id(&cycle, "nav-db")) {
                                Some(ProductTaskValue::PublishedNavDb { package, .. }) => {
                                    package.clone()
                                }
                                _ => bail!("missing nav-db output for cycle {cycle}"),
                            };
                        let bundle_manifest = build_bundle_manifest(
                            &cycle_config,
                            &build_manifest,
                            &stable_packages,
                            &nav_db_package,
                        )?;
                        let bundle_manifest_path = write_hashed_bundle_manifest(
                            &cycle_config.packaged_dir,
                            &bundle_manifest,
                        )?;
                        validate_bundle_manifest(
                            &cycle_config.packaged_dir,
                            &bundle_manifest_path,
                        )?;
                        Ok(ProductTaskCompletion {
                            node_records: vec![],
                            value: ProductTaskValue::CycleManifest {
                                path: bundle_manifest_path,
                            },
                            completion_detail: "published".to_string(),
                        })
                    }
                    ProductScheduledTaskKind::WorldBasemapBuild => {
                        let (
                            zip_path,
                            source_version,
                            source_fetched_at_utc,
                            tile_levels,
                            record,
                            mut source_records,
                        ) = build_world_basemap_product(&config)?;
                        let cache_hit = record.cache_hit;
                        let (zip_sha256, zip_size_bytes) = node_output_file_detail(&record, "zip");
                        let unpack_source_root = zip_path
                            .parent()
                            .with_context(|| {
                                format!("world basemap zip has no parent: {}", zip_path.display())
                            })?
                            .to_path_buf();
                        source_records.push(record);
                        Ok(ProductTaskCompletion {
                            node_records: source_records,
                            value: ProductTaskValue::BuiltStaticTileProduct {
                                zip_path,
                                unpack_source_root,
                                zip_sha256,
                                zip_size_bytes,
                                source_version,
                                source_fetched_at_utc,
                                tile_levels,
                            },
                            completion_detail: format!("cache_hit={cache_hit}"),
                        })
                    }
                    ProductScheduledTaskKind::TerrainDiscovery => {
                        let (index_path, source_fetched_at_utc, record) =
                            build_terrain_discovery_index(&config)?;
                        let cache_hit = record.cache_hit;
                        Ok(ProductTaskCompletion {
                            node_records: vec![record],
                            value: ProductTaskValue::TerrainDiscovery {
                                index_path,
                                source_fetched_at_utc,
                            },
                            completion_detail: format!("cache_hit={cache_hit}"),
                        })
                    }
                    ProductScheduledTaskKind::GeoidSource => {
                        let source = build_egm2008_geoid_source_node(&config)?;
                        let cache_hit = source.node_record.cache_hit;
                        Ok(ProductTaskCompletion {
                            node_records: vec![normalize_node_record_paths(
                                source.node_record,
                                &config.packaged_dir,
                            )],
                            value: ProductTaskValue::GeoidSource {
                                csv_path: source.csv_path,
                                metadata_path: source.metadata_path,
                                source_fetched_at_utc: source.source_fetched_at_utc,
                            },
                            completion_detail: format!("cache_hit={cache_hit}"),
                        })
                    }
                    ProductScheduledTaskKind::TerrainBuild { region } => {
                        let (index_path, source_fetched_at_utc) =
                            match task_values_snapshot.get("terrain-discovery") {
                                Some(ProductTaskValue::TerrainDiscovery {
                                    index_path,
                                    source_fetched_at_utc,
                                }) => (index_path.clone(), source_fetched_at_utc.clone()),
                                _ => bail!("missing terrain discovery output"),
                            };
                        let (geoid_csv_path, geoid_metadata_path, geoid_fetched_at_utc) =
                            match task_values_snapshot.get("geoid-source") {
                                Some(ProductTaskValue::GeoidSource {
                                    csv_path,
                                    metadata_path,
                                    source_fetched_at_utc,
                                }) => (
                                    csv_path.clone(),
                                    metadata_path.clone(),
                                    source_fetched_at_utc.clone(),
                                ),
                                _ => bail!("missing geoid source output"),
                            };
                        let (zip_path, source_version, source_fetched_at_utc, record) =
                            build_terrain_product(
                                &config,
                                region,
                                &index_path,
                                source_fetched_at_utc,
                                &geoid_csv_path,
                                &geoid_metadata_path,
                                geoid_fetched_at_utc,
                            )?;
                        let cache_hit = record.cache_hit;
                        let (zip_sha256, zip_size_bytes) = node_output_file_detail(&record, "zip");
                        let unpack_source_root = zip_path
                            .parent()
                            .with_context(|| {
                                format!("terrain zip has no parent: {}", zip_path.display())
                            })?
                            .to_path_buf();
                        Ok(ProductTaskCompletion {
                            node_records: vec![record],
                            value: ProductTaskValue::BuiltStandaloneProduct {
                                zip_path,
                                unpack_source_root,
                                zip_sha256,
                                zip_size_bytes,
                                source_version,
                                source_fetched_at_utc,
                            },
                            completion_detail: format!("cache_hit={cache_hit}"),
                        })
                    }
                    ProductScheduledTaskKind::TerrainWideBuild => {
                        let mut regional_products = Vec::new();
                        for region in config.profile.terrain_regions() {
                            let region_id = region.code().to_ascii_lowercase();
                            let task_id = format!("build-terrain-{region_id}");
                            match task_values_snapshot.get(&task_id) {
                                Some(ProductTaskValue::BuiltStandaloneProduct {
                                    zip_path,
                                    zip_sha256,
                                    source_version,
                                    source_fetched_at_utc,
                                    ..
                                }) => {
                                    let output_dir = zip_path
                                        .parent()
                                        .with_context(|| {
                                            format!(
                                                "terrain zip has no parent for {}",
                                                zip_path.display()
                                            )
                                        })?
                                        .to_path_buf();
                                    regional_products.push((
                                        region_id,
                                        output_dir,
                                        source_version.clone(),
                                        zip_sha256.clone().unwrap_or_default(),
                                        source_fetched_at_utc.clone(),
                                    ));
                                }
                                _ => bail!("missing terrain build output for {}", region.code()),
                            }
                        }
                        let (zip_path, source_version, source_fetched_at_utc, record) =
                            build_terrain_wide_product(&config, &regional_products)?;
                        let cache_hit = record.cache_hit;
                        let (zip_sha256, zip_size_bytes) = node_output_file_detail(&record, "zip");
                        let unpack_source_root = zip_path
                            .parent()
                            .with_context(|| {
                                format!("terrain wide zip has no parent: {}", zip_path.display())
                            })?
                            .to_path_buf();
                        Ok(ProductTaskCompletion {
                            node_records: vec![record],
                            value: ProductTaskValue::BuiltStandaloneProduct {
                                zip_path,
                                unpack_source_root,
                                zip_sha256,
                                zip_size_bytes,
                                source_version,
                                source_fetched_at_utc,
                            },
                            completion_detail: format!("cache_hit={cache_hit}"),
                        })
                    }
                    ProductScheduledTaskKind::WaterMaskBuild { region } => {
                        let (
                            _zip_path,
                            mask_tiles_dir,
                            source_version,
                            _source_fetched_at_utc,
                            record,
                        ) = build_water_mask_product(&config, region)?;
                        let cache_hit = record.cache_hit;
                        Ok(ProductTaskCompletion {
                            node_records: vec![record],
                            value: ProductTaskValue::BuiltWaterMask {
                                mask_tiles_dir,
                                source_version,
                            },
                            completion_detail: format!("cache_hit={cache_hit}"),
                        })
                    }
                    ProductScheduledTaskKind::ShadedReliefBuild { region } => {
                        let (index_path, source_fetched_at_utc) =
                            match task_values_snapshot.get("terrain-discovery") {
                                Some(ProductTaskValue::TerrainDiscovery {
                                    index_path,
                                    source_fetched_at_utc,
                                }) => (index_path.clone(), source_fetched_at_utc.clone()),
                                _ => bail!("missing terrain discovery output"),
                            };
                        let region_id = region.code().to_ascii_lowercase();
                        let (water_mask_dir, water_mask_version) = match task_values_snapshot
                            .get(&format!("build-water-mask-{region_id}"))
                        {
                            Some(ProductTaskValue::BuiltWaterMask {
                                mask_tiles_dir,
                                source_version,
                            }) => (mask_tiles_dir.clone(), source_version.clone()),
                            _ => bail!("missing water mask output for {}", region.code()),
                        };
                        let (
                            zip_path,
                            source_version,
                            source_fetched_at_utc,
                            tile_levels,
                            record,
                            mut source_records,
                        ) = build_shaded_relief_product(
                            &config,
                            region,
                            &index_path,
                            source_fetched_at_utc,
                            &water_mask_dir,
                            &water_mask_version,
                        )?;
                        let cache_hit = record.cache_hit;
                        let (zip_sha256, zip_size_bytes) = node_output_file_detail(&record, "zip");
                        let unpack_source_root = zip_path
                            .parent()
                            .with_context(|| {
                                format!("shaded relief zip has no parent: {}", zip_path.display())
                            })?
                            .join("package");
                        source_records.push(record);
                        Ok(ProductTaskCompletion {
                            node_records: source_records,
                            value: ProductTaskValue::BuiltStaticTileProduct {
                                zip_path,
                                unpack_source_root,
                                zip_sha256,
                                zip_size_bytes,
                                source_version,
                                source_fetched_at_utc,
                                tile_levels,
                            },
                            completion_detail: format!("cache_hit={cache_hit}"),
                        })
                    }
                    ProductScheduledTaskKind::ShadedReliefWideBuild => {
                        let overlays = prepare_shaded_relief_overlay_sources(&config)?;
                        let overlay_record = overlays.node_record.clone();
                        let mut regional_products = Vec::new();
                        for region in config.profile.terrain_regions() {
                            let region_id = region.code().to_ascii_lowercase();
                            let task_id = format!("build-shaded-relief-{region_id}");
                            match task_values_snapshot.get(&task_id) {
                                Some(ProductTaskValue::BuiltStaticTileProduct {
                                    zip_path,
                                    zip_sha256,
                                    source_version,
                                    source_fetched_at_utc,
                                    ..
                                }) => {
                                    let output_dir = zip_path
                                        .parent()
                                        .with_context(|| {
                                            format!(
                                                "shaded relief zip has no parent for {}",
                                                zip_path.display()
                                            )
                                        })?
                                        .to_path_buf();
                                    regional_products.push((
                                        region_id,
                                        output_dir,
                                        source_version.clone(),
                                        zip_sha256.clone().unwrap_or_default(),
                                        source_fetched_at_utc.clone(),
                                    ));
                                }
                                _ => bail!(
                                    "missing shaded relief build output for {}",
                                    region.code()
                                ),
                            }
                        }
                        let (zip_path, source_version, source_fetched_at_utc, tile_levels, record) =
                            build_shaded_relief_wide_product(
                                &config,
                                &regional_products,
                                &overlays,
                            )?;
                        let cache_hit = record.cache_hit;
                        let (zip_sha256, zip_size_bytes) = node_output_file_detail(&record, "zip");
                        let unpack_source_root = zip_path
                            .parent()
                            .with_context(|| {
                                format!(
                                    "shaded relief wide zip has no parent: {}",
                                    zip_path.display()
                                )
                            })?
                            .to_path_buf();
                        Ok(ProductTaskCompletion {
                            node_records: vec![overlay_record, record],
                            value: ProductTaskValue::BuiltStaticTileProduct {
                                zip_path,
                                unpack_source_root,
                                zip_sha256,
                                zip_size_bytes,
                                source_version,
                                source_fetched_at_utc,
                                tile_levels,
                            },
                            completion_detail: format!("cache_hit={cache_hit}"),
                        })
                    }
                    ProductScheduledTaskKind::WorldBasemapPublish => {
                        let built = match task_values_snapshot.get("build-world-basemap") {
                            Some(ProductTaskValue::BuiltStaticTileProduct {
                                zip_path,
                                unpack_source_root,
                                zip_sha256,
                                zip_size_bytes,
                                source_version,
                                source_fetched_at_utc,
                                ..
                            }) => (
                                zip_path.clone(),
                                unpack_source_root.clone(),
                                zip_sha256.clone(),
                                *zip_size_bytes,
                                source_version.clone(),
                                source_fetched_at_utc.clone(),
                            ),
                            _ => bail!("missing world basemap build output"),
                        };
                        let product_id = stable_product_id_with_contract("world-basemap")?;
                        let (published_zip, sha256, size_bytes) = publish_content_addressed_zip(
                            &config.packaged_dir,
                            &built.0,
                            &product_id,
                            built.2.as_deref(),
                            built.3,
                        )?;
                        Ok(ProductTaskCompletion {
                            node_records: vec![],
                            value: ProductTaskValue::PublishedStandaloneProduct {
                                id: product_id,
                                unpack_source_root: built.1,
                                published_zip,
                                sha256,
                                size_bytes,
                                source_version: built.4,
                                source_fetched_at_utc: built.5,
                            },
                            completion_detail: "published".to_string(),
                        })
                    }
                    ProductScheduledTaskKind::TerrainPublish { region } => {
                        let region_id = region.code().to_ascii_lowercase();
                        let task_id = format!("build-terrain-{region_id}");
                        let built = match task_values_snapshot.get(&task_id) {
                            Some(ProductTaskValue::BuiltStandaloneProduct {
                                zip_path,
                                unpack_source_root,
                                zip_sha256,
                                zip_size_bytes,
                                source_version,
                                source_fetched_at_utc,
                                ..
                            }) => (
                                zip_path.clone(),
                                unpack_source_root.clone(),
                                zip_sha256.clone(),
                                *zip_size_bytes,
                                source_version.clone(),
                                source_fetched_at_utc.clone(),
                            ),
                            _ => bail!("missing terrain build output for {}", region.code()),
                        };
                        let product_id =
                            stable_product_id_with_contract(&format!("terrain-{region_id}"))?;
                        let (published_zip, sha256, size_bytes) = publish_content_addressed_zip(
                            &config.packaged_dir,
                            &built.0,
                            &product_id,
                            built.2.as_deref(),
                            built.3,
                        )?;
                        Ok(ProductTaskCompletion {
                            node_records: vec![],
                            value: ProductTaskValue::PublishedStandaloneProduct {
                                id: product_id,
                                unpack_source_root: built.1,
                                published_zip,
                                sha256,
                                size_bytes,
                                source_version: built.4,
                                source_fetched_at_utc: built.5,
                            },
                            completion_detail: "published".to_string(),
                        })
                    }
                    ProductScheduledTaskKind::TerrainWidePublish => {
                        let task_id = format!("build-terrain-{WIDE_ANGLE_REGION_ID}");
                        let built = match task_values_snapshot.get(&task_id) {
                            Some(ProductTaskValue::BuiltStandaloneProduct {
                                zip_path,
                                unpack_source_root,
                                zip_sha256,
                                zip_size_bytes,
                                source_version,
                                source_fetched_at_utc,
                                ..
                            }) => (
                                zip_path.clone(),
                                unpack_source_root.clone(),
                                zip_sha256.clone(),
                                *zip_size_bytes,
                                source_version.clone(),
                                source_fetched_at_utc.clone(),
                            ),
                            _ => bail!("missing terrain wide-angle build output"),
                        };
                        let product_id = stable_product_id_with_contract(&format!(
                            "terrain-{WIDE_ANGLE_REGION_ID}"
                        ))?;
                        let (published_zip, sha256, size_bytes) = publish_content_addressed_zip(
                            &config.packaged_dir,
                            &built.0,
                            &product_id,
                            built.2.as_deref(),
                            built.3,
                        )?;
                        Ok(ProductTaskCompletion {
                            node_records: vec![],
                            value: ProductTaskValue::PublishedStandaloneProduct {
                                id: product_id,
                                unpack_source_root: built.1,
                                published_zip,
                                sha256,
                                size_bytes,
                                source_version: built.4,
                                source_fetched_at_utc: built.5,
                            },
                            completion_detail: "published".to_string(),
                        })
                    }
                    ProductScheduledTaskKind::ShadedReliefPublish { region } => {
                        let region_id = region.code().to_ascii_lowercase();
                        let task_id = format!("build-shaded-relief-{region_id}");
                        let built = match task_values_snapshot.get(&task_id) {
                            Some(ProductTaskValue::BuiltStaticTileProduct {
                                zip_path,
                                unpack_source_root,
                                zip_sha256,
                                zip_size_bytes,
                                source_version,
                                source_fetched_at_utc,
                                ..
                            }) => (
                                zip_path.clone(),
                                unpack_source_root.clone(),
                                zip_sha256.clone(),
                                *zip_size_bytes,
                                source_version.clone(),
                                source_fetched_at_utc.clone(),
                            ),
                            _ => bail!("missing shaded relief build output for {}", region.code()),
                        };
                        let product_id =
                            stable_product_id_with_contract(&format!("shaded-relief-{region_id}"))?;
                        let (published_zip, sha256, size_bytes) = publish_content_addressed_zip(
                            &config.packaged_dir,
                            &built.0,
                            &product_id,
                            built.2.as_deref(),
                            built.3,
                        )?;
                        Ok(ProductTaskCompletion {
                            node_records: vec![],
                            value: ProductTaskValue::PublishedStandaloneProduct {
                                id: product_id,
                                unpack_source_root: built.1,
                                published_zip,
                                sha256,
                                size_bytes,
                                source_version: built.4,
                                source_fetched_at_utc: built.5,
                            },
                            completion_detail: "published".to_string(),
                        })
                    }
                    ProductScheduledTaskKind::ShadedReliefWidePublish => {
                        let task_id = format!("build-shaded-relief-{WIDE_ANGLE_REGION_ID}");
                        let built = match task_values_snapshot.get(&task_id) {
                            Some(ProductTaskValue::BuiltStaticTileProduct {
                                zip_path,
                                unpack_source_root,
                                zip_sha256,
                                zip_size_bytes,
                                source_version,
                                source_fetched_at_utc,
                                ..
                            }) => (
                                zip_path.clone(),
                                unpack_source_root.clone(),
                                zip_sha256.clone(),
                                *zip_size_bytes,
                                source_version.clone(),
                                source_fetched_at_utc.clone(),
                            ),
                            _ => bail!("missing shaded relief wide-angle build output"),
                        };
                        let product_id = stable_product_id_with_contract(&format!(
                            "shaded-relief-{WIDE_ANGLE_REGION_ID}"
                        ))?;
                        let (published_zip, sha256, size_bytes) = publish_content_addressed_zip(
                            &config.packaged_dir,
                            &built.0,
                            &product_id,
                            built.2.as_deref(),
                            built.3,
                        )?;
                        Ok(ProductTaskCompletion {
                            node_records: vec![],
                            value: ProductTaskValue::PublishedStandaloneProduct {
                                id: product_id,
                                unpack_source_root: built.1,
                                published_zip,
                                sha256,
                                size_bytes,
                                source_version: built.4,
                                source_fetched_at_utc: built.5,
                            },
                            completion_detail: "published".to_string(),
                        })
                    }
                }
            },
        )?;
        let task_values = graph_outcome.task_values;
        let task_node_records = graph_outcome.task_node_records;

        let mut cycle_manifest_paths = cycles
            .iter()
            .filter_map(
                |cycle| match task_values.get(&cycle_task_id(cycle, "bundle-manifest")) {
                    Some(ProductTaskValue::CycleManifest { path }) => Some(path.clone()),
                    _ => None,
                },
            )
            .collect::<Vec<_>>();
        cycle_manifest_paths.sort();
        if cycle_manifest_paths.is_empty() {
            bail!(
                "no cycle manifest completed; failed_tasks={} skipped_tasks={}",
                graph_outcome.failures.len(),
                graph_outcome.skipped_tasks.len()
            );
        }
        for task_id in &publish_ready_task_ids {
            if !task_values.contains_key(task_id) {
                master_log.log(format!(
                    "product-scheduler-finalize missing_publish_ready_task {task_id}"
                ))?;
            }
        }
        master_log.log(format!(
            "product-scheduler-finalize successful_cycles={} failed_tasks={} skipped_tasks={}",
            cycle_manifest_paths.len(),
            graph_outcome.failures.len(),
            graph_outcome.skipped_tasks.len()
        ))?;

        let as_of_utc = Utc::now();
        let diagnostics = write_product_build_diagnostics(
            &config.packaged_dir,
            as_of_utc.date_naive(),
            &task_values,
        )?;
        let product_artifacts_path =
            write_current_artifacts_manifest(&config.packaged_dir, as_of_utc, diagnostics.clone())?;
        cleanup_published_packaged_root(&config.packaged_dir, &product_artifacts_path)?;
        let diagnostic_error_count = diagnostics
            .as_ref()
            .map(|value| value.error_count)
            .unwrap_or(0);
        if diagnostic_error_count > 0 {
            master_log.log(format!(
                "product-scheduler-finalize current-artifacts published ERROR diagnostic_errors={diagnostic_error_count}"
            ))?;
        } else {
            master_log.log(
                "product-scheduler-finalize current-artifacts published diagnostic_errors=0",
            )?;
        }

        let current_artifacts = load_current_artifacts_manifest(&product_artifacts_path)?;
        for bundle_ref in current_artifacts
            .bundles
            .iter()
            .filter(|bundle| bundle.bundle_type == "cycle")
        {
            let bundle_manifest_path = config.packaged_dir.join(&bundle_ref.filename);
            let bundle_manifest = load_bundle_manifest(&bundle_manifest_path)?;
            let cycle = bundle_manifest.cycle.clone();
            let mut cycle_config = config.clone();
            cycle_config.target_cycle = Some(cycle.clone());
            let built_this_run =
                task_values.contains_key(&cycle_task_id(&cycle, "bundle-manifest"));
            sync_unpacked_metadata(
                &cycle_config,
                &bundle_manifest,
                &bundle_manifest_path,
                built_this_run.then_some(&task_values),
            )?;
        }
        let static_products = static_product_task_ids(config)
            .iter()
            .map(|task_id| match task_values.get(task_id) {
                Some(ProductTaskValue::PublishedStandaloneProduct {
                    id,
                    unpack_source_root,
                    published_zip,
                    sha256,
                    ..
                }) => {
                    let unpack_strategy = static_product_unpacked_strategy(id, unpack_source_root)?;
                    Ok(PublishedZipArtifact {
                        unpack_strategy,
                        published_zip_path: published_zip.clone(),
                        checksum_sha256: sha256.clone(),
                    })
                }
                _ => bail!("missing published static product output for {}", task_id),
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        sync_product_level_unpacked(
            &config.packaged_dir,
            &product_artifacts_path,
            &static_products,
        )?;
        validate_packaged_contract(&config.packaged_dir, &product_artifacts_path)?;
        let unpacked_root = published_unpacked_root(config)?;
        validate_unpacked_contract(
            &config.packaged_dir,
            &unpacked_root,
            &product_artifacts_path,
        )?;
        record_gc_roots(config, "full", &task_node_records)?;

        let build_result = ProductBuildResult {
            cycle_manifest_paths,
            product_artifacts_path,
        };
        if !graph_outcome.failures.is_empty() || !graph_outcome.skipped_tasks.is_empty() {
            write_build_status_html(config, &build_result.product_artifacts_path)?;
            bail!(
                "product publication completed with failed attempted tasks; product_artifacts={} failed_tasks={} skipped_tasks={}",
                build_result.product_artifacts_path.display(),
                graph_outcome.failures.len(),
                graph_outcome.skipped_tasks.len()
            );
        }

        Ok(build_result)
    })();

    match result {
        Ok(result) => {
            write_build_status_html(config, &result.product_artifacts_path)?;
            master_log.log(format!(
                "complete PASS product_artifacts={}",
                result.product_artifacts_path.display()
            ))?;
            Ok(result)
        }
        Err(err) => {
            master_log.log(format!("complete FAIL error={err}"))?;
            Err(err)
        }
    }
}
