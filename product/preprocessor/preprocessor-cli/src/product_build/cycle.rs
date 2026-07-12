use super::*;

fn tpp_render_tasks_for_plan(
    plan_task_id: &str,
    region: Region,
    plan: &TppRegionRenderPlan,
) -> Vec<GraphScheduledTask<ScheduledTaskKind>> {
    let mut render_ids = Vec::with_capacity(plan.units().len());
    let mut tasks = Vec::with_capacity(plan.units().len() + 1);
    for unit in plan.units() {
        let render_id = tpp_render_unit_task_name(region, unit);
        render_ids.push(render_id.clone());
        tasks.push(GraphScheduledTask {
            id: render_id,
            deps: vec![plan_task_id.to_string()],
            weight: TPP_RENDER_UNIT_WEIGHT,
            kind: ScheduledTaskKind::TppRenderUnit {
                region,
                unit: unit.clone(),
            },
        });
    }
    tasks.push(GraphScheduledTask {
        id: tpp_render_assemble_task_name(region),
        deps: render_ids,
        weight: LIGHT_TASK_WEIGHT,
        kind: ScheduledTaskKind::TppRenderAssemble { region },
    });
    tasks
}

fn tpp_package_tasks_for_plan(
    plan_task_id: &str,
    region: Region,
    plan: &TppPackagePlan,
) -> Vec<GraphScheduledTask<ScheduledTaskKind>> {
    let mut thumbnail_ids = Vec::with_capacity(plan.thumbnails.len());
    let mut tasks = Vec::with_capacity(plan.thumbnails.len() + 1);
    for thumbnail in &plan.thumbnails {
        let thumbnail_id = tpp_thumbnail_task_name(region, thumbnail);
        thumbnail_ids.push(thumbnail_id.clone());
        tasks.push(GraphScheduledTask {
            id: thumbnail_id,
            deps: vec![plan_task_id.to_string()],
            weight: TPP_THUMBNAIL_WEIGHT,
            kind: ScheduledTaskKind::TppThumbnail {
                region,
                thumbnail: thumbnail.clone(),
            },
        });
    }
    let mut package_deps = thumbnail_ids;
    package_deps.push(plan_task_id.to_string());
    tasks.push(GraphScheduledTask {
        id: format!("tpp-{}-package", region.code().to_ascii_lowercase()),
        deps: package_deps,
        weight: LIGHT_TASK_WEIGHT,
        kind: ScheduledTaskKind::TppPackage { region },
    });
    tasks
}

