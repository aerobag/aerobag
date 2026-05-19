use super::*;

pub fn build_cycle(config: &ProductBuildConfig) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(&config.build_root)
        .with_context(|| format!("failed to create {}", config.build_root.display()))?;
    let log_root = artifact_root_from_build_root(&config.build_root)
        .join("private-work")
        .join("orchestrator-logs")
        .join(if config.profile == ProductBuildProfile::Production {
            "published_packaged"
        } else {
            "published_packaged_validation"
        });
    fs::create_dir_all(&log_root)
        .with_context(|| format!("failed to create {}", log_root.display()))?;
    let mut master_log = MasterLog::create(&log_root.join("master.log"))?;
    master_log.log(format!(
        "begin pid={} profile={} build_root={} scheduler=weighted_dag scheduler_version=2 max_heavy_jobs={} cpu_jobs={} fetch_jobs={} fetch_cache_mode={}",
        std::process::id(),
        config.profile.as_str(),
        config.build_root.display(),
        config.max_heavy_jobs,
        config.cpu_jobs,
        config.fetch_jobs,
        config.fetch_cache_mode
    ))?;

    let result = (|| -> anyhow::Result<PathBuf> {
        let mut node_records = Vec::new();
        let (source_urls_dir, source_urls_record) = build_source_urls_node(config)?;
        master_log.log(format!(
            "complete source-urls cache_hit={}",
            source_urls_record.cache_hit
        ))?;
        node_records.push(normalize_node_record_paths(
            source_urls_record,
            &config.build_root,
        ));

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
        master_log.log(format!(
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
        ))?;

        let chart_families = [
            ChartFamily::Sec,
            ChartFamily::Tac,
            ChartFamily::EnrL,
            ChartFamily::EnrH,
        ];
        let work_unit_budget = config.max_heavy_jobs.max(1) * 4 + 2;
        let mut pending_tasks = Vec::new();
        for family in chart_families {
            let family_id = family_slug(family).to_string();
            pending_tasks.push(GraphScheduledTask {
                id: format!("charts-{family_id}-render"),
                deps: vec![],
                weight: 4,
                kind: ScheduledTaskKind::ChartRender { family },
            });
            pending_tasks.push(GraphScheduledTask {
                id: format!("charts-{family_id}-package"),
                deps: vec![format!("charts-{family_id}-render")],
                weight: 1,
                kind: ScheduledTaskKind::ChartPackage { family },
            });
            for region in Region::ALL {
                pending_tasks.push(GraphScheduledTask {
                    id: format!(
                        "charts-{}-unpack-{}",
                        family_id,
                        region.code().to_ascii_lowercase()
                    ),
                    deps: vec![format!("charts-{family_id}-package")],
                    weight: 1,
                    kind: ScheduledTaskKind::ChartUnpack { family, region },
                });
            }
        }
        pending_tasks.push(GraphScheduledTask {
            id: "csup-stage".to_string(),
            deps: vec![],
            weight: 1,
            kind: ScheduledTaskKind::CsupStage,
        });
        let mut csup_render_ids = Vec::new();
        for region in Region::ALL {
            let region_id = region.code().to_ascii_lowercase();
            let task_id = format!("csup-render-{region_id}");
            csup_render_ids.push(task_id.clone());
            pending_tasks.push(GraphScheduledTask {
                id: task_id,
                deps: vec!["csup-stage".to_string()],
                weight: 2,
                kind: ScheduledTaskKind::CsupRender { region },
            });
        }
        pending_tasks.push(GraphScheduledTask {
            id: "csup-package".to_string(),
            deps: csup_render_ids.clone(),
            weight: 1,
            kind: ScheduledTaskKind::CsupPackage,
        });
        for region in Region::ALL {
            pending_tasks.push(GraphScheduledTask {
                id: format!("csup-unpack-{}", region.code().to_ascii_lowercase()),
                deps: vec!["csup-package".to_string()],
                weight: 1,
                kind: ScheduledTaskKind::CsupUnpack { region },
            });
        }
        let mut tpp_package_ids = Vec::new();
        for region in config.profile.tpp_regions() {
            let region_id = region.code().to_ascii_lowercase();
            let render_id = format!("tpp-{region_id}");
            let package_id = format!("tpp-{region_id}-package");
            pending_tasks.push(GraphScheduledTask {
                id: render_id.clone(),
                deps: vec![],
                weight: TPP_RENDER_WEIGHT,
                kind: ScheduledTaskKind::TppRender { region: *region },
            });
            pending_tasks.push(GraphScheduledTask {
                id: package_id.clone(),
                deps: vec![render_id],
                weight: 1,
                kind: ScheduledTaskKind::TppPackage { region: *region },
            });
            pending_tasks.push(GraphScheduledTask {
                id: format!("tpp-{region_id}-unpack"),
                deps: vec![package_id.clone()],
                weight: 1,
                kind: ScheduledTaskKind::TppUnpack { region: *region },
            });
            tpp_package_ids.push(package_id);
        }
        pending_tasks.push(GraphScheduledTask {
            id: "data-base".to_string(),
            deps: vec![],
            weight: 4,
            kind: ScheduledTaskKind::DataBase,
        });
        pending_tasks.push(GraphScheduledTask {
            id: "data".to_string(),
            deps: {
                let mut deps = vec!["data-base".to_string()];
                deps.extend(tpp_package_ids.iter().cloned());
                deps
            },
            weight: 1,
            kind: ScheduledTaskKind::DataMatch,
        });
        pending_tasks.push(GraphScheduledTask {
            id: "vectors".to_string(),
            deps: vec!["data".to_string()],
            weight: 1,
            kind: ScheduledTaskKind::Vectors,
        });
        pending_tasks.push(GraphScheduledTask {
            id: "data-unpack".to_string(),
            deps: vec!["data".to_string()],
            weight: 1,
            kind: ScheduledTaskKind::DataUnpack,
        });
        let mut resource_index_deps = chart_families
            .iter()
            .map(|family| format!("charts-{}-package", family_slug(*family)))
            .collect::<Vec<_>>();
        resource_index_deps.push("csup-package".to_string());
        resource_index_deps.extend(tpp_package_ids.iter().cloned());
        resource_index_deps.push("data".to_string());
        pending_tasks.push(GraphScheduledTask {
            id: "resource-index".to_string(),
            deps: resource_index_deps,
            weight: 2,
            kind: ScheduledTaskKind::ResourceIndex,
        });

        master_log.log(format!(
            "scheduler-ready tasks={} work_unit_budget={} chart_and_data_weight=4 csup_weight=2 tpp_weight={} tpp_render_jobs_per_run={} light_weight=1 resource_index_weight=2",
            pending_tasks.len(), work_unit_budget, TPP_RENDER_WEIGHT, TPP_RENDER_JOBS_PER_RUN
        ))?;

        let config_for_tasks = config.clone();
        let source_urls_dir_for_tasks = source_urls_dir.clone();
        let chart_versions_for_tasks = chart_versions.clone();
        let csup_version_for_tasks = csup_version.clone();
        let tpp_versions_for_tasks = tpp_versions.clone();
        let data_version_for_tasks = data_version.clone();
        let bundle_cycle_for_tasks = bundle_cycle.clone();
        let (_task_values, task_node_records) = run_weighted_task_graph(
            "cycle-scheduler",
            pending_tasks,
            work_unit_budget,
            |message| master_log.log(message),
            move |kind, task_values_snapshot, _task_node_records_snapshot| {
                let config = config_for_tasks.clone();
                let source_urls_dir = source_urls_dir_for_tasks.clone();
                let chart_versions = chart_versions_for_tasks.clone();
                let csup_version = csup_version_for_tasks.clone();
                let tpp_versions = tpp_versions_for_tasks.clone();
                let data_version = data_version_for_tasks.clone();
                let bundle_cycle = bundle_cycle_for_tasks.clone();
                match kind {
                    ScheduledTaskKind::ChartRender { family } => {
                        let family_id = family_slug(family).to_string();
                        let record = build_chart_render_node(
                            &config,
                            family,
                            &config.chart_cutline_root,
                            &source_urls_dir.join(format!("charts-{family_id}/source_urls.jsonl")),
                            config.fetch_jobs,
                            config.cpu_jobs.min(8).max(1),
                        )
                        .map(|record| TaskCompletion {
                            node_records: vec![record],
                            value: TaskValue::None,
                            completion_detail: "cache_or_rebuild".to_string(),
                        });
                        record
                    }
                    ScheduledTaskKind::CsupStage => {
                        let record = build_csup_stage_node(
                            &config,
                            Path::new(""),
                            &source_urls_dir.join("csup/source_urls.jsonl"),
                            config.fetch_jobs,
                        )
                        .and_then(|record| {
                            let work_dir =
                                resolve_artifact_path(&config, output_path(&record, "work_dir")?);
                            Ok(TaskCompletion {
                                node_records: vec![record.clone()],
                                value: TaskValue::CsupStage { record, work_dir },
                                completion_detail: "cache_or_rebuild".to_string(),
                            })
                        });
                        record
                    }
                    ScheduledTaskKind::CsupRender { region } => {
                        let stage = match task_values_snapshot.get("csup-stage") {
                            Some(TaskValue::CsupStage { record, work_dir }) => (record, work_dir),
                            _ => unreachable!("csup-stage dependency should have completed"),
                        };
                        build_csup_render_node(
                            &config,
                            region,
                            stage.1,
                            &stage.0.fingerprint,
                            &csup_version,
                            config.cpu_jobs.max(1),
                        )
                        .map(|record| TaskCompletion {
                            node_records: vec![record],
                            value: TaskValue::None,
                            completion_detail: "cache_or_rebuild".to_string(),
                        })
                    }
                    ScheduledTaskKind::TppRender { region } => {
                        let region_id = region.code().to_ascii_lowercase();
                        let request = NativeTppRunRequest {
                            region,
                            source_repo: PathBuf::new(),
                            run_root: PathBuf::new(),
                            prefetch_source_urls: Some(
                                source_urls_dir.join(format!("tpp-{region_id}/source_urls.jsonl")),
                            ),
                            fetch_jobs: config.fetch_jobs,
                            render_jobs: TPP_RENDER_JOBS_PER_RUN,
                            fetch_cache: Some(static_source_fetch_cache_config(&config)?),
                        };
                        build_tpp_render_node(&config, &request).map(|record| TaskCompletion {
                            node_records: vec![record],
                            value: TaskValue::None,
                            completion_detail: "cache_or_rebuild".to_string(),
                        })
                    }
                    ScheduledTaskKind::DataBase => build_data_nodes(
                        &config,
                        &source_urls_dir,
                        "data-base",
                    )
                    .and_then(|records| {
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
                        let zip = resolve_artifact_path(&config, output_path(&data_record, "zip")?);
                        let intermediate_sqlite_db =
                            resolve_artifact_path(&config, sqlite_output_path(&data_record)?);
                        let source_input_dir = resolve_artifact_path(
                            &config,
                            output_path(&staging_record, "staged_input_dir")?,
                        );
                        Ok(TaskCompletion {
                            node_records: records,
                            value: TaskValue::FingerprintedData {
                                intermediate_sqlite_db,
                                source_input_dir,
                                zip,
                                fingerprint: data_record.fingerprint,
                            },
                            completion_detail: "cache_or_rebuild".to_string(),
                        })
                    }),
                    ScheduledTaskKind::DataMatch => {
                        let raw_data = match task_values_snapshot.get("data-base") {
                            Some(TaskValue::FingerprintedData {
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
                            _ => unreachable!("data-base dependency should have completed"),
                        };
                        let tpp_sources = config
                            .profile
                            .tpp_regions()
                            .iter()
                            .map(|region| {
                                let region_id = region.code().to_ascii_lowercase();
                                let key = format!("tpp-{region_id}-package");
                                match task_values_snapshot.get(&key) {
                                    Some(TaskValue::FingerprintedTppSource {
                                        source,
                                        fingerprint,
                                    }) => Ok((*region, source.clone(), fingerprint.clone())),
                                    _ => bail!("missing tpp package source for {region_id}"),
                                }
                            })
                            .collect::<anyhow::Result<Vec<_>>>()?;
                        let record = build_data_match_node(
                            &config,
                            &raw_data.0,
                            &raw_data.2,
                            &data_version,
                            &raw_data.3,
                            &tpp_sources,
                        )?;
                        let cache_hit = record.cache_hit;
                        let zip = resolve_artifact_path(&config, output_path(&record, "zip")?);
                        let intermediate_sqlite_db =
                            resolve_artifact_path(&config, sqlite_output_path(&record)?);
                        let fingerprint = record.fingerprint.clone();
                        Ok(TaskCompletion {
                            node_records: vec![record],
                            value: TaskValue::FingerprintedData {
                                intermediate_sqlite_db,
                                source_input_dir: raw_data.1,
                                zip,
                                fingerprint,
                            },
                            completion_detail: format!("cache_hit={}", cache_hit),
                        })
                    }
                    ScheduledTaskKind::ChartPackage { family } => {
                        let family_id = family_slug(family).to_string();
                        let started = Instant::now();
                        let (records, source) = build_chart_package_nodes(
                            &config,
                            family,
                            &source_urls_dir,
                            chart_versions
                                .get(&family_id)
                                .expect("chart family version should exist"),
                        )?;
                        let summary = summarize_package_records(&records);
                        Ok(TaskCompletion {
                            node_records: records,
                            value: TaskValue::ChartSource(source),
                            completion_detail: format!(
                                "elapsed_ms={} regions={} cache_hits={} rebuilt={}",
                                started.elapsed().as_millis(),
                                summary.total,
                                summary.cache_hits,
                                summary.rebuilt,
                            ),
                        })
                    }
                    ScheduledTaskKind::CsupPackage => {
                        let started = Instant::now();
                        let (records, source) =
                            build_csup_package_nodes(&config, &source_urls_dir, &csup_version)?;
                        let summary = summarize_package_records(&records);
                        Ok(TaskCompletion {
                            node_records: records,
                            value: TaskValue::CsupSource(source),
                            completion_detail: format!(
                                "elapsed_ms={} regions={} cache_hits={} rebuilt={}",
                                started.elapsed().as_millis(),
                                summary.total,
                                summary.cache_hits,
                                summary.rebuilt,
                            ),
                        })
                    }
                    ScheduledTaskKind::TppPackage { region } => {
                        let region_id = region.code().to_ascii_lowercase();
                        let started = Instant::now();
                        let (record, source) = build_tpp_package_node(
                            &config,
                            region,
                            &source_urls_dir.join(format!("tpp-{region_id}/source_urls.jsonl")),
                            tpp_versions
                                .get(&region_id)
                                .expect("tpp region version should exist"),
                        )?;
                        let cache_hit = record.cache_hit;
                        let fingerprint = record.fingerprint.clone();
                        Ok(TaskCompletion {
                            node_records: vec![record],
                            value: TaskValue::FingerprintedTppSource {
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
                    ScheduledTaskKind::Vectors => {
                        let (data, source_input_dir, data_fingerprint) =
                            match task_values_snapshot.get("data") {
                                Some(TaskValue::FingerprintedData {
                                    intermediate_sqlite_db,
                                    source_input_dir,
                                    fingerprint,
                                    ..
                                }) => (intermediate_sqlite_db, source_input_dir, fingerprint),
                                _ => unreachable!("data dependency should have completed"),
                            };
                        let record = build_vectors_node(
                            &config,
                            data,
                            source_input_dir,
                            data_fingerprint,
                            &data_version,
                        )?;
                        let cache_hit = record.cache_hit;
                        Ok(TaskCompletion {
                            node_records: vec![record],
                            value: TaskValue::None,
                            completion_detail: format!("cache_hit={}", cache_hit),
                        })
                    }
                    ScheduledTaskKind::ResourceIndex => {
                        let data_zip = match task_values_snapshot.get("data") {
                            Some(TaskValue::FingerprintedData {
                                intermediate_sqlite_db: _,
                                zip,
                                ..
                            }) => zip.clone(),
                            _ => unreachable!("data dependency should have completed"),
                        };
                        let chart_sources = ["sec", "tac", "enr-l", "enr-h"]
                            .iter()
                            .map(|family_id| {
                                let key = format!("charts-{family_id}-package");
                                match task_values_snapshot.get(&key) {
                                    Some(TaskValue::ChartSource(source)) => Ok(source.clone()),
                                    _ => bail!("missing chart source for {family_id}"),
                                }
                            })
                            .collect::<anyhow::Result<Vec<_>>>()?;
                        let csup_sources = vec![match task_values_snapshot.get("csup-package") {
                            Some(TaskValue::CsupSource(source)) => source.clone(),
                            _ => bail!("missing csup package source"),
                        }];
                        let tpp_sources = config
                            .profile
                            .tpp_regions()
                            .iter()
                            .map(|region| {
                                let region_id = region.code().to_ascii_lowercase();
                                let key = format!("tpp-{region_id}-package");
                                match task_values_snapshot.get(&key) {
                                    Some(TaskValue::FingerprintedTppSource { source, .. }) => {
                                        Ok(source.clone())
                                    }
                                    _ => bail!("missing tpp package source for {region_id}"),
                                }
                            })
                            .collect::<anyhow::Result<Vec<_>>>()?;
                        let record = build_resource_index_node(
                            &config,
                            &data_zip,
                            chart_sources,
                            tpp_sources,
                            csup_sources,
                        )?;
                        let cache_hit = record.cache_hit;
                        Ok(TaskCompletion {
                            node_records: vec![record],
                            value: TaskValue::None,
                            completion_detail: format!("cache_hit={}", cache_hit),
                        })
                    }
                    ScheduledTaskKind::ChartUnpack { family, region } => {
                        let family_id = family_slug(family).to_string();
                        let key = format!("charts-{family_id}-package");
                        let source = match task_values_snapshot.get(&key) {
                            Some(TaskValue::ChartSource(source)) => source.clone(),
                            _ => bail!("missing chart source for {family_id}"),
                        };
                        let package =
                            package_record_for_region(&source.package_outputs_path, region)?;
                        let zip_path = source.package_root.join(&package.zip);
                        let unpacked_root = published_unpacked_root(&config)?;
                        let published_filename = canonical_package_filename(
                            &family_id,
                            &region.code().to_ascii_lowercase(),
                            &package.zip,
                        )?;
                        let (cache_hit, unpack_dir) = sync_unpacked_zip_from_source(
                            &zip_path,
                            &source.unpack_source_root,
                            &unpacked_root,
                            &published_filename,
                            Some(&package.zip_sha256),
                        )?;
                        Ok(TaskCompletion {
                            node_records: vec![],
                            value: TaskValue::None,
                            completion_detail: format!(
                                "cache_hit={} unpack_dir={}",
                                cache_hit,
                                unpack_dir.display()
                            ),
                        })
                    }
                    ScheduledTaskKind::CsupUnpack { region } => {
                        let source = match task_values_snapshot.get("csup-package") {
                            Some(TaskValue::CsupSource(source)) => source.clone(),
                            _ => bail!("missing csup package source"),
                        };
                        let package =
                            package_record_for_region(&source.package_outputs_path, region)?;
                        let zip_path = source.package_root.join(&package.zip);
                        let unpacked_root = published_unpacked_root(&config)?;
                        let published_filename = canonical_package_filename(
                            "csup",
                            &region.code().to_ascii_lowercase(),
                            &package.zip,
                        )?;
                        let (cache_hit, unpack_dir) = sync_unpacked_zip_from_source(
                            &zip_path,
                            &source.unpack_source_root,
                            &unpacked_root,
                            &published_filename,
                            Some(&package.zip_sha256),
                        )?;
                        Ok(TaskCompletion {
                            node_records: vec![],
                            value: TaskValue::None,
                            completion_detail: format!(
                                "cache_hit={} unpack_dir={}",
                                cache_hit,
                                unpack_dir.display()
                            ),
                        })
                    }
                    ScheduledTaskKind::TppUnpack { region } => {
                        let region_id = region.code().to_ascii_lowercase();
                        let key = format!("tpp-{region_id}-package");
                        let source = match task_values_snapshot.get(&key) {
                            Some(TaskValue::FingerprintedTppSource { source, .. }) => {
                                source.clone()
                            }
                            _ => bail!("missing tpp package source for {region_id}"),
                        };
                        let package =
                            package_record_for_region(&source.package_outputs_path, region)?;
                        let zip_path = source.package_root.join(&package.zip);
                        let unpacked_root = published_unpacked_root(&config)?;
                        let published_filename = canonical_package_filename(
                            "tpp",
                            &region.code().to_ascii_lowercase(),
                            &package.zip,
                        )?;
                        let (cache_hit, unpack_dir) = sync_unpacked_zip_from_source(
                            &zip_path,
                            &source.unpack_source_root,
                            &unpacked_root,
                            &published_filename,
                            Some(&package.zip_sha256),
                        )?;
                        Ok(TaskCompletion {
                            node_records: vec![],
                            value: TaskValue::None,
                            completion_detail: format!(
                                "cache_hit={} unpack_dir={}",
                                cache_hit,
                                unpack_dir.display()
                            ),
                        })
                    }
                    ScheduledTaskKind::DataUnpack => {
                        let zip = match task_values_snapshot.get("data") {
                            Some(TaskValue::FingerprintedData {
                                intermediate_sqlite_db: _,
                                zip,
                                ..
                            }) => zip.clone(),
                            _ => bail!("missing data zip"),
                        };
                        let unpacked_root = published_unpacked_root(&config)?;
                        let (cache_hit, unpack_dir) = sync_unpacked_zip_from_source(
                            &zip,
                            zip.parent().unwrap_or_else(|| Path::new("/")),
                            &unpacked_root,
                            &format!("data_{bundle_cycle}.zip"),
                            None,
                        )?;
                        Ok(TaskCompletion {
                            node_records: vec![],
                            value: TaskValue::None,
                            completion_detail: format!(
                                "cache_hit={} unpack_dir={}",
                                cache_hit,
                                unpack_dir.display()
                            ),
                        })
                    }
                }
            },
        )?;
        for records in task_node_records.values() {
            for record in records {
                node_records.push(normalize_node_record_paths(
                    record.clone(),
                    &config.build_root,
                ));
            }
        }

        node_records.sort_by(|left, right| left.name.cmp(&right.name));
        node_records.sort_by(|left, right| left.name.cmp(&right.name));

        let build_manifest = BuildManifest {
            schema_version: 1,
            profile: config.profile.as_str().to_string(),
            cycle: bundle_cycle.clone(),
            build_root: relative_product_build_path(&config.build_root),
            generated_at_utc: manifest_generated_at(&node_records),
            fetch_cache_root: relative_artifact_path(&config.fetch_cache_root, &config.build_root),
            fetch_cache_mode: config.fetch_cache_mode.clone(),
            nodes: node_records,
        };
        let build_manifest_path = internal_build_manifest_path(config, &bundle_cycle)?;
        fs::write(
            &build_manifest_path,
            serde_json::to_vec_pretty(&build_manifest)
                .context("failed to encode product build manifest")?,
        )
        .with_context(|| format!("failed to write {}", build_manifest_path.display()))?;

        let resource_index_record = build_manifest
            .nodes
            .iter()
            .find(|node| node.name == "resource-index")
            .context("build manifest missing resource-index node")?;
        let data_record = build_manifest
            .nodes
            .iter()
            .find(|node| node.name == "data")
            .context("build manifest missing data node")?;
        let vectors_record = build_manifest
            .nodes
            .iter()
            .find(|node| node.name == "vectors")
            .context("build manifest missing vectors node")?;
        let resource_index_path = resolve_artifact_path(
            config,
            output_path(resource_index_record, "resource_index")?,
        );
        let intermediate_sqlite_db =
            resolve_artifact_path(config, sqlite_output_path(data_record)?);
        let vector_had_pairs_path =
            resolve_artifact_path(config, output_path(vectors_record, "had_pairs")?);
        let wmm_source = build_wmm_source_node(config)?;
        let nav_db = build_nav_kv_artifact(
            config,
            &resource_index_path,
            &intermediate_sqlite_db,
            &bundle_cycle,
            &vector_had_pairs_path,
            &wmm_source.cof_path,
            &wmm_source.metadata_path,
            &[],
            &[],
        )?;
        let bundle_manifest = build_bundle_manifest(config, &build_manifest, &[], &nav_db.package)?;
        let bundle_manifest_path =
            write_hashed_bundle_manifest(&config.build_root, &bundle_manifest)?;
        validate_bundle_manifest(&config.build_root, &bundle_manifest_path)?;
        sync_unpacked_metadata(config, &bundle_manifest, &bundle_manifest_path, None)?;
        record_gc_roots_from_build_manifest(
            config,
            &format!("cycle:{bundle_cycle}"),
            &build_manifest,
        )?;
        Ok(bundle_manifest_path)
    })();

    match result {
        Ok(manifest_path) => {
            master_log.log(format!(
                "complete PASS manifest={}",
                manifest_path.display()
            ))?;
            Ok(manifest_path)
        }
        Err(err) => {
            master_log.log(format!("complete FAIL error={err}"))?;
            Err(err)
        }
    }
}
