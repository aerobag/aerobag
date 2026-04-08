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
    copy_source_urls_provenance, hash_file, prefetch_archives_with_provenance,
    read_source_urls_jsonl, write_package_outputs_jsonl, PackageOutputRecord,
};
use preprocessor_tools::ToolInvocation;

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
struct AirportRecord {
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

fn stage_work_dir(source_repo: &Path, run_root: &Path) -> anyhow::Result<PathBuf> {
    let work_dir = run_root.join("work").join("csup");
    copy_dir_recursive(
        source_repo,
        &work_dir,
        looks_like_populated_work_dir(source_repo),
    )?;
    Ok(work_dir)
}

fn render_csup_pages(work_dir: &Path, render_jobs: usize) -> anyhow::Result<()> {
    fs::create_dir_all(work_dir.join("afd")).context("failed to create afd dir")?;
    uppercase_pdf_names(work_dir)?;
    let airports = parse_airports(&find_xml_path(work_dir)?)?;
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

fn package_csup_regions(work_dir: &Path, provenance_dir: &Path) -> anyhow::Result<usize> {
    let manifest_cycle = current_cycle_manifest();
    let mut package_records = Vec::with_capacity(Region::ALL.len());

    for region in Region::ALL {
        let manifest_name = format!("{}_CSUP", region.code());
        let zip_name = format!("{}_CSUP.zip", region.code());
        let manifest_path = work_dir.join(&manifest_name);
        let zip_path = work_dir.join(&zip_name);
        remove_if_exists(&manifest_path)?;
        remove_if_exists(&zip_path)?;

        let selected = collect_region_pngs(work_dir, region.code())?;

        let mut manifest_text = String::new();
        manifest_text.push_str(&manifest_cycle);
        manifest_text.push('\n');
        for path in &selected {
            manifest_text.push_str(path);
            manifest_text.push('\n');
        }
        fs::write(&manifest_path, manifest_text)
            .with_context(|| format!("failed to write {}", manifest_path.display()))?;

        let mut stdin_text = String::new();
        for path in &selected {
            stdin_text.push_str(path);
            stdin_text.push('\n');
        }
        stdin_text.push_str(&manifest_name);
        stdin_text.push('\n');

        let invocation = ToolInvocation {
            program: "zip".to_string(),
            args: vec!["-q".to_string(), zip_name.clone(), "-@".to_string()],
            cwd: work_dir.to_path_buf(),
            label: format!("csup-package-{}", region.code()),
            env: Vec::new(),
            stdin_text: Some(stdin_text),
        };
        let outcome = invocation.run_logged(work_dir.join(".rust-logs"))?;
        if !outcome.success {
            bail!("zip failed for region {}", region.code());
        }

        package_records.push(PackageOutputRecord {
            label: "csup".to_string(),
            chart: None,
            region: region.code().to_string(),
            manifest: manifest_name,
            manifest_sha256: hash_file(&manifest_path)?,
            zip: zip_name,
            zip_sha256: hash_file(&zip_path)?,
        });
    }

    write_package_outputs_jsonl(provenance_dir, &package_records)?;
    Ok(Region::ALL.len())
}

fn collect_region_pngs(work_dir: &Path, region_code: &str) -> anyhow::Result<Vec<String>> {
    fn visit(
        dir: &Path,
        root: &Path,
        region_code: &str,
        out: &mut Vec<String>,
    ) -> anyhow::Result<()> {
        for entry in
            fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .with_context(|| format!("failed to stat {}", path.display()))?;
            if file_type.is_dir() {
                visit(&path, root, region_code, out)?;
            } else if file_type.is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(&format!("CSUP-{region_code}_")) && name.ends_with(".png") {
                    let relative = path
                        .strip_prefix(root)
                        .with_context(|| format!("failed to relativize {}", path.display()))?;
                    out.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        Ok(())
    }

    let afd_dir = work_dir.join("afd");
    if !afd_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    visit(&afd_dir, work_dir, region_code, &mut paths)?;
    Ok(paths)
}

fn current_cycle_manifest() -> String {
    let (manifest_cycle, _) = calculate_cycle(1, Utc::now());
    manifest_cycle.to_string()
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

fn copy_dir_recursive(src: &Path, dst: &Path, preserve_generated: bool) -> anyhow::Result<()> {
    fs::create_dir_all(dst)
        .with_context(|| format!("failed to create destination {}", dst.display()))?;
    for entry in fs::read_dir(src).with_context(|| format!("failed to read {}", src.display()))? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        if should_skip_copy(&src_path, file_type.is_dir(), preserve_generated) {
            continue;
        }
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path, preserve_generated)?;
        } else if file_type.is_file() {
            fs::copy(&src_path, &dst_path).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn looks_like_populated_work_dir(path: &Path) -> bool {
    path.join("afd").is_dir()
        || path
            .read_dir()
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .any(|entry| {
                let entry_path = entry.path();
                entry_path
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|ext| matches!(ext, "zip" | "pdf" | "PDF" | "png" | "xml"))
            })
}

fn should_skip_copy(path: &Path, is_dir: bool, preserve_generated: bool) -> bool {
    let name = match path.file_name().and_then(|value| value.to_str()) {
        Some(name) => name,
        None => return false,
    };

    if is_dir {
        if matches!(name, "logs" | "meta" | "work" | "rust-runs") {
            return true;
        }
        return matches!(name, ".git" | "__pycache__" | ".rust-logs");
    }

    if preserve_generated {
        return false;
    }

    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("zip" | "pdf" | "PDF" | "png" | "xml")
    )
}

fn remove_if_exists(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}
