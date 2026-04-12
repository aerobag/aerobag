use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::Instant,
};

use anyhow::{bail, Context};
use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use preprocessor_core::{Region, RunPaths};
use preprocessor_fetch::{
    copy_source_urls_provenance, prefetch_archives_with_provenance, read_source_urls_jsonl,
};
use preprocessor_tools::{append_pngs_vertical, ToolInvocation};

mod package;

pub use package::{package_csup_region, package_csup_region_versioned, package_csup_regions};

#[derive(Debug, Clone)]
pub struct NativeCsupRunRequest {
    pub source_repo: PathBuf,
    pub run_root: PathBuf,
    pub prefetch_source_urls: Option<PathBuf>,
    pub fetch_jobs: usize,
    pub render_jobs: usize,
}

#[derive(Debug, Clone)]
pub struct NativeCsupRunResult {
    pub work_dir: PathBuf,
    pub prefetch_elapsed_ms: u128,
    pub render_elapsed_ms: u128,
    pub package_elapsed_ms: u128,
    pub package_count: usize,
}

#[derive(Debug, Clone)]
pub struct AirportRecord {
    apt_id: String,
    pdfs: Vec<String>,
}

pub fn run_native_csup(request: &NativeCsupRunRequest) -> anyhow::Result<NativeCsupRunResult> {
    let paths = RunPaths::new(&request.run_root);
    fs::create_dir_all(&paths.logs).context("failed to create logs dir")?;
    fs::create_dir_all(&paths.meta).context("failed to create meta dir")?;

    let work_dir = stage_work_dir(&request.source_repo, &request.run_root)?;
    let provenance_dir = paths.meta.join("provenance").join("csup");
    fs::create_dir_all(&provenance_dir).context("failed to create provenance dir")?;

    let mut prefetch_elapsed_ms = 0_u128;
    if let Some(source_urls_path) = &request.prefetch_source_urls {
        let start = Instant::now();
        copy_source_urls_provenance(source_urls_path, &provenance_dir)?;
        let urls = read_source_urls_jsonl(source_urls_path)?;
        prefetch_archives_with_provenance(
            &urls,
            &work_dir,
            request.fetch_jobs,
            &provenance_dir,
            "csup",
        )?;
        prefetch_elapsed_ms = start.elapsed().as_millis();
    }

    let render_start = Instant::now();
    render_csup_pages(&work_dir, request.render_jobs)?;
    let render_elapsed_ms = render_start.elapsed().as_millis();

    let package_start = Instant::now();
    let package_count = package_csup_regions(&work_dir, &provenance_dir)?;
    let package_elapsed_ms = package_start.elapsed().as_millis();

    Ok(NativeCsupRunResult {
        work_dir,
        prefetch_elapsed_ms,
        render_elapsed_ms,
        package_elapsed_ms,
        package_count,
    })
}

fn stage_work_dir(_source_repo: &Path, run_root: &Path) -> anyhow::Result<PathBuf> {
    let work_dir = run_root.join("work").join("csup");
    fs::create_dir_all(&work_dir)
        .with_context(|| format!("failed to create {}", work_dir.display()))?;
    Ok(work_dir)
}

pub fn stage_work_dir_for_product(source_repo: &Path, run_root: &Path) -> anyhow::Result<PathBuf> {
    stage_work_dir(source_repo, run_root)
}

pub fn render_csup_pages(work_dir: &Path, render_jobs: usize) -> anyhow::Result<()> {
    fs::create_dir_all(work_dir.join("afd")).context("failed to create afd dir")?;
    uppercase_pdf_names(work_dir)?;
    let airports = load_airports(work_dir)?;
    render_airport_groups_parallel(work_dir, airports, render_jobs)
}

pub fn prepare_csup_inputs(work_dir: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(work_dir.join("afd")).context("failed to create afd dir")?;
    uppercase_pdf_names(work_dir)
}

pub fn load_airports(work_dir: &Path) -> anyhow::Result<Vec<AirportRecord>> {
    parse_airports(&find_xml_path(work_dir)?)
}

pub fn render_csup_region(
    work_dir: &Path,
    region: Region,
    render_jobs: usize,
) -> anyhow::Result<()> {
    fs::create_dir_all(work_dir.join("afd")).context("failed to create afd dir")?;
    let airports = load_airports(work_dir)?
        .into_iter()
        .filter(|airport| airport_matches_region(airport, region))
        .collect::<Vec<_>>();
    render_airport_groups_parallel(work_dir, airports, render_jobs)
}

