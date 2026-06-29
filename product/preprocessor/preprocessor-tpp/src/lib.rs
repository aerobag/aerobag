use std::{
    collections::hash_map::DefaultHasher,
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::Instant,
};

use anyhow::{bail, Context};
use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use preprocessor_core::{Region, RunPaths};
use preprocessor_fetch::{
    copy_source_urls_provenance, hash_file, prefetch_archives_with_provenance,
    read_source_prefetch_requests_jsonl, FetchCacheConfig, PrefetchRequest,
};
use preprocessor_tools::{
    append_pngs_vertical, command_output_diagnostic_summary, flatten_png_onto_white,
    sanitize_label, ToolInvocation,
};
use serde::{Deserialize, Serialize};

mod package;
pub use package::{
    assemble_package_region, plan_package_region, write_tpp_thumbnail, TppPackagePlan,
    TppThumbnailPlan,
};
use package::{package_region, package_region_versioned};

const TPP_AIRPORT_DIAGRAMS_URL: &str =
    "https://www.outerworldapps.com/WairToNowWork/avare_aptdiags.php";
const TPP_BASIC_PIPELINE_VERSION: &str = "basic-v5-hotspot-stapled";
const TPP_AIRPORT_DIAGRAM_PIPELINE_VERSION: &str = "airport-diagram-v1";
const TPP_CONTINUED_PIPELINE_VERSION: &str = "continued-v6-hotspot-shared-path";
const TPP_GEOTAGGED_PIPELINE_VERSION: &str = "geotagged-v2-dstalpha";
const TPP_MINIMUM_PIPELINE_VERSION: &str = "minimum-v1";
const TPP_RENDER_DPI: &str = "225";
const TPP_DELETED_JOB_PDF_NAME: &str = "DELETED_JOB.PDF";

#[derive(Debug, Clone)]
pub struct NativeTppRunRequest {
    pub region: Region,
    pub source_repo: PathBuf,
    pub run_root: PathBuf,
    pub prefetch_source_urls: Option<PathBuf>,
    pub fetch_jobs: usize,
    pub render_jobs: usize,
    pub fetch_cache: Option<FetchCacheConfig>,
}

#[derive(Debug, Clone)]
pub struct NativeTppRunResult {
    pub work_dir: PathBuf,
    pub prefetch_elapsed_ms: u128,
    pub render_elapsed_ms: u128,
    pub package_elapsed_ms: u128,
    pub package_count: usize,
}

#[derive(Debug, Clone)]
pub struct NativeTppRenderResult {
    pub work_dir: PathBuf,
    pub provenance_dir: PathBuf,
    pub prefetch_elapsed_ms: u128,
    pub render_elapsed_ms: u128,
}