pub fn build_cycle(config: &ProductBuildConfig) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(&config.packaged_dir)
        .with_context(|| format!("failed to create {}", config.packaged_dir.display()))?;
    let log_root = orchestrator_log_root(config)?;
    let mut master_log = MasterLog::create(&log_root.join("master.log"))?;
    master_log.log(format!(
        "begin pid={} build_root={} publish_dir={} publish_label={} publish_timestamp={} scheduler=weighted_dag scheduler_version=2 max_heavy_jobs={} cpu_jobs={} fetch_jobs={} fetch_cache_mode={}",
        std::process::id(),
        config.build_root.display(),
        config.publish_dir.display(),
        config.publish_label,
        config.publish_timestamp,
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
            &config.packaged_dir,
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
        let tpp_versions = Region::ALL
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
            Region::ALL
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
        let work_unit_budget = (config.max_heavy_jobs.max(1) * 4 + 2) * SCHEDULER_WEIGHT_SCALE;
        let mut pending_tasks = Vec::new();
        for family in chart_families {
            let family_id = family_slug(family).to_string();
            let fetch_id = format!("charts-{family_id}-fetch");
            let process_id = format!("charts-{family_id}-process");
            pending_tasks.push(GraphScheduledTask {
                id: fetch_id.clone(),
                deps: vec![],
                weight: LIGHT_TASK_WEIGHT,
                kind: ScheduledTaskKind::ChartFetch { family },
            });
            pending_tasks.push(GraphScheduledTask {
                id: process_id.clone(),
                deps: vec![fetch_id],
                weight: CHART_PROCESS_WEIGHT,
                kind: ScheduledTaskKind::ChartProcess { family },
            });
            pending_tasks.push(GraphScheduledTask {
                id: format!("charts-{family_id}-package"),
                deps: vec![process_id, format!("charts-{family_id}-fetch")],
                weight: LIGHT_TASK_WEIGHT,
                kind: ScheduledTaskKind::ChartPackage { family },
            });
            for region in Region::ALL.iter() {
                pending_tasks.push(GraphScheduledTask {
                    id: format!(
                        "charts-{}-unpack-{}",
                        family_id,
                        region.code().to_ascii_lowercase()
                    ),
                    deps: vec![format!("charts-{family_id}-package")],
                    weight: LIGHT_TASK_WEIGHT,
                    kind: ScheduledTaskKind::ChartUnpack {
                        family,
                        region: *region,
                    },
                });
            }
        }
        pending_tasks.push(GraphScheduledTask {
            id: "csup-fetch".to_string(),
            deps: vec![],
            weight: LIGHT_TASK_WEIGHT,
            kind: ScheduledTaskKind::CsupFetch,
        });
        pending_tasks.push(GraphScheduledTask {
            id: "csup-process".to_string(),
            deps: vec!["csup-fetch".to_string()],
            weight: LIGHT_TASK_WEIGHT,
            kind: ScheduledTaskKind::CsupProcess,
        });
        let mut csup_render_ids = Vec::new();
        for region in Region::ALL.iter() {
            let region_id = region.code().to_ascii_lowercase();
            let task_id = format!("csup-render-{region_id}");
            csup_render_ids.push(task_id.clone());
            pending_tasks.push(GraphScheduledTask {
                id: task_id,
                deps: vec!["csup-process".to_string()],
                weight: CSUP_RENDER_WEIGHT,
                kind: ScheduledTaskKind::CsupRender { region: *region },
            });
        }
        pending_tasks.push(GraphScheduledTask {
            id: "csup-package".to_string(),
            deps: csup_render_ids
                .iter()
                .cloned()
                .chain(std::iter::once("csup-fetch".to_string()))
                .collect(),
            weight: LIGHT_TASK_WEIGHT,
            kind: ScheduledTaskKind::CsupPackage,
        });
        for region in Region::ALL.iter() {
            pending_tasks.push(GraphScheduledTask {
                id: format!("csup-unpack-{}", region.code().to_ascii_lowercase()),
                deps: vec!["csup-package".to_string()],
                weight: LIGHT_TASK_WEIGHT,
                kind: ScheduledTaskKind::CsupUnpack { region: *region },
            });
        }
        pending_tasks.push(GraphScheduledTask {
            id: "tpp-fetch".to_string(),
            deps: vec![],
            weight: LIGHT_TASK_WEIGHT,
            kind: ScheduledTaskKind::TppFetch,
        });
        let mut tpp_package_ids = Vec::new();
        for region in Region::ALL.iter() {
            let region_id = region.code().to_ascii_lowercase();
            let plan_id = format!("tpp-{region_id}-plan");
            let assemble_id = tpp_render_assemble_task_name(*region);
            let package_plan_id = tpp_package_plan_task_name(*region);
            let package_id = format!("tpp-{region_id}-package");
            pending_tasks.push(GraphScheduledTask {
                id: plan_id,
                deps: vec!["tpp-fetch".to_string()],
                weight: LIGHT_TASK_WEIGHT,
                kind: ScheduledTaskKind::TppPlan { region: *region },
            });
            pending_tasks.push(GraphScheduledTask {
                id: package_plan_id,
                deps: vec![assemble_id],
                weight: LIGHT_TASK_WEIGHT,
                kind: ScheduledTaskKind::TppPackagePlan { region: *region },
            });
            pending_tasks.push(GraphScheduledTask {
                id: format!("tpp-{region_id}-unpack"),
                deps: vec![package_id.clone()],
                weight: LIGHT_TASK_WEIGHT,
                kind: ScheduledTaskKind::TppUnpack { region: *region },
            });
            tpp_package_ids.push(package_id);
        }
        pending_tasks.push(GraphScheduledTask {
            id: "data-base".to_string(),
            deps: vec![],
            weight: DATA_BASE_WEIGHT,
            kind: ScheduledTaskKind::DataBase,
        });
        pending_tasks.push(GraphScheduledTask {
            id: "data".to_string(),
            deps: {
                let mut deps = vec!["data-base".to_string()];
                deps.extend(tpp_package_ids.iter().cloned());
                deps
            },
            weight: LIGHT_TASK_WEIGHT,
            kind: ScheduledTaskKind::DataMatch,
        });
        pending_tasks.push(GraphScheduledTask {
            id: "vectors".to_string(),
            deps: vec!["data".to_string()],
            weight: LIGHT_TASK_WEIGHT,
            kind: ScheduledTaskKind::Vectors,
        });
        pending_tasks.push(GraphScheduledTask {
            id: "data-unpack".to_string(),
            deps: vec!["data".to_string()],
            weight: LIGHT_TASK_WEIGHT,
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
            weight: RESOURCE_INDEX_WEIGHT,
            kind: ScheduledTaskKind::ResourceIndex,
        });

        master_log.log(format!(
            "scheduler-ready tasks={} work_unit_budget={} weight_scale={} chart_and_data_weight={} csup_weight={} tpp_unit_weight={} tpp_thumbnail_weight={} light_weight={} resource_index_weight={}",
            pending_tasks.len(),
            work_unit_budget,
            SCHEDULER_WEIGHT_SCALE,
            CHART_PROCESS_WEIGHT,
            CSUP_RENDER_WEIGHT,
            TPP_RENDER_UNIT_WEIGHT,
            TPP_THUMBNAIL_WEIGHT,
            LIGHT_TASK_WEIGHT,
            RESOURCE_INDEX_WEIGHT
        ))?;

        let config_for_tasks = config.clone();
        let source_urls_dir_for_tasks = source_urls_dir.clone();
        let chart_versions_for_tasks = chart_versions.clone();
        let csup_version_for_tasks = csup_version.clone();
        let tpp_versions_for_tasks = tpp_versions.clone();
        let data_version_for_tasks = data_version.clone();
        let bundle_cycle_for_tasks = bundle_cycle.clone();
        let (_task_values, task_node_records) = run_weighted_task_graph_with_expansion(
            "cycle-scheduler",
            pending_tasks,
            work_unit_budget,
            |message| master_log.log(message),
            move |kind, task_values_snapshot, task_node_records_snapshot| {
                let config = config_for_tasks.clone();
                let source_urls_dir = source_urls_dir_for_tasks.clone();
                let chart_versions = chart_versions_for_tasks.clone();
                let csup_version = csup_version_for_tasks.clone();
                let tpp_versions = tpp_versions_for_tasks.clone();
                let data_version = data_version_for_tasks.clone();
                let bundle_cycle = bundle_cycle_for_tasks.clone();
                match kind {
                    ScheduledTaskKind::ChartFetch { family } => {
                        let family_id = family_slug(family).to_string();
                        let record = build_chart_fetch_node(
                            &config,
                            family,
                            &source_urls_dir.join(format!("charts-{family_id}/source_urls.jsonl")),
                            config.fetch_jobs,
                        )
                        .map(|record| TaskCompletion {
                            node_records: vec![record.clone()],
                            value: TaskValue::ChartFetch { record },
                            completion_detail: "cache_or_rebuild".to_string(),
                        });
                        record
                    }
                    ScheduledTaskKind::ChartProcess { family } => {
                        let family_id = family_slug(family).to_string();
                        let source_fetch =
                            match task_values_snapshot.get(&format!("charts-{family_id}-fetch")) {
                                Some(TaskValue::ChartFetch { record }) => record,
                                _ => unreachable!("chart fetch dependency should have completed"),
                            };
                        let record = build_chart_process_node(
                            &config,
                            family,
                            &config.chart_metadata_root,
                            &source_urls_dir.join(format!("charts-{family_id}/source_urls.jsonl")),
                            &source_fetch,
                            config.cpu_jobs.min(8).max(1),
                        )
                        .map(|record| TaskCompletion {
                            node_records: vec![record],
                            value: TaskValue::None,
                            completion_detail: "cache_or_rebuild".to_string(),
                        });
                        record
                    }
                    ScheduledTaskKind::CsupFetch => {
                        let record = build_csup_fetch_node(
                            &config,
                            &source_urls_dir.join("csup/source_urls.jsonl"),
                            config.fetch_jobs,
                        )
                        .map(|record| TaskCompletion {
                            node_records: vec![record.clone()],
                            value: TaskValue::CsupFetch { record },
                            completion_detail: "cache_or_rebuild".to_string(),
                        });
                        record
                    }
                    ScheduledTaskKind::CsupProcess => {
                        let source_fetch = match task_values_snapshot.get("csup-fetch") {
                            Some(TaskValue::CsupFetch { record }) => record,
                            _ => unreachable!("csup-fetch dependency should have completed"),
                        };
                        let record = build_csup_process_node(
                            &config,
                            Path::new(""),
                            &source_urls_dir.join("csup/source_urls.jsonl"),
                            &source_fetch,
                        )
                        .and_then(|record| {
                            let work_dir =
                                resolve_artifact_path(&config, output_path(&record, "work_dir")?);
                            Ok(TaskCompletion {
                                node_records: vec![record.clone()],
                                value: TaskValue::CsupProcess { record, work_dir },
                                completion_detail: "cache_or_rebuild".to_string(),
                            })
                        });
                        record
                    }
                    ScheduledTaskKind::CsupRender { region } => {
                        let process = match task_values_snapshot.get("csup-process") {
                            Some(TaskValue::CsupProcess { record, work_dir }) => (record, work_dir),
                            _ => unreachable!("csup-process dependency should have completed"),
                        };
                        build_csup_render_node(
                            &config,
                            region,
                            &process.1,
                            &process.0.fingerprint,
                            &csup_version,
                            config.cpu_jobs.max(1),
                        )
                        .map(|record| TaskCompletion {
                            node_records: vec![record],
                            value: TaskValue::None,
                            completion_detail: "cache_or_rebuild".to_string(),
                        })
                    }
                    ScheduledTaskKind::TppFetch => {
                        let record = build_tpp_fetch_node(&config, &source_urls_dir)?;
                        Ok(TaskCompletion {
                            node_records: vec![record.clone()],
                            value: TaskValue::TppFetch { record },
                            completion_detail: "cache_or_rebuild".to_string(),
                        })
                    }
                    ScheduledTaskKind::TppPlan { region } => {
                        let source_fetch = match task_values_snapshot.get("tpp-fetch") {
                            Some(TaskValue::TppFetch { record }) => record,
                            _ => unreachable!("tpp-fetch dependency should have completed"),
                        };
                        let region_id = region.code().to_ascii_lowercase();
                        let source_urls_path =
                            source_urls_dir.join(format!("tpp-{region_id}/source_urls.jsonl"));
                        build_tpp_plan_node(
                            &config,
                            region,
                            &source_urls_path,
                            config.fetch_jobs,
                            Some(&source_fetch),
                        )
                        .map(
                            |(record, source_root, plan, source_content_fingerprint)| {
                                let unit_count = plan.units().len();
                                let cache_hit = record.cache_hit;
                                TaskCompletion {
                                    node_records: vec![record.clone()],
                                    value: TaskValue::TppPlan {
                                        record,
                                        source_root,
                                        plan,
                                        source_content_fingerprint,
                                    },
                                    completion_detail: format!(
                                        "units={} cache_hit={}",
                                        unit_count, cache_hit
                                    ),
                                }
                            },
                        )
                    }
                    ScheduledTaskKind::TppRenderUnit { region, unit } => {
                        let region_id = region.code().to_ascii_lowercase();
                        let plan_id = format!("tpp-{region_id}-plan");
                        let (source_root, source_content_fingerprint) =
                            task_values_snapshot.with(&plan_id, |value| match value {
                                Some(TaskValue::TppPlan {
                                    source_root,
                                    source_content_fingerprint,
                                    ..
                                }) => (source_root.clone(), source_content_fingerprint.clone()),
                                _ => unreachable!("tpp plan dependency should have completed"),
                            });
                        let record = build_tpp_render_unit_node(
                            &config,
                            &region_id,
                            &source_content_fingerprint,
                            &source_root,
                            &unit,
                        )?;
                        let cache_hit = record.cache_hit;
                        Ok(TaskCompletion {
                            node_records: vec![record],
                            value: TaskValue::None,
                            completion_detail: format!("cache_hit={cache_hit}"),
                        })
                    }
                    ScheduledTaskKind::TppRenderAssemble { region } => {
                        let region_id = region.code().to_ascii_lowercase();
                        let plan_id = format!("tpp-{region_id}-plan");
                        let (plan_record, plan) = match task_values_snapshot.get(&plan_id) {
                            Some(TaskValue::TppPlan { record, plan, .. }) => (record, plan),
                            _ => unreachable!("tpp plan dependency should have completed"),
                        };
                        let unit_records = tpp_render_unit_records_for_plan(
                            region,
                            &plan,
                            &task_node_records_snapshot.iter().collect(),
                        )?;
                        let record = build_tpp_render_assemble_node(
                            &config,
                            region,
                            &plan_record,
                            &unit_records,
                        )?;
                        let cache_hit = record.cache_hit;
                        Ok(TaskCompletion {
                            node_records: vec![record.clone()],
                            value: TaskValue::TppRender { record },
                            completion_detail: format!(
                                "units={} cache_hit={}",
                                unit_records.len(),
                                cache_hit
                            ),
                        })
                    }
                    ScheduledTaskKind::TppPackagePlan { region } => {
                        let region_id = region.code().to_ascii_lowercase();
                        let render_record = match task_values_snapshot
                            .get(&tpp_render_assemble_task_name(region))
                        {
                            Some(TaskValue::TppRender { record }) => record,
                            _ => {
                                unreachable!("tpp render assemble dependency should have completed")
                            }
                        };
                        let source_urls_path =
                            source_urls_dir.join(format!("tpp-{region_id}/source_urls.jsonl"));
                        let (record, metadata_root, plate_sources, plan) =
                            build_tpp_package_plan_node(
                                &config,
                                region,
                                &source_urls_path,
                                tpp_versions
                                    .get(&region_id)
                                    .expect("tpp region version should exist"),
                                &render_record,
                            )?;
                        let cache_hit = record.cache_hit;
                        Ok(TaskCompletion {
                            node_records: vec![record.clone()],
                            value: TaskValue::TppPackagePlan {
                                record,
                                metadata_root,
                                plate_sources,
                                plan: plan.clone(),
                            },
                            completion_detail: format!(
                                "plates={} thumbnails={} cache_hit={}",
                                plan.plate_members.len(),
                                plan.thumbnails.len(),
                                cache_hit
                            ),
                        })
                    }
                    ScheduledTaskKind::TppThumbnail { region, thumbnail } => {
                        let region_id = region.code().to_ascii_lowercase();
                        let plan_id = tpp_package_plan_task_name(region);
                        let source_png =
                            task_values_snapshot.with(&plan_id, |value| match value {
                                Some(TaskValue::TppPackagePlan { plate_sources, .. }) => {
                                    plate_sources
                                        .get(&thumbnail.asset_path)
                                        .cloned()
                                        .with_context(|| {
                                            format!(
                                                "missing tpp plate source for {}",
                                                thumbnail.asset_path
                                            )
                                        })
                                }
                                _ => unreachable!(
                                    "tpp package plan dependency should have completed"
                                ),
                            })?;
                        let record =
                            build_tpp_thumbnail_node(&config, region, &source_png, &thumbnail)?;
                        let cache_hit = record.cache_hit;
                        Ok(TaskCompletion {
                            node_records: vec![record],
                            value: TaskValue::None,
                            completion_detail: format!(
                                "region={} thumbnail={} cache_hit={}",
                                region_id, thumbnail.id, cache_hit
                            ),
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
                        let tpp_sources = Region::ALL
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
                        let source_fetch =
                            match task_values_snapshot.get(&format!("charts-{family_id}-fetch")) {
                                Some(TaskValue::ChartFetch { record }) => record,
                                _ => unreachable!("chart fetch dependency should have completed"),
                            };
                        let started = Instant::now();
                        let (records, source) = build_chart_package_nodes(
                            &config,
                            family,
                            &source_urls_dir,
                            chart_versions
                                .get(&family_id)
                                .expect("chart family version should exist"),
                            &source_fetch,
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
                        let source_fetch = match task_values_snapshot.get("csup-fetch") {
                            Some(TaskValue::CsupFetch { record }) => record,
                            _ => unreachable!("csup-fetch dependency should have completed"),
                        };
                        let started = Instant::now();
                        let (records, source) = build_csup_package_nodes(
                            &config,
                            &source_urls_dir,
                            &csup_version,
                            &source_fetch,
                        )?;
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
                        let package_plan_id = tpp_package_plan_task_name(region);
                        let (plan_record, metadata_root, plate_sources, plan) =
                            match task_values_snapshot.get(&package_plan_id) {
                                Some(TaskValue::TppPackagePlan {
                                    record,
                                    metadata_root,
                                    plate_sources,
                                    plan,
                                }) => (record, metadata_root, plate_sources, plan),
                                _ => {
                                    unreachable!(
                                        "tpp package plan dependency should have completed"
                                    )
                                }
                            };
                        let thumbnail_records = tpp_thumbnail_records_for_plan(
                            region,
                            &plan,
                            &task_node_records_snapshot.iter().collect(),
                        )?;
                        let started = Instant::now();
                        let (record, source) = build_tpp_package_assemble_node(
                            &config,
                            region,
                            &source_urls_dir.join(format!("tpp-{region_id}/source_urls.jsonl")),
                            &plan_record,
                            &metadata_root,
                            &plate_sources,
                            &plan,
                            &thumbnail_records,
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
                                "elapsed_ms={} thumbnails={} cache_hit={}",
                                started.elapsed().as_millis(),
                                thumbnail_records.len(),
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
                            &data,
                            &source_input_dir,
                            &data_fingerprint,
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
                        let tpp_sources = Region::ALL
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
            |task_id, kind, completion, _task_values, _task_node_records| match kind {
                ScheduledTaskKind::TppPlan { region } => {
                    let plan = match &completion.value {
                        TaskValue::TppPlan { plan, .. } => plan,
                        _ => unreachable!("tpp plan completion should carry plan value"),
                    };
                    Ok(tpp_render_tasks_for_plan(task_id, *region, plan))
                }
                ScheduledTaskKind::TppPackagePlan { region } => {
                    let plan = match &completion.value {
                        TaskValue::TppPackagePlan { plan, .. } => plan,
                        _ => unreachable!("tpp package plan completion should carry plan value"),
                    };
                    Ok(tpp_package_tasks_for_plan(task_id, *region, plan))
                }
                _ => Ok(Vec::new()),
            },
        )?;
        for records in task_node_records.values() {
            for record in records {
                node_records.push(normalize_node_record_paths(
                    record.clone(),
                    &config.packaged_dir,
                ));
            }
        }

        node_records.sort_by(|left, right| left.name.cmp(&right.name));
        node_records.sort_by(|left, right| left.name.cmp(&right.name));

        let build_manifest = BuildManifest {
            schema_version: 1,
            cycle: bundle_cycle.clone(),
            build_root: config.build_root.display().to_string(),
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
        let mut build_manifest = build_manifest;
        build_manifest.nodes.push(normalize_node_record_paths(
            nav_db.node_record.clone(),
            &config.packaged_dir,
        ));
        build_manifest
            .nodes
            .sort_by(|left, right| left.name.cmp(&right.name));
        build_manifest.generated_at_utc = manifest_generated_at(&build_manifest.nodes);
        fs::write(
            &build_manifest_path,
            serde_json::to_vec_pretty(&build_manifest)
                .context("failed to encode product build manifest")?,
        )
        .with_context(|| format!("failed to write {}", build_manifest_path.display()))?;

        let bundle_manifest = build_bundle_manifest(config, &build_manifest, &[], &nav_db.package)?;
        let bundle_manifest_path =
            write_hashed_bundle_manifest(&config.packaged_dir, &bundle_manifest)?;
        validate_bundle_manifest(&config.packaged_dir, &bundle_manifest_path)?;
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
            master_log.log(format!("complete FAIL error={}", log_error_chain(&err)))?;
            Err(err)
        }
    }
}