fn render_airport_groups_parallel(
    work_dir: &Path,
    airports: Vec<AirportRecord>,
    render_jobs: usize,
) -> anyhow::Result<()> {
    let mut grouped = BTreeMap::<String, Vec<AirportRecord>>::new();
    let mut apt_order = Vec::new();
    for airport in airports {
        let apt_id = airport.apt_id.clone();
        let entry = grouped.entry(apt_id.clone()).or_insert_with(|| {
            apt_order.push(apt_id.clone());
            Vec::new()
        });
        entry.push(airport);
    }

    let groups = apt_order
        .into_iter()
        .filter_map(|apt_id| grouped.remove(&apt_id))
        .collect::<Vec<_>>();
    let queue = Arc::new(Mutex::new(VecDeque::from(groups)));
    let job_count = render_jobs.max(1);
    let mut handles = Vec::with_capacity(job_count);

    for _ in 0..job_count {
        let queue = Arc::clone(&queue);
        let work_dir = work_dir.to_path_buf();
        handles.push(thread::spawn(move || -> anyhow::Result<()> {
            loop {
                let group = {
                    let mut guard = queue
                        .lock()
                        .map_err(|_| anyhow::anyhow!("csup airport queue poisoned"))?;
                    guard.pop_front()
                };
                let Some(group) = group else {
                    break;
                };
                for airport in group {
                    render_airport_pages(&work_dir, &airport)?;
                }
            }
            Ok(())
        }));
    }

    for handle in handles {
        handle
            .join()
            .map_err(|_| anyhow::anyhow!("csup render worker panicked"))??;
    }

    Ok(())
}

fn airport_matches_region(airport: &AirportRecord, region: Region) -> bool {
    airport.pdfs.iter().any(|pdf_name| {
        pdf_name
            .split('_')
            .next()
            .is_some_and(|token| token.eq_ignore_ascii_case(region.code()))
    })
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

fn find_xml_path(work_dir: &Path) -> anyhow::Result<PathBuf> {
    let mut xmls = fs::read_dir(work_dir)
        .with_context(|| format!("failed to read {}", work_dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.starts_with("afd_") && name.ends_with(".xml"))
        })
        .collect::<Vec<_>>();
    xmls.sort();
    xmls.into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no afd_*.xml found in {}", work_dir.display()))
}

fn parse_airports(xml_path: &Path) -> anyhow::Result<Vec<AirportRecord>> {
    let text = fs::read_to_string(xml_path)
        .with_context(|| format!("failed to read {}", xml_path.display()))?;
    let document = roxmltree::Document::parse(&text)
        .with_context(|| format!("failed to parse {}", xml_path.display()))?;
    let mut airports = Vec::new();
    for airport in document
        .descendants()
        .filter(|node| node.has_tag_name("airport"))
    {
        let apt_id = airport
            .children()
            .find(|node| node.has_tag_name("aptid"))
            .and_then(|node| node.text())
            .unwrap_or("")
            .trim()
            .to_string();
        if apt_id.is_empty() {
            continue;
        }
        let pdfs = airport
            .descendants()
            .filter(|node| node.has_tag_name("pdf"))
            .filter_map(|node| node.text())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_uppercase())
            .collect::<Vec<_>>();
        if pdfs.is_empty() {
            continue;
        }
        airports.push(AirportRecord { apt_id, pdfs });
    }
    Ok(airports)
}