#[derive(Debug, Clone)]
pub struct NativeTppPackageResult {
    pub package_elapsed_ms: u128,
    pub package_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PlateRecord {
    apt_id: String,
    state_id: String,
    chart_name: String,
    chart_code: String,
    pdf_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ContinuedPlateGroup {
    apt_id: String,
    output_name: String,
    members: Vec<PlateRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
enum PlateTask {
    Single(PlateRecord),
    Continued(ContinuedPlateGroup),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
enum PlateRenderKind {
    Minimum,
    AirportDiagram,
    Geotagged,
    Basic,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
enum PlateRotation {
    None,
    Clockwise90,
    CounterClockwise90,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TppRegionRenderPlan {
    units: Vec<TppRenderUnitPlan>,
}

impl TppRegionRenderPlan {
    pub fn units(&self) -> &[TppRenderUnitPlan] {
        &self.units
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TppRenderUnitPlan {
    id: String,
    task: PlannedPlateTask,
}

impl TppRenderUnitPlan {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn source_pdf_names(&self) -> Vec<&str> {
        let mut names = BTreeSet::new();
        match &self.task {
            PlannedPlateTask::Single(plate) => {
                names.insert(plate.record.pdf_name.as_str());
            }
            PlannedPlateTask::Continued(group) => {
                for member in &group.members {
                    names.insert(member.record.pdf_name.as_str());
                }
            }
        }
        names.into_iter().collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
enum PlannedPlateTask {
    Single(PlannedPlate),
    Continued(PlannedContinuedPlateGroup),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PlannedContinuedPlateGroup {
    apt_id: String,
    output_name: String,
    members: Vec<PlannedPlate>,
    legacy_continued_outputs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PlannedPlate {
    record: PlateRecord,
    output_name: String,
    pdf_hash: String,
    render_kind: PlateRenderKind,
    rotation: PlateRotation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    airport_diagram_comment: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    minimum_pages: Vec<u32>,
}

#[derive(Debug, Clone)]
struct PdfPlanningFacts {
    pdf_hash: String,
    non_special_render_kind: Option<PlateRenderKind>,
    landscape_rotation: Option<PlateRotation>,
}

pub fn run_native_tpp(request: &NativeTppRunRequest) -> anyhow::Result<NativeTppRunResult> {
    let render = render_native_tpp(request)?;
    let package = package_native_tpp(
        &render.work_dir,
        &render.work_dir,
        &render.provenance_dir,
        request.region,
    )?;
    Ok(NativeTppRunResult {
        work_dir: render.work_dir,
        prefetch_elapsed_ms: render.prefetch_elapsed_ms,
        render_elapsed_ms: render.render_elapsed_ms,
        package_elapsed_ms: package.package_elapsed_ms,
        package_count: package.package_count,
    })
}

pub fn tpp_prefetch_requests(source_urls_path: &Path) -> anyhow::Result<Vec<PrefetchRequest>> {
    let mut requests = read_source_prefetch_requests_jsonl(source_urls_path)?;
    if !requests
        .iter()
        .any(|request| request.url == TPP_AIRPORT_DIAGRAMS_URL)
    {
        requests.push(PrefetchRequest::new(TPP_AIRPORT_DIAGRAMS_URL));
    }
    Ok(requests)
}

pub fn render_native_tpp(request: &NativeTppRunRequest) -> anyhow::Result<NativeTppRenderResult> {
    let paths = RunPaths::new(&request.run_root);
    fs::create_dir_all(&paths.logs).context("failed to create logs dir")?;
    fs::create_dir_all(&paths.meta).context("failed to create meta dir")?;

    let work_dir = stage_work_dir(&request.source_repo, &request.run_root, request.region)?;
    clean_tpp_transient_work_files(&work_dir)?;
    let provenance_dir = paths.meta.join("provenance").join(format!(
        "tpp-{}",
        request.region.code().to_ascii_lowercase()
    ));
    fs::create_dir_all(&provenance_dir).context("failed to create provenance dir")?;

    let mut prefetch_elapsed_ms = 0_u128;
    if let Some(source_urls_path) = &request.prefetch_source_urls {
        let start = Instant::now();
        copy_source_urls_provenance(source_urls_path, &provenance_dir)?;
        let requests = tpp_prefetch_requests(source_urls_path)?;
        prefetch_archives_with_provenance(
            &requests,
            &work_dir,
            request.fetch_jobs,
            request.fetch_cache.as_ref(),
            &provenance_dir,
            &format!("tpp-{}", request.region.code().to_ascii_lowercase()),
        )?;
        prefetch_elapsed_ms = start.elapsed().as_millis();
    }

    let render_start = Instant::now();
    render_tpp_region(&work_dir, request.region, request.render_jobs)?;
    clean_tpp_transient_work_files(&work_dir)?;
    let render_elapsed_ms = render_start.elapsed().as_millis();

    Ok(NativeTppRenderResult {
        work_dir,
        provenance_dir,
        prefetch_elapsed_ms,
        render_elapsed_ms,
    })
}

pub fn package_native_tpp(
    asset_root: &Path,
    output_root: &Path,
    provenance_dir: &Path,
    region: Region,
) -> anyhow::Result<NativeTppPackageResult> {
    let package_start = Instant::now();
    let package_count = package_region(asset_root, output_root, provenance_dir, region)?;
    Ok(NativeTppPackageResult {
        package_elapsed_ms: package_start.elapsed().as_millis(),
        package_count,
    })
}

pub fn package_native_tpp_versioned(
    asset_root: &Path,
    output_root: &Path,
    provenance_dir: &Path,
    region: Region,
    manifest_version: &str,
    artifact_version: &str,
) -> anyhow::Result<NativeTppPackageResult> {
    let package_start = Instant::now();
    let package_count = package_region_versioned(
        asset_root,
        output_root,
        provenance_dir,
        region,
        manifest_version,
        artifact_version,
    )?;
    Ok(NativeTppPackageResult {
        package_elapsed_ms: package_start.elapsed().as_millis(),
        package_count,
    })
}

fn stage_work_dir(_source_repo: &Path, run_root: &Path, region: Region) -> anyhow::Result<PathBuf> {
    let work_dir = run_root
        .join("work")
        .join(format!("tpp-{}", region.code().to_ascii_lowercase()));
    fs::create_dir_all(&work_dir)
        .with_context(|| format!("failed to create {}", work_dir.display()))?;
    Ok(work_dir)
}

fn clean_tpp_transient_work_files(work_dir: &Path) -> anyhow::Result<()> {
    let imagemagick_tmp = work_dir.join(".tmp-imagemagick");
    if imagemagick_tmp.exists() {
        fs::remove_dir_all(&imagemagick_tmp)
            .with_context(|| format!("failed to remove {}", imagemagick_tmp.display()))?;
    }
    clean_tpp_transient_tree(work_dir)
}

fn clean_tpp_transient_tree(dir: &Path) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            clean_tpp_transient_tree(&path)?;
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.ends_with("_exiftool_tmp") || name.ends_with('~') {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        }
    }
    Ok(())
}

fn render_tpp_region(work_dir: &Path, region: Region, render_jobs: usize) -> anyhow::Result<()> {
    let plan = plan_tpp_region_render(work_dir, work_dir, region)?;
    render_planned_units_parallel(work_dir, plan.units, render_jobs)
}

pub fn plan_tpp_region_render(
    metadata_dir: &Path,
    pdf_root: &Path,
    region: Region,
) -> anyhow::Result<TppRegionRenderPlan> {
    if metadata_dir == pdf_root {
        uppercase_pdf_names(pdf_root)?;
    }
    let xml_path = metadata_dir.join("d-TPP_Metafile.xml");
    let plates = parse_region_plates(&xml_path, region)?;
    let tasks = build_plate_tasks(plates);
    let ad_tags = read_airport_diagram_tags(&pdf_root.join("avare_aptdiags.php"))?;
    let minimum_pages = collect_minimum_pages_by_plate(pdf_root, &tasks)?;
    let pdf_facts = collect_pdf_planning_facts(pdf_root, &tasks)?;
    let units = tasks
        .into_iter()
        .map(|task| plan_plate_task(&ad_tags, &minimum_pages, &pdf_facts, task))
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(TppRegionRenderPlan { units })
}

pub fn render_tpp_unit(
    source_root: &Path,
    work_dir: &Path,
    unit: &TppRenderUnitPlan,
) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(work_dir)
        .with_context(|| format!("failed to create {}", work_dir.display()))?;
    clean_tpp_transient_work_files(work_dir)?;
    for pdf_name in unit.source_pdf_names() {
        hard_link_or_copy_file(&source_root.join(pdf_name), &work_dir.join(pdf_name))?;
    }
    render_planned_unit(work_dir, unit)?;
    clean_tpp_transient_work_files(work_dir)?;
    Ok(work_dir.join("plates"))
}

fn render_planned_units_parallel(
    work_dir: &Path,
    units: Vec<TppRenderUnitPlan>,
    render_jobs: usize,
) -> anyhow::Result<()> {
    let queue = Arc::new(Mutex::new(VecDeque::from(units)));
    let job_count = render_jobs.max(1);
    let mut handles = Vec::with_capacity(job_count);

    for _ in 0..job_count {
        let queue = Arc::clone(&queue);
        let work_dir = work_dir.to_path_buf();
        handles.push(thread::spawn(move || -> anyhow::Result<()> {
            loop {
                let unit = {
                    let mut guard = queue
                        .lock()
                        .map_err(|_| anyhow::anyhow!("plate queue poisoned"))?;
                    guard.pop_front()
                };
                let Some(unit) = unit else {
                    break;
                };
                render_planned_unit(&work_dir, &unit)?;
            }
            Ok(())
        }));
    }

    for handle in handles {
        handle
            .join()
            .map_err(|_| anyhow::anyhow!("tpp render worker panicked"))??;
    }

    Ok(())
}

fn render_planned_unit(work_dir: &Path, unit: &TppRenderUnitPlan) -> anyhow::Result<()> {
    match &unit.task {
        PlannedPlateTask::Single(plate) => make_resolved_plate(work_dir, plate),
        PlannedPlateTask::Continued(group) => {
            if resolved_continued_group_should_keep_separate(&group.members) {
                for member in &group.members {
                    make_resolved_plate(work_dir, member)?;
                }
            } else {
                make_resolved_continued_plate_group(work_dir, group)?;
            }
            Ok(())
        }
    }
}

fn build_plate_tasks(plates: Vec<PlateRecord>) -> Vec<PlateTask> {
    let mut grouped = BTreeMap::<String, Vec<(usize, Option<u32>, PlateRecord)>>::new();
    let mut group_order = Vec::new();
    for (original_index, plate) in plates.into_iter().enumerate() {
        let base_chart_name = grouped_plate_base_name(&plate);
        let owner_key = if plate.chart_code == "HOT" {
            plate.state_id.as_str()
        } else {
            plate.apt_id.as_str()
        };
        let key = format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}",
            owner_key, plate.state_id, plate.chart_code, base_chart_name
        );
        if !grouped.contains_key(&key) {
            group_order.push(key.clone());
        }
        grouped
            .entry(key)
            .or_default()
            .push((original_index, grouped_plate_index(&plate), plate));
    }

    let mut tasks = Vec::new();
    for key in group_order {
        let mut members = grouped.remove(&key).unwrap_or_default();
        let has_grouped_pages = members
            .iter()
            .any(|(_, continuation, _)| continuation.is_some());
        let is_hotspot = members
            .first()
            .map(|(_, _, plate)| plate.chart_code == "HOT")
            .unwrap_or(false);
        members.sort_by_key(|(original_index, continuation, _)| {
            (continuation.unwrap_or(0), *original_index)
        });
        if is_hotspot {
            tasks.push(PlateTask::Single(members[0].2.clone()));
        } else if has_grouped_pages && members.len() > 1 {
            let first = &members[0].2;
            let output_name = plate_output_name(
                &first.chart_code,
                &first.state_id,
                &grouped_plate_base_name(first),
            );
            tasks.push(PlateTask::Continued(ContinuedPlateGroup {
                apt_id: first.apt_id.clone(),
                output_name,
                members: members.into_iter().map(|(_, _, plate)| plate).collect(),
            }));
        } else {
            tasks.extend(
                members
                    .into_iter()
                    .map(|(_, _, plate)| PlateTask::Single(plate)),
            );
        }
    }
    tasks
}

fn plan_plate_task(
    ad_tags: &std::collections::HashMap<String, String>,
    minimum_pages: &BTreeMap<(String, String), Vec<u32>>,
    pdf_facts: &BTreeMap<String, PdfPlanningFacts>,
    task: PlateTask,
) -> anyhow::Result<TppRenderUnitPlan> {
    let planned_task = match task {
        PlateTask::Single(plate) => {
            PlannedPlateTask::Single(plan_plate(ad_tags, minimum_pages, pdf_facts, plate)?)
        }
        PlateTask::Continued(group) => {
            let mut members = group
                .members
                .into_iter()
                .map(|plate| plan_plate(ad_tags, minimum_pages, pdf_facts, plate))
                .collect::<anyhow::Result<Vec<_>>>()?;
            let mut legacy_continued_outputs = Vec::new();
            for member in &members {
                if member.output_name != group.output_name {
                    legacy_continued_outputs.push(member.output_name.clone());
                }
            }
            let planned_group = PlannedContinuedPlateGroup {
                apt_id: group.apt_id,
                output_name: group.output_name,
                members: {
                    members.shrink_to_fit();
                    members
                },
                legacy_continued_outputs,
            };
            PlannedPlateTask::Continued(planned_group)
        }
    };
    Ok(TppRenderUnitPlan {
        id: planned_unit_id(&planned_task),
        task: planned_task,
    })
}

fn collect_pdf_planning_facts(
    pdf_root: &Path,
    tasks: &[PlateTask],
) -> anyhow::Result<BTreeMap<String, PdfPlanningFacts>> {
    #[derive(Debug, Default)]
    struct NeededFacts {
        geotag_classification: bool,
        landscape_rotation: bool,
    }

    let mut needed_by_pdf = BTreeMap::<String, NeededFacts>::new();
    for plate in plate_records_for_tasks(tasks) {
        let output_name = plate_output_name(&plate.chart_code, &plate.state_id, &plate.chart_name);
        let needed = needed_by_pdf.entry(plate.pdf_name.clone()).or_default();
        if plate.chart_code != "HOT"
            && !output_name.starts_with("MIN-")
            && !output_name.starts_with("APD-")
        {
            needed.geotag_classification = true;
        }
        if plate.chart_code != "HOT" && should_detect_landscape_rotation(&output_name) {
            needed.landscape_rotation = true;
        }
    }

    needed_by_pdf
        .into_iter()
        .map(|(pdf_name, needed)| {
            let pdf_path = pdf_root.join(&pdf_name);
            if !pdf_path.is_file() {
                bail!("file not found {}", pdf_path.display());
            }
            let pdf_hash = hash_file(&pdf_path)?;
            let non_special_render_kind = if needed.geotag_classification {
                Some(classify_pdf_non_special_render_kind(&pdf_path)?)
            } else {
                None
            };
            let landscape_rotation = if needed.landscape_rotation {
                Some(detect_landscape_rotation(&pdf_path)?)
            } else {
                None
            };
            Ok((
                pdf_name,
                PdfPlanningFacts {
                    pdf_hash,
                    non_special_render_kind,
                    landscape_rotation,
                },
            ))
        })
        .collect()
}

fn resolved_continued_group_should_keep_separate(members: &[PlannedPlate]) -> bool {
    members.iter().enumerate().any(|(part_index, member)| {
        if part_index == 0 {
            !matches!(
                member.render_kind,
                PlateRenderKind::Basic | PlateRenderKind::Geotagged
            )
        } else {
            member.render_kind != PlateRenderKind::Basic
        }
    })
}

fn collect_minimum_pages_by_plate(
    pdf_root: &Path,
    tasks: &[PlateTask],
) -> anyhow::Result<BTreeMap<(String, String), Vec<u32>>> {
    let mut apt_ids_by_pdf = BTreeMap::<String, BTreeSet<String>>::new();
    for plate in plate_records_for_tasks(tasks) {
        let output_name = plate_output_name(&plate.chart_code, &plate.state_id, &plate.chart_name);
        if output_name.starts_with("MIN-") {
            apt_ids_by_pdf
                .entry(plate.pdf_name.clone())
                .or_default()
                .insert(plate.apt_id.clone());
        }
    }

    let mut pages_by_plate = BTreeMap::new();
    for (pdf_name, apt_ids) in apt_ids_by_pdf {
        let apt_ids = apt_ids.into_iter().collect::<Vec<_>>();
        let pages_by_apt = find_plate_pages_by_airport(&pdf_root.join(&pdf_name), &apt_ids)?;
        for apt_id in apt_ids {
            pages_by_plate.insert(
                (pdf_name.clone(), apt_id.clone()),
                pages_by_apt.get(&apt_id).cloned().unwrap_or_default(),
            );
        }
    }
    Ok(pages_by_plate)
}

fn plate_records_for_tasks(tasks: &[PlateTask]) -> Vec<&PlateRecord> {
    let mut records = Vec::new();
    for task in tasks {
        match task {
            PlateTask::Single(plate) => records.push(plate),
            PlateTask::Continued(group) => records.extend(group.members.iter()),
        }
    }
    records
}

fn plan_plate(
    ad_tags: &std::collections::HashMap<String, String>,
    minimum_pages: &BTreeMap<(String, String), Vec<u32>>,
    pdf_facts: &BTreeMap<String, PdfPlanningFacts>,
    plate: PlateRecord,
) -> anyhow::Result<PlannedPlate> {
    let output_name = plate_output_name(&plate.chart_code, &plate.state_id, &plate.chart_name);
    let facts = pdf_facts
        .get(&plate.pdf_name)
        .with_context(|| format!("missing planning facts for {}", plate.pdf_name))?;
    let render_kind = if plate.chart_code == "HOT" {
        PlateRenderKind::Basic
    } else if output_name.starts_with("MIN-") {
        PlateRenderKind::Minimum
    } else if output_name.starts_with("APD-") {
        PlateRenderKind::AirportDiagram
    } else {
        facts
            .non_special_render_kind
            .with_context(|| format!("missing render-kind facts for {}", plate.pdf_name))?
    };
    let rotation = if plate.chart_code == "HOT" {
        PlateRotation::None
    } else if should_detect_landscape_rotation(&output_name) {
        facts
            .landscape_rotation
            .with_context(|| format!("missing rotation facts for {}", plate.pdf_name))?
    } else {
        PlateRotation::None
    };
    let airport_diagram_comment = (render_kind == PlateRenderKind::AirportDiagram)
        .then(|| ad_tags.get(&plate.apt_id).cloned().unwrap_or_default());
    let minimum_pages = if output_name.starts_with("MIN-") {
        minimum_pages
            .get(&(plate.pdf_name.clone(), plate.apt_id.clone()))
            .cloned()
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    Ok(PlannedPlate {
        pdf_hash: facts.pdf_hash.clone(),
        render_kind,
        rotation,
        airport_diagram_comment,
        minimum_pages,
        record: plate,
        output_name,
    })
}

fn planned_unit_id(task: &PlannedPlateTask) -> String {
    let label = match task {
        PlannedPlateTask::Single(plate) => {
            format!("{}-{}", plate_owner(&plate.record), plate.output_name)
        }
        PlannedPlateTask::Continued(group) => {
            format!("{}-{}", group_owner(group), group.output_name)
        }
    };
    let json = serde_json::to_string(task).unwrap_or_else(|_| format!("{task:?}"));
    format!("{}-{}", sanitize_label(&label), short_stable_hash(&json))
}

fn short_stable_hash(value: &str) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn uppercase_pdf_names(work_dir: &Path) -> anyhow::Result<()> {
    for entry in
        fs::read_dir(work_dir).with_context(|| format!("failed to read {}", work_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
        {
            let upper_name = entry.file_name().to_string_lossy().to_uppercase();
            let upper_path = work_dir.join(&upper_name);
            if path != upper_path && !upper_path.exists() {
                fs::rename(&path, &upper_path).with_context(|| {
                    format!(
                        "failed to rename {} to {}",
                        path.display(),
                        upper_path.display()
                    )
                })?;
            }
        }
    }
    Ok(())
}

fn read_airport_diagram_tags(
    path: &Path,
) -> anyhow::Result<std::collections::HashMap<String, String>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut map = std::collections::HashMap::new();
    for line in text.lines() {
        let tokens = line.split(',').collect::<Vec<_>>();
        if tokens.len() < 12 {
            continue;
        }
        map.insert(tokens[0].to_string(), tokens[6..12].join("|"));
    }
    Ok(map)
}

fn parse_region_plates(xml_path: &Path, region: Region) -> anyhow::Result<Vec<PlateRecord>> {
    let text = fs::read_to_string(xml_path)
        .with_context(|| format!("failed to read {}", xml_path.display()))?;
    let document = roxmltree::Document::parse(&text)
        .with_context(|| format!("failed to parse {}", xml_path.display()))?;

    let mut plates = Vec::new();
    for state in document
        .descendants()
        .filter(|node| node.has_tag_name("state_code"))
    {
        let state_id = state.attribute("ID").unwrap_or("").trim().to_string();
        if state_id.is_empty() || !region.state_codes().contains(&state_id.as_str()) {
            continue;
        }
        for city in state
            .children()
            .filter(|node| node.has_tag_name("city_name"))
        {
            for airport in city
                .children()
                .filter(|node| node.has_tag_name("airport_name"))
            {
                let apt_id = airport
                    .attribute("apt_ident")
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if apt_id.is_empty() {
                    continue;
                }
                for record in airport
                    .children()
                    .filter(|node| node.has_tag_name("record"))
                {
                    let chart_name = record
                        .children()
                        .find(|node| node.has_tag_name("chart_name"))
                        .and_then(|node| node.text())
                        .unwrap_or("")
                        .trim()
                        .to_uppercase();
                    let chart_code = record
                        .children()
                        .find(|node| node.has_tag_name("chart_code"))
                        .and_then(|node| node.text())
                        .unwrap_or("")
                        .trim()
                        .to_uppercase();
                    let pdf_name = record
                        .children()
                        .find(|node| node.has_tag_name("pdf_name"))
                        .and_then(|node| node.text())
                        .unwrap_or("")
                        .trim()
                        .to_uppercase();
                    if chart_name.is_empty() || chart_code.is_empty() || pdf_name.is_empty() {
                        continue;
                    }
                    if pdf_name == TPP_DELETED_JOB_PDF_NAME {
                        continue;
                    }
                    plates.push(PlateRecord {
                        apt_id: apt_id.clone(),
                        state_id: state_id.clone(),
                        chart_name,
                        chart_code,
                        pdf_name,
                    });
                }
            }
        }
    }
    Ok(plates)
}

fn make_resolved_plate(work_dir: &Path, plate: &PlannedPlate) -> anyhow::Result<()> {
    let pdf_path = work_dir.join(&plate.record.pdf_name);
    if !pdf_path.is_file() {
        eprintln!("warning: file not found {}", pdf_path.display());
        return Ok(());
    }

    let output_name = &plate.output_name;
    let folder = plate_asset_folder(work_dir, &plate.record);
    fs::create_dir_all(&folder)
        .with_context(|| format!("failed to create {}", folder.display()))?;
    let pdf_hash = &plate.pdf_hash;

    if output_name.starts_with("MIN-") {
        let marker_path = plate_marker_path(&folder, output_name);
        let fingerprint = minimum_plate_fingerprint(pdf_hash, output_name, &plate.record.apt_id)?;
        invalidate_plate_prefix_if_stale(&folder, output_name, &marker_path, &fingerprint)?;
        render_minimum_plate(
            work_dir,
            &folder,
            &pdf_path,
            output_name,
            &plate.minimum_pages,
        )?;
        write_plate_marker(&marker_path, &fingerprint)?;
        return Ok(());
    }

    let png_path = folder.join(format!("{output_name}.png"));
    let marker_path = plate_marker_path(&folder, output_name);

    if plate.record.chart_code == "HOT" {
        let fingerprint = basic_plate_fingerprint(pdf_hash, output_name)?;
        if marker_matches(&marker_path, &fingerprint)? && png_path.is_file() {
            return Ok(());
        }
        invalidate_plate_prefix_if_stale(&folder, output_name, &marker_path, &fingerprint)?;

        let temp_prefix = format!("{output_name}-page");
        let temp_seed_path = folder.join(format!("{temp_prefix}.png"));
        remove_if_exists(&temp_seed_path)?;
        render_basic_png(work_dir, &pdf_path, &temp_seed_path, PlateRotation::None)?;

        let mut rendered_pages =
            existing_pngs_for_prefix(&folder, &temp_prefix)?.collect::<Vec<_>>();
        rendered_pages.sort();
        if rendered_pages.is_empty() {
            bail!("hotspot render produced no pngs for {}", pdf_path.display());
        }
        if rendered_pages.len() == 1 {
            let only_page = rendered_pages.remove(0);
            if only_page != png_path {
                fs::rename(&only_page, &png_path).with_context(|| {
                    format!(
                        "failed to rename {} to {}",
                        only_page.display(),
                        png_path.display()
                    )
                })?;
            }
        } else {
            for rendered_page in &rendered_pages {
                flatten_png_onto_white(rendered_page)?;
            }
            append_pngs_vertical(
                work_dir,
                &work_dir.join(".rust-logs"),
                &rendered_pages,
                &png_path,
                &format!("tpp-hotspot-{}", sanitize_label(output_name)),
            )?;
            for rendered_page in rendered_pages {
                remove_if_exists(&rendered_page)?;
            }
        }
        write_plate_marker(&marker_path, &fingerprint)?;
        return Ok(());
    }

    if output_name.starts_with("APD-") {
        let fingerprint = airport_diagram_fingerprint(
            pdf_hash,
            output_name,
            plate.airport_diagram_comment.as_deref().unwrap_or(""),
        )?;
        invalidate_single_plate_if_stale(&png_path, None, &marker_path, &fingerprint)?;
        if png_path.is_file() {
            return Ok(());
        }
        render_airport_diagram(
            work_dir,
            &pdf_path,
            &png_path,
            plate.airport_diagram_comment.as_ref(),
        )?;
        write_plate_marker(&marker_path, &fingerprint)?;
        return Ok(());
    }

    if plate.render_kind == PlateRenderKind::Geotagged {
        if plate.rotation != PlateRotation::None {
            // Some PHX departure plates are tagged as georeferenced by GDAL but also explicitly say
            // "Chart not to scale", so we treat their geotagging as untrustworthy once we rotate
            // them into the user-facing reading orientation. As of cycle 2604 this affects:
            // DP-AZ-BROAK ONE (RNAV), DP-AZ-ECLPS ONE (RNAV), and DP-AZ-FYRBD ONE (RNAV).
            // Perhaps these charts are actually to scale, in which case we might just transform
            // the geotagging instead of discarding it here.
            let tif_path = png_path.with_extension("tif");
            let fingerprint = basic_plate_fingerprint(pdf_hash, output_name)?;
            invalidate_single_plate_if_stale(
                &png_path,
                Some(&tif_path),
                &marker_path,
                &fingerprint,
            )?;
            if png_path.is_file() {
                return Ok(());
            }
            render_basic_png(work_dir, &pdf_path, &png_path, plate.rotation)?;
            write_plate_marker(&marker_path, &fingerprint)?;
            return Ok(());
        }
        let tif_path = png_path.with_extension("tif");
        let fingerprint = geotagged_plate_fingerprint(pdf_hash, output_name)?;
        invalidate_single_plate_if_stale(&png_path, Some(&tif_path), &marker_path, &fingerprint)?;
        if png_path.is_file() && tif_path.is_file() {
            return Ok(());
        }
        let _ = render_geotagged_plate(work_dir, &pdf_path, &png_path)?;
        write_plate_marker(&marker_path, &fingerprint)?;
    } else {
        let fingerprint = basic_plate_fingerprint(pdf_hash, output_name)?;
        invalidate_single_plate_if_stale(&png_path, None, &marker_path, &fingerprint)?;
        if png_path.is_file() {
            return Ok(());
        }
        render_basic_png(work_dir, &pdf_path, &png_path, plate.rotation)?;
        write_plate_marker(&marker_path, &fingerprint)?;
    }
    Ok(())
}

fn make_resolved_continued_plate_group(
    work_dir: &Path,
    group: &PlannedContinuedPlateGroup,
) -> anyhow::Result<()> {
    let folder = group_asset_folder(work_dir, group);
    fs::create_dir_all(&folder)
        .with_context(|| format!("failed to create {}", folder.display()))?;
    let final_png_path = folder.join(format!("{}.png", group.output_name));
    let marker_path = plate_marker_path(&folder, &group.output_name);

    let temp_dir = folder.join(format!(
        ".continued-parts-{}",
        sanitize_label(&group.output_name)
    ));
    fs::create_dir_all(&temp_dir)
        .with_context(|| format!("failed to create {}", temp_dir.display()))?;

    let mut part_paths = Vec::with_capacity(group.members.len());
    for (part_index, member) in group.members.iter().enumerate() {
        let pdf_path = work_dir.join(&member.record.pdf_name);
        if !pdf_path.is_file() {
            eprintln!("warning: file not found {}", pdf_path.display());
            return Ok(());
        }
        let temp_png = temp_dir.join(format!(
            "{}-part-{:02}.png",
            sanitize_label(&group.output_name),
            part_index
        ));
        part_paths.push((pdf_path, temp_png, member.render_kind, member.rotation));
    }

    let pdf_hashes = group
        .members
        .iter()
        .map(|member| member.pdf_hash.clone())
        .collect::<Vec<_>>();
    let fingerprint = continued_plate_fingerprint(
        &pdf_hashes,
        &group.output_name,
        &group.legacy_continued_outputs,
    )?;
    invalidate_continued_group_if_stale(
        &final_png_path,
        &marker_path,
        &group.legacy_continued_outputs,
        &folder,
        &fingerprint,
    )?;
    if final_png_path.is_file() {
        return Ok(());
    }

    let drop_group_geotag = part_paths.iter().any(|(_, _, render_kind, rotation)| {
        *render_kind == PlateRenderKind::Geotagged && *rotation != PlateRotation::None
    });
    let mut geotag_comment: Option<String> = None;
    let mut rendered_parts = Vec::with_capacity(part_paths.len());
    for (pdf_path, temp_png, render_kind, rotation) in &part_paths {
        remove_if_exists(temp_png)?;
        if *render_kind == PlateRenderKind::Geotagged && !drop_group_geotag {
            geotag_comment = Some(render_geotagged_plate(work_dir, pdf_path, temp_png)?);
        } else {
            render_basic_png(work_dir, pdf_path, temp_png, *rotation)?;
        }
        flatten_png_onto_white(temp_png)?;
        rendered_parts.push(temp_png.clone());
    }

    // Product UX intentionally diverges from the source-page layout here: CONT.
    // pages are separate FAA artifacts, but in the delivered product we want one
    // tall scrollable procedure image.
    append_pngs_vertical(
        work_dir,
        &work_dir.join(".rust-logs"),
        &rendered_parts,
        &final_png_path,
        &format!("tpp-continued-{}", sanitize_label(&group.output_name)),
    )?;
    if let Some(comment) = geotag_comment.as_deref() {
        // The 4-value plate geotag is anchored at the image top-left and uses
        // pixel-per-degree scale, so when only page 1 is georeferenced we can
        // safely keep that same transform on the taller concatenated image. The
        // appended continuation pages just extend downward.
        write_user_comment(work_dir, &final_png_path, comment)?;
    }
    for rendered_part in rendered_parts {
        remove_if_exists(&rendered_part)?;
    }
    remove_dir_if_exists(&temp_dir)?;
    remove_plate_outputs(&folder, &group.legacy_continued_outputs)?;
    write_plate_marker(&marker_path, &fingerprint)?;
    Ok(())
}

fn render_minimum_plate(
    work_dir: &Path,
    folder: &Path,
    pdf_path: &Path,
    output_name: &str,
    pages: &[u32],
) -> anyhow::Result<()> {
    if existing_pngs_for_prefix(folder, output_name)?
        .next()
        .is_some()
    {
        return Ok(());
    }

    if pages.is_empty() {
        render_basic_png(
            work_dir,
            pdf_path,
            &folder.join(format!("{output_name}.png")),
            PlateRotation::None,
        )?;
        return Ok(());
    }

    for page in pages {
        let png_path = folder.join(format!("{output_name}-{page}.png"));
        let invocation = ToolInvocation {
            program: "gs".to_string(),
            args: vec![
                "-dNOPAUSE".to_string(),
                "-dQUIET".to_string(),
                "-dNOPROMPT".to_string(),
                "-sDEVICE=pnggray".to_string(),
                format!("-r{TPP_RENDER_DPI}"),
                format!("-dFirstPage={}", *page + 1),
                format!("-dLastPage={}", *page + 1),
                "-o".to_string(),
                png_path.to_string_lossy().to_string(),
                pdf_path.to_string_lossy().to_string(),
            ],
            cwd: work_dir.to_path_buf(),
            label: format!("tpp-min-{}-{}", sanitize_label(output_name), page),
            env: Vec::new(),
            stdin_text: None,
        };
        let outcome = invocation.run_logged(work_dir.join(".rust-logs"))?;
        invocation.ensure_success(&outcome, &format!("gs failed for {}", pdf_path.display()))?;
    }

    Ok(())
}

fn render_airport_diagram(
    work_dir: &Path,
    pdf_path: &Path,
    png_path: &Path,
    comment: Option<&String>,
) -> anyhow::Result<()> {
    render_basic_png(work_dir, pdf_path, png_path, PlateRotation::None)?;
    write_user_comment(
        work_dir,
        png_path,
        comment.map(String::as_str).unwrap_or(""),
    )?;
    Ok(())
}

fn render_geotagged_plate(
    work_dir: &Path,
    pdf_path: &Path,
    png_path: &Path,
) -> anyhow::Result<String> {
    let tif_path = png_path.with_extension("tif");
    if !tif_path.is_file() {
        let invocation = ToolInvocation {
            program: "gdalwarp".to_string(),
            args: vec![
                "-q".to_string(),
                "-r".to_string(),
                "lanczos".to_string(),
                // Georeferenced plate PDFs often warp to shapes that do not fill the target
                // rectangle. Ask GDAL for an explicit alpha band so those edge pixels remain
                // transparent in the delivered PNG instead of turning into black slivers.
                "-dstalpha".to_string(),
                "-t_srs".to_string(),
                "epsg:3857".to_string(),
                pdf_path.to_string_lossy().to_string(),
                tif_path.to_string_lossy().to_string(),
            ],
            cwd: work_dir.to_path_buf(),
            label: format!("tpp-gdalwarp-{}", compact_path_label(png_path)),
            env: Vec::new(),
            stdin_text: None,
        };
        let outcome = invocation.run_logged(work_dir.join(".rust-logs"))?;
        invocation.ensure_success(
            &outcome,
            &format!("gdalwarp failed for {}", pdf_path.display()),
        )?;
    }

    render_png_preserve_alpha(work_dir, &tif_path, png_path)?;
    let info = read_gdalinfo(&tif_path)?;
    let comment = geotag_comment_from_gdalinfo(&info)?;
    write_user_comment(work_dir, png_path, &comment)?;
    Ok(comment)
}

fn render_basic_png(
    work_dir: &Path,
    input_path: &Path,
    png_path: &Path,
    rotation: PlateRotation,
) -> anyhow::Result<()> {
    let invocation = ToolInvocation {
        program: "mogrify".to_string(),
        args: vec![
            "-quiet".to_string(),
            "-dither".to_string(),
            "none".to_string(),
            "-antialias".to_string(),
            "-depth".to_string(),
            "8".to_string(),
            "-quality".to_string(),
            "100".to_string(),
            "-background".to_string(),
            "white".to_string(),
            "-alpha".to_string(),
            "remove".to_string(),
            "-colors".to_string(),
            "15".to_string(),
            "-density".to_string(),
            TPP_RENDER_DPI.to_string(),
            "-format".to_string(),
            "png".to_string(),
            "-write".to_string(),
            png_path.to_string_lossy().to_string(),
            input_path.to_string_lossy().to_string(),
        ],
        cwd: work_dir.to_path_buf(),
        label: format!("tpp-mogrify-{}", compact_path_label(png_path)),
        env: Vec::new(),
        stdin_text: None,
    };
    let outcome = invocation.run_logged(work_dir.join(".rust-logs"))?;
    invocation.ensure_success(
        &outcome,
        &format!("mogrify failed for {}", input_path.display()),
    )?;
    rotate_png_if_needed(work_dir, png_path, rotation)?;
    Ok(())
}

fn detect_landscape_rotation(pdf_path: &Path) -> anyhow::Result<PlateRotation> {
    let script_path = detect_landscape_rotation_script()?;
    let output = Command::new("python3")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .arg(&script_path)
        .arg(pdf_path)
        .output()
        .with_context(|| format!("failed to run {}", script_path.display()))?;
    if !output.status.success() {
        bail!(
            "detect_landscape_rotation.py failed for {}; command=\"python3 {} {}\" {}",
            pdf_path.display(),
            script_path.display(),
            pdf_path.display(),
            command_output_diagnostic_summary(&output)
        );
    }
    let stdout =
        String::from_utf8(output.stdout).context("landscape rotation output was not utf-8")?;
    let rotation = match stdout.trim() {
        "90" => PlateRotation::Clockwise90,
        "270" => PlateRotation::CounterClockwise90,
        _ => PlateRotation::None,
    };
    Ok(rotation)
}

fn should_detect_landscape_rotation(output_name: &str) -> bool {
    output_name.starts_with("STAR-")
        || output_name.starts_with("DP-")
        || output_name.starts_with("ODP-")
}

fn rotate_png_if_needed(
    work_dir: &Path,
    png_path: &Path,
    rotation: PlateRotation,
) -> anyhow::Result<()> {
    let angle = match rotation {
        PlateRotation::None => return Ok(()),
        PlateRotation::Clockwise90 => "90",
        PlateRotation::CounterClockwise90 => "270",
    };
    let invocation = ToolInvocation {
        program: "mogrify".to_string(),
        args: vec![
            "-quiet".to_string(),
            "-rotate".to_string(),
            angle.to_string(),
            png_path.to_string_lossy().to_string(),
        ],
        cwd: work_dir.to_path_buf(),
        label: format!("tpp-rotate-{}", compact_path_label(png_path)),
        env: Vec::new(),
        stdin_text: None,
    };
    let outcome = invocation.run_logged(work_dir.join(".rust-logs"))?;
    invocation.ensure_success(
        &outcome,
        &format!("mogrify rotate failed for {}", png_path.display()),
    )?;
    Ok(())
}

fn classify_pdf_non_special_render_kind(pdf_path: &Path) -> anyhow::Result<PlateRenderKind> {
    let gdalinfo = read_gdalinfo(pdf_path)?;
    if gdalinfo.contains("PROJCRS") {
        return Ok(PlateRenderKind::Geotagged);
    }
    Ok(PlateRenderKind::Basic)
}

fn render_png_preserve_alpha(
    work_dir: &Path,
    input_path: &Path,
    png_path: &Path,
) -> anyhow::Result<()> {
    let invocation = ToolInvocation {
        program: "mogrify".to_string(),
        args: vec![
            "-quiet".to_string(),
            "-dither".to_string(),
            "none".to_string(),
            "-antialias".to_string(),
            "-depth".to_string(),
            "8".to_string(),
            "-quality".to_string(),
            "100".to_string(),
            "-background".to_string(),
            "none".to_string(),
            "-alpha".to_string(),
            "on".to_string(),
            "-colors".to_string(),
            "15".to_string(),
            "-density".to_string(),
            TPP_RENDER_DPI.to_string(),
            "-format".to_string(),
            "png".to_string(),
            "-write".to_string(),
            png_path.to_string_lossy().to_string(),
            input_path.to_string_lossy().to_string(),
        ],
        cwd: work_dir.to_path_buf(),
        label: format!("tpp-mogrify-alpha-{}", compact_path_label(png_path)),
        env: Vec::new(),
        stdin_text: None,
    };
    let outcome = invocation.run_logged(work_dir.join(".rust-logs"))?;
    invocation.ensure_success(
        &outcome,
        &format!("mogrify failed for {}", input_path.display()),
    )?;
    Ok(())
}

fn write_user_comment(work_dir: &Path, png_path: &Path, comment: &str) -> anyhow::Result<()> {
    // Some georeference consumers read plate georeference from PNG EXIF
    // UserComment via ExifInterface. Aerobag should use typed metadata, but we
    // still emit the EXIF path for compatibility with geotag-aware tooling.
    let temp_path = PathBuf::from(format!("{}_exiftool_tmp", png_path.display()));
    if temp_path.exists() {
        fs::remove_file(&temp_path)
            .with_context(|| format!("failed to remove stale {}", temp_path.display()))?;
    }
    let invocation = ToolInvocation {
        program: "exiftool".to_string(),
        args: vec![
            "-q".to_string(),
            "-overwrite_original_in_place".to_string(),
            format!("-UserComment={comment}"),
            png_path.to_string_lossy().to_string(),
        ],
        cwd: work_dir.to_path_buf(),
        label: format!("tpp-exif-{}", compact_path_label(png_path)),
        env: Vec::new(),
        stdin_text: None,
    };
    let outcome = invocation.run_logged(work_dir.join(".rust-logs"))?;
    invocation.ensure_success(
        &outcome,
        &format!("exiftool failed for {}", png_path.display()),
    )?;
    Ok(())
}

fn find_plate_pages_by_airport(
    pdf_path: &Path,
    apt_ids: &[String],
) -> anyhow::Result<BTreeMap<String, Vec<u32>>> {
    if apt_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let script_path = find_plate_pages_script()?;
    let mut command = Command::new("python3");
    command
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .arg(&script_path)
        .arg(pdf_path);
    for apt_id in apt_ids {
        command.arg(apt_id);
    }
    let output = command
        .output()
        .with_context(|| format!("failed to run {}", script_path.display()))?;
    if !output.status.success() {
        bail!(
            "find_plate_pages.py failed for {} apt_ids={}; command=\"python3 {} {} <{} apt_ids>\" {}",
            pdf_path.display(),
            apt_ids.join(","),
            script_path.display(),
            pdf_path.display(),
            apt_ids.len(),
            command_output_diagnostic_summary(&output)
        );
    }
    serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "failed to decode find_plate_pages.py output for {}",
            pdf_path.display()
        )
    })
}

fn find_plate_pages_script() -> anyhow::Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut candidates = vec![manifest_dir.join("scripts").join("find_plate_pages.py")];

    if let Ok(current_exe) = std::env::current_exe() {
        for ancestor in current_exe.ancestors() {
            candidates.push(
                ancestor.join("product/preprocessor/preprocessor-tpp/scripts/find_plate_pages.py"),
            );
        }
    }

    for candidate in candidates {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    bail!("could not locate find_plate_pages.py in any known workspace layout")
}

fn detect_landscape_rotation_script() -> anyhow::Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut candidates = vec![manifest_dir
        .join("scripts")
        .join("detect_landscape_rotation.py")];
    if let Ok(current_exe) = std::env::current_exe() {
        for ancestor in current_exe.ancestors() {
            candidates.push(ancestor.join(
                "product/preprocessor/preprocessor-tpp/scripts/detect_landscape_rotation.py",
            ));
        }
    }
    for candidate in candidates {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    bail!("could not locate detect_landscape_rotation.py in any known workspace layout")
}

fn read_gdalinfo(path: &Path) -> anyhow::Result<String> {
    let output = Command::new("gdalinfo")
        .arg(path)
        .output()
        .with_context(|| format!("failed to run gdalinfo on {}", path.display()))?;
    if !output.status.success() {
        bail!(
            "gdalinfo failed for {}; command=\"gdalinfo {}\" {}",
            path.display(),
            path.display(),
            command_output_diagnostic_summary(&output)
        );
    }
    String::from_utf8(output.stdout).context("gdalinfo output was not utf-8")
}

fn geotag_comment_from_gdalinfo(info: &str) -> anyhow::Result<String> {
    let size_line = info
        .lines()
        .find(|line| line.starts_with("Size is "))
        .ok_or_else(|| anyhow::anyhow!("missing size line in gdalinfo output"))?;
    let upper_left_line = info
        .lines()
        .find(|line| line.starts_with("Upper Left"))
        .ok_or_else(|| anyhow::anyhow!("missing upper left line in gdalinfo output"))?;
    let lower_right_line = info
        .lines()
        .find(|line| line.starts_with("Lower Right"))
        .ok_or_else(|| anyhow::anyhow!("missing lower right line in gdalinfo output"))?;

    let (width, height) = parse_plate_size(size_line)?;
    let (x, y) = parse_plate_coordinate(upper_left_line)?;
    let (x0, y0) = parse_plate_coordinate(lower_right_line)?;
    Ok(format!(
        "{}|{}|{}|{}",
        width / (x0 - x),
        height / (y0 - y),
        x,
        y
    ))
}

fn parse_plate_size(line: &str) -> anyhow::Result<(f64, f64)> {
    let value = line
        .trim()
        .strip_prefix("Size is ")
        .unwrap_or(line)
        .replace(' ', "");
    let (width, height) = value
        .split_once(',')
        .ok_or_else(|| anyhow::anyhow!("invalid size line: {line}"))?;
    Ok((width.parse()?, height.parse()?))
}

fn parse_plate_coordinate(line: &str) -> anyhow::Result<(f64, f64)> {
    let start = line
        .rfind('(')
        .ok_or_else(|| anyhow::anyhow!("invalid coordinate line: {line}"))?;
    let end = line
        .rfind(')')
        .ok_or_else(|| anyhow::anyhow!("invalid coordinate line: {line}"))?;
    let body = &line[start + 1..end];
    let Some((lon_text, lat_text)) = body.split_once(',') else {
        bail!("invalid coordinate line: {line}");
    };
    Ok((
        parse_dms_coordinate(lon_text.trim())?,
        parse_dms_coordinate(lat_text.trim())?,
    ))
}

fn parse_dms_coordinate(value: &str) -> anyhow::Result<f64> {
    let bytes = value.as_bytes();
    let d_pos = value
        .find('d')
        .ok_or_else(|| anyhow::anyhow!("invalid dms coordinate: {value}"))?;
    let m_pos = value
        .find('\'')
        .ok_or_else(|| anyhow::anyhow!("invalid dms coordinate: {value}"))?;
    let q_pos = value
        .find('"')
        .ok_or_else(|| anyhow::anyhow!("invalid dms coordinate: {value}"))?;
    let hemi = *bytes
        .get(q_pos + 1)
        .ok_or_else(|| anyhow::anyhow!("invalid hemisphere: {value}"))? as char;

    let degrees: f64 = value[..d_pos].trim().parse()?;
    let minutes: f64 = value[d_pos + 1..m_pos].trim().parse()?;
    let seconds: f64 = value[m_pos + 1..q_pos].trim().parse()?;
    let mut decimal = degrees + minutes / 60.0 + seconds / 3600.0;
    if matches!(hemi, 'W' | 'S') {
        decimal *= -1.0;
    }
    Ok(decimal)
}

fn existing_pngs_for_prefix<'a>(
    folder: &'a Path,
    prefix: &'a str,
) -> anyhow::Result<impl Iterator<Item = PathBuf> + 'a> {
    let mut matches = Vec::new();
    for entry in
        fs::read_dir(folder).with_context(|| format!("failed to read {}", folder.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(prefix) && name.ends_with(".png") {
            matches.push(path);
        }
    }
    Ok(matches.into_iter())
}

fn plate_marker_path(folder: &Path, output_name: &str) -> PathBuf {
    folder.join(format!(".{}.fingerprint", sanitize_label(output_name)))
}

fn continuation_index(chart_name: &str) -> Option<u32> {
    chart_name
        .rsplit_once(", CONT.")
        .and_then(|(_, suffix)| suffix.trim().parse::<u32>().ok())
}

fn strip_continued_suffix(chart_name: &str) -> Option<String> {
    chart_name
        .rsplit_once(", CONT.")
        .map(|(base, _)| base.trim().to_string())
}

fn hotspot_page_index(chart_name: &str) -> Option<u32> {
    chart_name.rsplit_once('-').and_then(|(base, suffix)| {
        if base.trim_end().ends_with("HOT SPOT") {
            suffix.trim().parse::<u32>().ok()
        } else {
            None
        }
    })
}

fn strip_hotspot_page_suffix(chart_name: &str) -> Option<String> {
    chart_name.rsplit_once('-').and_then(|(base, suffix)| {
        if base.trim_end().ends_with("HOT SPOT") && suffix.trim().parse::<u32>().is_ok() {
            Some(base.trim().to_string())
        } else {
            None
        }
    })
}

fn grouped_plate_base_name(plate: &PlateRecord) -> String {
    strip_continued_suffix(&plate.chart_name)
        .or_else(|| {
            if plate.chart_code == "HOT" {
                strip_hotspot_page_suffix(&plate.chart_name)
            } else {
                None
            }
        })
        .unwrap_or_else(|| plate.chart_name.clone())
}

fn grouped_plate_index(plate: &PlateRecord) -> Option<u32> {
    continuation_index(&plate.chart_name).or_else(|| {
        if plate.chart_code == "HOT" {
            hotspot_page_index(&plate.chart_name)
        } else {
            None
        }
    })
}

fn plate_asset_folder(work_dir: &Path, plate: &PlateRecord) -> PathBuf {
    work_dir.join("plates").join(plate_owner(plate))
}

fn plate_owner(plate: &PlateRecord) -> &str {
    if plate.chart_code == "HOT" {
        plate.state_id.as_str()
    } else {
        plate.apt_id.as_str()
    }
}

fn group_asset_folder(work_dir: &Path, group: &PlannedContinuedPlateGroup) -> PathBuf {
    work_dir.join("plates").join(group_owner(group))
}

fn group_owner(group: &PlannedContinuedPlateGroup) -> &str {
    group
        .members
        .first()
        .map(|plate| {
            if plate.record.chart_code == "HOT" {
                plate.record.state_id.as_str()
            } else {
                group.apt_id.as_str()
            }
        })
        .unwrap_or(group.apt_id.as_str())
}

fn plate_output_name(chart_code: &str, state_id: &str, chart_name: &str) -> String {
    format!(
        "{}-{}-{}",
        chart_code,
        state_id,
        chart_name.replace('/', " AND ")
    )
}

fn basic_plate_fingerprint(pdf_hash: &str, output_name: &str) -> anyhow::Result<String> {
    let tools_hash = preprocessor_tools_source_hash()?;
    Ok(hash_fingerprint_components(&[
        TPP_BASIC_PIPELINE_VERSION,
        pdf_hash,
        output_name,
        &tools_hash,
    ]))
}

fn airport_diagram_fingerprint(
    pdf_hash: &str,
    output_name: &str,
    comment: &str,
) -> anyhow::Result<String> {
    let tools_hash = preprocessor_tools_source_hash()?;
    Ok(hash_fingerprint_components(&[
        TPP_AIRPORT_DIAGRAM_PIPELINE_VERSION,
        pdf_hash,
        output_name,
        comment,
        &tools_hash,
    ]))
}

fn geotagged_plate_fingerprint(pdf_hash: &str, output_name: &str) -> anyhow::Result<String> {
    let tools_hash = preprocessor_tools_source_hash()?;
    Ok(hash_fingerprint_components(&[
        TPP_GEOTAGGED_PIPELINE_VERSION,
        pdf_hash,
        output_name,
        &tools_hash,
    ]))
}

fn continued_plate_fingerprint(
    pdf_hashes: &[String],
    output_name: &str,
    legacy_continued_outputs: &[String],
) -> anyhow::Result<String> {
    let tools_hash = preprocessor_tools_source_hash()?;
    let mut parts = vec![
        TPP_CONTINUED_PIPELINE_VERSION.to_string(),
        output_name.to_string(),
    ];
    parts.extend(pdf_hashes.iter().cloned());
    parts.extend(legacy_continued_outputs.iter().cloned());
    parts.push(tools_hash);
    Ok(hash_fingerprint_components(
        &parts.iter().map(String::as_str).collect::<Vec<_>>(),
    ))
}

fn minimum_plate_fingerprint(
    pdf_hash: &str,
    output_name: &str,
    apt_id: &str,
) -> anyhow::Result<String> {
    let script_hash = hash_file(&find_plate_pages_script()?)?;
    Ok(hash_fingerprint_components(&[
        TPP_MINIMUM_PIPELINE_VERSION,
        pdf_hash,
        output_name,
        apt_id,
        &script_hash,
    ]))
}

fn preprocessor_tools_source_hash() -> anyhow::Result<String> {
    hash_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("preprocessor-tpp crate should live under workspace root")?
            .join("preprocessor-tools/src/lib.rs"),
    )
}

fn hash_fingerprint_components(parts: &[&str]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for part in parts {
        part.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn invalidate_single_plate_if_stale(
    png_path: &Path,
    tif_path: Option<&Path>,
    marker_path: &Path,
    expected: &str,
) -> anyhow::Result<()> {
    if marker_matches(marker_path, expected)? {
        return Ok(());
    }
    remove_if_exists(png_path)?;
    if let Some(tif_path) = tif_path {
        remove_if_exists(tif_path)?;
    }
    remove_if_exists(marker_path)?;
    Ok(())
}

fn invalidate_plate_prefix_if_stale(
    folder: &Path,
    output_name: &str,
    marker_path: &Path,
    expected: &str,
) -> anyhow::Result<()> {
    if marker_matches(marker_path, expected)? {
        return Ok(());
    }
    for path in existing_pngs_for_prefix(folder, output_name)? {
        remove_if_exists(&path)?;
    }
    remove_if_exists(marker_path)?;
    Ok(())
}

fn invalidate_continued_group_if_stale(
    png_path: &Path,
    marker_path: &Path,
    legacy_continued_outputs: &[String],
    folder: &Path,
    expected: &str,
) -> anyhow::Result<()> {
    if marker_matches(marker_path, expected)? {
        return Ok(());
    }
    remove_if_exists(png_path)?;
    remove_plate_outputs(folder, legacy_continued_outputs)?;
    remove_if_exists(marker_path)?;
    Ok(())
}

fn marker_matches(marker_path: &Path, expected: &str) -> anyhow::Result<bool> {
    if !marker_path.is_file() {
        return Ok(false);
    }
    let actual = fs::read_to_string(marker_path)
        .with_context(|| format!("failed to read {}", marker_path.display()))?;
    Ok(actual.trim() == expected)
}

fn write_plate_marker(marker_path: &Path, fingerprint: &str) -> anyhow::Result<()> {
    fs::write(marker_path, format!("{fingerprint}\n"))
        .with_context(|| format!("failed to write {}", marker_path.display()))
}

fn remove_plate_outputs(folder: &Path, output_names: &[String]) -> anyhow::Result<()> {
    for output_name in output_names {
        remove_if_exists(&folder.join(format!("{output_name}.png")))?;
        remove_if_exists(&folder.join(format!("{output_name}.tif")))?;
        remove_if_exists(&plate_marker_path(folder, output_name))?;
    }
    Ok(())
}

fn calculate_cycle(future: i64, now: DateTime<Utc>) -> (u32, u32) {
    let mut start_utc = Utc.with_ymd_and_hms(2020, 1, 2, 9, 0, 0).unwrap();
    let mut cycle = 1_u32;
    let mut last_year = 2019_i32;
    let mut combined = 2001_u32;
    let mut is56 = true;
    let target = now + Duration::days(28 * future);

    while start_utc < target {
        if last_year != start_utc.year() {
            cycle = 1;
            last_year = start_utc.year();
        } else {
            cycle += 1;
        }

        combined = ((start_utc.year() % 2000) as u32) * 100 + cycle;
        is56 = !is56;
        start_utc += Duration::days(28);
    }

    if is56 {
        (combined, combined)
    } else {
        let (_, prior_56) = calculate_cycle(future - 1, now);
        (combined, prior_56)
    }
}

fn compact_path_label(path: &Path) -> String {
    let base = path
        .file_name()
        .and_then(|value| value.to_str())
        .map(sanitize_label)
        .unwrap_or_else(|| "path".to_string());
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("{base}-{:016x}", hasher.finish())
}

fn remove_if_exists(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

fn remove_dir_if_exists(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)
            .with_context(|| format!("failed to remove directory {}", path.display()))?;
    }
    Ok(())
}

fn hard_link_or_copy_file(from: &Path, to: &Path) -> anyhow::Result<()> {
    if from == to {
        return Ok(());
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    remove_if_exists(to)?;
    match fs::hard_link(from, to) {
        Ok(()) => Ok(()),
        Err(link_error) => {
            fs::copy(from, to).with_context(|| {
                format!(
                    "failed to hardlink {} to {} ({link_error}); copy also failed",
                    from.display(),
                    to.display()
                )
            })?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_plate_tasks, geotag_comment_from_gdalinfo, parse_dms_coordinate, parse_region_plates,
        resolved_continued_group_should_keep_separate, PlannedPlate, PlateRecord, PlateRenderKind,
        PlateRotation, PlateTask,
    };
    use preprocessor_core::Region;
    use std::fs;

    #[test]
    fn parse_west_coordinate() {
        let value = parse_dms_coordinate("74d54'12.53\"W").unwrap();
        assert!((value - (-74.90348055555556)).abs() < 1e-12);
    }

    #[test]
    fn geotag_comment_matches_legacy_formula() {
        let info = "\
Size is 811, 1240
Upper Left  (-8338217.262, 5134922.586) ( 74d54'12.53\"W, 41d49'32.92\"N)
Lower Right (-8246604.366, 4994848.615) ( 74d 4'49.83\"W, 40d52'52.67\"N)
";
        assert_eq!(
            geotag_comment_from_gdalinfo(info).unwrap(),
            "985.4524589057144|-1312.8446437761886|-74.90348055555556|41.825811111111115"
        );
    }

    #[test]
    fn parse_region_plates_skips_deleted_job_tombstones() {
        let dir = tempfile::tempdir().unwrap();
        let xml_path = dir.path().join("d-TPP_Metafile.xml");
        fs::write(
            &xml_path,
            r#"
<digital_tpp>
  <state_code ID="WA">
    <city_name ID="SEATTLE">
      <airport_name apt_ident="SEA">
        <record>
          <chart_code>IAP</chart_code>
          <chart_name>RNAV (GPS) RWY 16C</chart_name>
          <pdf_name>SEA-RNAV16C.PDF</pdf_name>
        </record>
        <record>
          <chart_code>IAP</chart_code>
          <chart_name>DELETED PROCEDURE</chart_name>
          <pdf_name>DELETED_JOB.PDF</pdf_name>
        </record>
      </airport_name>
    </city_name>
  </state_code>
</digital_tpp>
"#,
        )
        .unwrap();

        let plates = parse_region_plates(&xml_path, Region::Nw).unwrap();

        assert_eq!(plates.len(), 1);
        assert_eq!(plates[0].pdf_name, "SEA-RNAV16C.PDF");
    }

    #[test]
    fn continued_records_are_grouped_into_one_task() {
        let tasks = build_plate_tasks(vec![
            PlateRecord {
                apt_id: "SEA".to_string(),
                state_id: "WA".to_string(),
                chart_name: "ILS OR LOC RWY 16C".to_string(),
                chart_code: "IAP".to_string(),
                pdf_name: "BASE.PDF".to_string(),
            },
            PlateRecord {
                apt_id: "SEA".to_string(),
                state_id: "WA".to_string(),
                chart_name: "ILS OR LOC RWY 16C, CONT.1".to_string(),
                chart_code: "IAP".to_string(),
                pdf_name: "CONT1.PDF".to_string(),
            },
        ]);
        assert_eq!(tasks.len(), 1);
        match &tasks[0] {
            PlateTask::Continued(group) => {
                assert_eq!(group.output_name, "IAP-WA-ILS OR LOC RWY 16C");
                assert_eq!(group.members.len(), 2);
            }
            other => panic!("expected continued task, got {other:?}"),
        }
    }

    #[test]
    fn standalone_records_remain_single_tasks() {
        let tasks = build_plate_tasks(vec![PlateRecord {
            apt_id: "RNT".to_string(),
            state_id: "WA".to_string(),
            chart_name: "RNAV (GPS) Z RWY 16".to_string(),
            chart_code: "IAP".to_string(),
            pdf_name: "BASE.PDF".to_string(),
        }]);
        assert_eq!(tasks.len(), 1);
        assert!(matches!(tasks[0], PlateTask::Single(_)));
    }

    #[test]
    fn hotspot_records_are_collapsed_into_one_render_task() {
        let tasks = build_plate_tasks(vec![
            PlateRecord {
                apt_id: "PAE".to_string(),
                state_id: "WA".to_string(),
                chart_name: "HOT SPOT-0".to_string(),
                chart_code: "HOT".to_string(),
                pdf_name: "HOT0.PDF".to_string(),
            },
            PlateRecord {
                apt_id: "PAE".to_string(),
                state_id: "WA".to_string(),
                chart_name: "HOT SPOT-1".to_string(),
                chart_code: "HOT".to_string(),
                pdf_name: "HOT1.PDF".to_string(),
            },
        ]);
        assert_eq!(tasks.len(), 1);
        match &tasks[0] {
            PlateTask::Single(plate) => {
                assert_eq!(plate.chart_code, "HOT");
                assert_eq!(plate.state_id, "WA");
                assert_eq!(plate.chart_name, "HOT SPOT-0");
            }
            other => panic!("expected collapsed hotspot task, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_hotspot_airport_rows_collapse_to_one_render_task() {
        let tasks = build_plate_tasks(vec![
            PlateRecord {
                apt_id: "PAE".to_string(),
                state_id: "WA".to_string(),
                chart_name: "HOT SPOT".to_string(),
                chart_code: "HOT".to_string(),
                pdf_name: "NW1HOTSPOT.PDF".to_string(),
            },
            PlateRecord {
                apt_id: "SEA".to_string(),
                state_id: "WA".to_string(),
                chart_name: "HOT SPOT".to_string(),
                chart_code: "HOT".to_string(),
                pdf_name: "NW1HOTSPOT.PDF".to_string(),
            },
        ]);
        assert_eq!(tasks.len(), 1);
        match &tasks[0] {
            PlateTask::Single(plate) => {
                assert_eq!(plate.chart_code, "HOT");
                assert_eq!(plate.state_id, "WA");
                assert_eq!(plate.chart_name, "HOT SPOT");
            }
            other => panic!("expected single hotspot task, got {other:?}"),
        }
    }

    fn planned_plate(render_kind: PlateRenderKind) -> PlannedPlate {
        PlannedPlate {
            record: PlateRecord {
                apt_id: "SEA".to_string(),
                state_id: "WA".to_string(),
                chart_name: "TEST".to_string(),
                chart_code: "IAP".to_string(),
                pdf_name: "TEST.PDF".to_string(),
            },
            output_name: "IAP-WA-TEST".to_string(),
            pdf_hash: "hash".to_string(),
            render_kind,
            rotation: PlateRotation::None,
            airport_diagram_comment: None,
            minimum_pages: Vec::new(),
        }
    }

    #[test]
    fn continued_planner_keeps_only_basic_continuations_merged() {
        assert!(!resolved_continued_group_should_keep_separate(&[
            planned_plate(PlateRenderKind::Geotagged),
            planned_plate(PlateRenderKind::Basic),
        ]));
        assert!(resolved_continued_group_should_keep_separate(&[
            planned_plate(PlateRenderKind::Basic),
            planned_plate(PlateRenderKind::Geotagged),
        ]));
        assert!(resolved_continued_group_should_keep_separate(&[
            planned_plate(PlateRenderKind::Minimum),
            planned_plate(PlateRenderKind::Basic),
        ]));
    }
}