fn render_airport_pages(work_dir: &Path, airport: &AirportRecord) -> anyhow::Result<()> {
    let apt_dir = work_dir.join("afd").join(&airport.apt_id);
    fs::create_dir_all(&apt_dir)
        .with_context(|| format!("failed to create {}", apt_dir.display()))?;

    for (page_index, pdf_name) in airport.pdfs.iter().enumerate() {
        let tokens = pdf_name.split('_').collect::<Vec<_>>();
        let Some(region_token) = tokens.first() else {
            continue;
        };
        let base_name = format!("CSUP-{}", region_token.to_uppercase());
        let output_base = format!("{base_name}_{page_index}");
        // The legacy Python pipeline names CSUP pages by the index of the PDF within a
        // single <airport> record, not by any FAA-global page identifier. The FAA XML can
        // contain duplicate <airport> entries with the same aptid but different PDF refs.
        // Legacy processes those duplicate airports in document order and simply writes the
        // same afd/<APT>/CSUP-<REGION>_<index>.png path again, so the later duplicate
        // silently overwrites the earlier one. We preserve that behavior here because the
        // packaged artifact contract is defined by legacy output paths, even though the FAA
        // feed shape is surprising.
        remove_pngs_for_base(&apt_dir, &output_base)?;

        let invocation = ToolInvocation {
            program: "mogrify".to_string(),
            args: vec![
                "-trim".to_string(),
                "+repage".to_string(),
                "-dither".to_string(),
                "none".to_string(),
                "-antialias".to_string(),
                "-density".to_string(),
                "225".to_string(),
                "-depth".to_string(),
                "8".to_string(),
                "-background".to_string(),
                "white".to_string(),
                "-alpha".to_string(),
                "remove".to_string(),
                "-alpha".to_string(),
                "off".to_string(),
                "-colors".to_string(),
                "15".to_string(),
                "-format".to_string(),
                "png".to_string(),
                "-quality".to_string(),
                "100".to_string(),
                "-write".to_string(),
                format!("afd/{}/{output_base}.png", airport.apt_id),
                pdf_name.clone(),
            ],
            cwd: work_dir.to_path_buf(),
            label: format!("csup-{}-{}", airport.apt_id, page_index),
            env: Vec::new(),
            stdin_text: None,
        };
        let outcome = invocation.run_logged(work_dir.join(".rust-logs"))?;
        if !outcome.success {
            bail!(
                "mogrify failed for airport {} page {}",
                airport.apt_id,
                page_index
            );
        }
        collapse_rendered_pdf_pages(work_dir, &apt_dir, &output_base)?;
    }

    Ok(())
}

fn collapse_rendered_pdf_pages(work_dir: &Path, apt_dir: &Path, output_base: &str) -> anyhow::Result<()> {
    let final_png = apt_dir.join(format!("{output_base}.png"));
    let mut rendered_pages = collect_rendered_pdf_pages(apt_dir, output_base)?;
    if rendered_pages.is_empty() {
        return Ok(());
    }
    if rendered_pages.len() == 1 {
        let only_page = rendered_pages.pop().unwrap();
        if only_page != final_png {
            remove_if_exists(&final_png)?;
            fs::rename(&only_page, &final_png).with_context(|| {
                format!(
                    "failed to rename {} to {}",
                    only_page.display(),
                    final_png.display()
                )
            })?;
        }
        return Ok(());
    }

    // Product UX intentionally collapses multi-page CSUP PDFs into one tall PNG so the airport
    // browser exposes a single scrollable supplement page instead of separate page-0/page-1
    // entries.
    append_pngs_vertical(
        work_dir,
        &work_dir.join(".rust-logs"),
        &rendered_pages,
        &final_png,
        &format!("csup-append-{}", sanitize_label(output_base)),
    )?;
    for rendered_page in rendered_pages {
        remove_if_exists(&rendered_page)?;
    }
    Ok(())
}

fn remove_pngs_for_base(apt_dir: &Path, output_base: &str) -> anyhow::Result<()> {
    for entry in
        fs::read_dir(apt_dir).with_context(|| format!("failed to read {}", apt_dir.display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(output_base) && name.ends_with(".png") {
            fs::remove_file(entry.path())
                .with_context(|| format!("failed to remove {}", entry.path().display()))?;
        }
    }
    Ok(())
}

fn collect_rendered_pdf_pages(apt_dir: &Path, output_base: &str) -> anyhow::Result<Vec<PathBuf>> {
    let mut pages = Vec::new();
    for entry in
        fs::read_dir(apt_dir).with_context(|| format!("failed to read {}", apt_dir.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name == format!("{output_base}.png")
            || (name.starts_with(&format!("{output_base}-")) && name.ends_with(".png"))
        {
            pages.push(path);
        }
    }
    pages.sort_by_key(|path| csup_page_sort_key(path, output_base));
    Ok(pages)
}

fn csup_page_sort_key(path: &Path, output_base: &str) -> (u32, String) {
    let filename = path.file_name().and_then(|value| value.to_str()).unwrap_or_default();
    if filename == format!("{output_base}.png") {
        return (0, filename.to_string());
    }
    let suffix = filename
        .strip_prefix(&format!("{output_base}-"))
        .and_then(|value| value.strip_suffix(".png"))
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(u32::MAX);
    (suffix + 1, filename.to_string())
}

pub(crate) fn calculate_cycle(future: i64, now: DateTime<Utc>) -> (u32, u32) {
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

fn sanitize_label(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

pub(crate) fn remove_if_exists(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}
