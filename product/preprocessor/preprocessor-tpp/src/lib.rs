use std::{
    collections::VecDeque,
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
    read_source_urls_jsonl, write_package_outputs_jsonl, PackageOutputRecord,
};
use preprocessor_tools::ToolInvocation;

const TPP_AIRPORT_DIAGRAMS_URL: &str =
    "https://www.outerworldapps.com/WairToNowWork/avare_aptdiags.php";
const TPP_BASIC_PIPELINE_VERSION: &str = "basic-v1";
const TPP_AIRPORT_DIAGRAM_PIPELINE_VERSION: &str = "airport-diagram-v1";
const TPP_GEOTAGGED_PIPELINE_VERSION: &str = "geotagged-v2-dstalpha";
const TPP_MINIMUM_PIPELINE_VERSION: &str = "minimum-v1";

#[derive(Debug, Clone)]
pub struct NativeTppRunRequest {
    pub region: Region,
    pub source_repo: PathBuf,
    pub run_root: PathBuf,
    pub prefetch_source_urls: Option<PathBuf>,
    pub fetch_jobs: usize,
    pub render_jobs: usize,
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
struct PlateRecord {
    apt_id: String,
    state_id: String,
    chart_name: String,
    chart_code: String,
    pdf_name: String,
}

pub fn run_native_tpp(request: &NativeTppRunRequest) -> anyhow::Result<NativeTppRunResult> {
    let paths = RunPaths::new(&request.run_root);
    fs::create_dir_all(&paths.logs).context("failed to create logs dir")?;
    fs::create_dir_all(&paths.meta).context("failed to create meta dir")?;

    let work_dir = stage_work_dir(&request.source_repo, &request.run_root, request.region)?;
    let provenance_dir = paths.meta.join("provenance").join(format!(
        "tpp-{}",
        request.region.code().to_ascii_lowercase()
    ));
    fs::create_dir_all(&provenance_dir).context("failed to create provenance dir")?;

    let mut prefetch_elapsed_ms = 0_u128;
    if let Some(source_urls_path) = &request.prefetch_source_urls {
        let start = Instant::now();
        copy_source_urls_provenance(source_urls_path, &provenance_dir)?;
        let mut urls = read_source_urls_jsonl(source_urls_path)?;
        if !urls.iter().any(|url| url == TPP_AIRPORT_DIAGRAMS_URL) {
            urls.push(TPP_AIRPORT_DIAGRAMS_URL.to_string());
        }
        prefetch_archives_with_provenance(
            &urls,
            &work_dir,
            request.fetch_jobs,
            &provenance_dir,
            &format!("tpp-{}", request.region.code().to_ascii_lowercase()),
        )?;
        prefetch_elapsed_ms = start.elapsed().as_millis();
    }

    let render_start = Instant::now();
    render_tpp_region(&work_dir, request.region, request.render_jobs)?;
    let render_elapsed_ms = render_start.elapsed().as_millis();

    let package_start = Instant::now();
    let package_count = package_region(&work_dir, &provenance_dir, request.region)?;
    let package_elapsed_ms = package_start.elapsed().as_millis();

    Ok(NativeTppRunResult {
        work_dir,
        prefetch_elapsed_ms,
        render_elapsed_ms,
        package_elapsed_ms,
        package_count,
    })
}

fn stage_work_dir(source_repo: &Path, run_root: &Path, region: Region) -> anyhow::Result<PathBuf> {
    let work_dir = run_root
        .join("work")
        .join(format!("tpp-{}", region.code().to_ascii_lowercase()));
    copy_dir_recursive(
        source_repo,
        &work_dir,
        looks_like_populated_work_dir(source_repo),
    )?;
    Ok(work_dir)
}

fn render_tpp_region(work_dir: &Path, region: Region, render_jobs: usize) -> anyhow::Result<()> {
    uppercase_pdf_names(work_dir)?;
    fs::create_dir_all(work_dir.join("plates")).context("failed to create plates dir")?;
    let ad_tags = read_airport_diagram_tags(&work_dir.join("avare_aptdiags.php"))?;
    let xml_path = work_dir.join("d-TPP_Metafile.xml");
    let plates = parse_region_plates(&xml_path, region)?;
    render_plates_parallel(work_dir, &ad_tags, plates, render_jobs)
}

fn render_plates_parallel(
    work_dir: &Path,
    ad_tags: &std::collections::HashMap<String, String>,
    plates: Vec<PlateRecord>,
    render_jobs: usize,
) -> anyhow::Result<()> {
    let queue = Arc::new(Mutex::new(VecDeque::from(plates)));
    let job_count = render_jobs.max(1);
    let mut handles = Vec::with_capacity(job_count);

    for _ in 0..job_count {
        let queue = Arc::clone(&queue);
        let work_dir = work_dir.to_path_buf();
        let ad_tags = ad_tags.clone();
        handles.push(thread::spawn(move || -> anyhow::Result<()> {
            loop {
                let plate = {
                    let mut guard = queue.lock().map_err(|_| anyhow::anyhow!("plate queue poisoned"))?;
                    guard.pop_front()
                };
                let Some(plate) = plate else {
                    break;
                };
                make_plate(&work_dir, &ad_tags, &plate)?;
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

fn make_plate(
    work_dir: &Path,
    ad_tags: &std::collections::HashMap<String, String>,
    plate: &PlateRecord,
) -> anyhow::Result<()> {
    let pdf_path = work_dir.join(&plate.pdf_name);
    if !pdf_path.is_file() {
        eprintln!("warning: file not found {}", pdf_path.display());
        return Ok(());
    }

    let output_name = format!(
        "{}-{}-{}",
        plate.chart_code,
        plate.state_id,
        plate.chart_name.replace('/', " AND ")
    );
    let folder = work_dir.join("plates").join(&plate.apt_id);
    fs::create_dir_all(&folder)
        .with_context(|| format!("failed to create {}", folder.display()))?;
    let pdf_hash = hash_file(&pdf_path)?;

    if output_name.starts_with("MIN-") {
        let marker_path = plate_marker_path(&folder, &output_name);
        let fingerprint = minimum_plate_fingerprint(&pdf_hash, &output_name, &plate.apt_id)?;
        invalidate_plate_prefix_if_stale(&folder, &output_name, &marker_path, &fingerprint)?;
        render_minimum_plate(work_dir, &folder, &pdf_path, &output_name, &plate.apt_id)?;
        write_plate_marker(&marker_path, &fingerprint)?;
        return Ok(());
    }

    let png_path = folder.join(format!("{output_name}.png"));
    let marker_path = plate_marker_path(&folder, &output_name);

    if output_name.starts_with("APD-") {
        let fingerprint = airport_diagram_fingerprint(
            &pdf_hash,
            &output_name,
            ad_tags.get(&plate.apt_id).map(String::as_str).unwrap_or(""),
        )?;
        invalidate_single_plate_if_stale(&png_path, None, &marker_path, &fingerprint)?;
        if png_path.is_file() {
            return Ok(());
        }
        render_airport_diagram(work_dir, &pdf_path, &png_path, ad_tags.get(&plate.apt_id))?;
        write_plate_marker(&marker_path, &fingerprint)?;
        return Ok(());
    }

    let gdalinfo = read_gdalinfo(&pdf_path)?;
    let has_proj = gdalinfo.contains("PROJCRS");
    if has_proj {
        let tif_path = png_path.with_extension("tif");
        let fingerprint = geotagged_plate_fingerprint(&pdf_hash, &output_name)?;
        invalidate_single_plate_if_stale(&png_path, Some(&tif_path), &marker_path, &fingerprint)?;
        if png_path.is_file() && tif_path.is_file() {
            return Ok(());
        }
        render_geotagged_plate(work_dir, &pdf_path, &png_path)?;
        write_plate_marker(&marker_path, &fingerprint)?;
    } else {
        let fingerprint = basic_plate_fingerprint(&pdf_hash, &output_name)?;
        invalidate_single_plate_if_stale(&png_path, None, &marker_path, &fingerprint)?;
        if png_path.is_file() {
            return Ok(());
        }
        render_basic_png(work_dir, &pdf_path, &png_path)?;
        write_plate_marker(&marker_path, &fingerprint)?;
    }
    Ok(())
}

fn render_minimum_plate(
    work_dir: &Path,
    folder: &Path,
    pdf_path: &Path,
    output_name: &str,
    apt_id: &str,
) -> anyhow::Result<()> {
    if existing_pngs_for_prefix(folder, output_name)?
        .next()
        .is_some()
    {
        return Ok(());
    }

    let pages = find_plate_pages(pdf_path, apt_id)?;
    if pages.is_empty() {
        render_basic_png(
            work_dir,
            pdf_path,
            &folder.join(format!("{output_name}.png")),
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
                "-r150".to_string(),
                format!("-dFirstPage={}", page + 1),
                format!("-dLastPage={}", page + 1),
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
        if !outcome.success {
            bail!("gs failed for {}", pdf_path.display());
        }
    }

    Ok(())
}

fn render_airport_diagram(
    work_dir: &Path,
    pdf_path: &Path,
    png_path: &Path,
    comment: Option<&String>,
) -> anyhow::Result<()> {
    render_basic_png(work_dir, pdf_path, png_path)?;
    write_user_comment(
        work_dir,
        png_path,
        comment.map(String::as_str).unwrap_or(""),
    )?;
    Ok(())
}

fn render_geotagged_plate(work_dir: &Path, pdf_path: &Path, png_path: &Path) -> anyhow::Result<()> {
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
            label: format!(
                "tpp-gdalwarp-{}",
                sanitize_label(&png_path.display().to_string())
            ),
            env: Vec::new(),
            stdin_text: None,
        };
        let outcome = invocation.run_logged(work_dir.join(".rust-logs"))?;
        if !outcome.success {
            bail!("gdalwarp failed for {}", pdf_path.display());
        }
    }

    render_png_preserve_alpha(work_dir, &tif_path, png_path)?;
    let info = read_gdalinfo(&tif_path)?;
    let comment = geotag_comment_from_gdalinfo(&info)?;
    write_user_comment(work_dir, png_path, &comment)?;
    Ok(())
}

fn render_basic_png(work_dir: &Path, input_path: &Path, png_path: &Path) -> anyhow::Result<()> {
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
            "150".to_string(),
            "-format".to_string(),
            "png".to_string(),
            "-write".to_string(),
            png_path.to_string_lossy().to_string(),
            input_path.to_string_lossy().to_string(),
        ],
        cwd: work_dir.to_path_buf(),
        label: format!(
            "tpp-mogrify-{}",
            sanitize_label(&png_path.display().to_string())
        ),
        env: Vec::new(),
        stdin_text: None,
    };
    let outcome = invocation.run_logged(work_dir.join(".rust-logs"))?;
    if !outcome.success {
        bail!("mogrify failed for {}", input_path.display());
    }
    Ok(())
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
            "150".to_string(),
            "-format".to_string(),
            "png".to_string(),
            "-write".to_string(),
            png_path.to_string_lossy().to_string(),
            input_path.to_string_lossy().to_string(),
        ],
        cwd: work_dir.to_path_buf(),
        label: format!(
            "tpp-mogrify-alpha-{}",
            sanitize_label(&png_path.display().to_string())
        ),
        env: Vec::new(),
        stdin_text: None,
    };
    let outcome = invocation.run_logged(work_dir.join(".rust-logs"))?;
    if !outcome.success {
        bail!("mogrify failed for {}", input_path.display());
    }
    Ok(())
}

fn write_user_comment(work_dir: &Path, png_path: &Path, comment: &str) -> anyhow::Result<()> {
    let invocation = ToolInvocation {
        program: "exiftool".to_string(),
        args: vec![
            "-q".to_string(),
            "-overwrite_original_in_place".to_string(),
            format!("-UserComment={comment}"),
            png_path.to_string_lossy().to_string(),
        ],
        cwd: work_dir.to_path_buf(),
        label: format!(
            "tpp-exif-{}",
            sanitize_label(&png_path.display().to_string())
        ),
        env: Vec::new(),
        stdin_text: None,
    };
    let outcome = invocation.run_logged(work_dir.join(".rust-logs"))?;
    if !outcome.success {
        bail!("exiftool failed for {}", png_path.display());
    }
    Ok(())
}

fn find_plate_pages(pdf_path: &Path, apt_id: &str) -> anyhow::Result<Vec<u32>> {
    let script_path = find_plate_pages_script()?;
    let output = Command::new("python3")
        .arg(&script_path)
        .arg(pdf_path)
        .arg(apt_id)
        .output()
        .with_context(|| format!("failed to run {}", script_path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("find_plate_pages.py failed: {stderr}");
    }
    let stdout = String::from_utf8(output.stdout).context("plate page output was not utf-8")?;
    let mut pages = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        pages.push(
            trimmed
                .parse()
                .with_context(|| format!("invalid page number: {trimmed}"))?,
        );
    }
    Ok(pages)
}

fn find_plate_pages_script() -> anyhow::Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut candidates = vec![manifest_dir.join("scripts").join("find_plate_pages.py")];

    // Keep the TPP helper resilient to repo refactors. This compatibility
    // layer must sometimes run from binaries compiled before or during a tree
    // reorganization, and we do not want a stale baked-in crate path to break
    // parity runs when the helper script is still present elsewhere in the
    // repo. Prefer the current crate layout, then fall back across known
    // workspace homes for the same compatibility code.
    if let Ok(current_exe) = std::env::current_exe() {
        for ancestor in current_exe.ancestors() {
            candidates.push(
                ancestor
                    .join("baseline/avare_equivalent/preprocessor-tpp/scripts/find_plate_pages.py"),
            );
            candidates.push(
                ancestor.join("product/preprocessor/preprocessor-tpp/scripts/find_plate_pages.py"),
            );
            candidates.push(
                ancestor.join("rust-preprocessor/preprocessor-tpp/scripts/find_plate_pages.py"),
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

fn read_gdalinfo(path: &Path) -> anyhow::Result<String> {
    let output = Command::new("gdalinfo")
        .arg(path)
        .output()
        .with_context(|| format!("failed to run gdalinfo on {}", path.display()))?;
    if !output.status.success() {
        bail!("gdalinfo failed for {}", path.display());
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

fn basic_plate_fingerprint(pdf_hash: &str, output_name: &str) -> anyhow::Result<String> {
    Ok(hash_fingerprint_components(&[
        TPP_BASIC_PIPELINE_VERSION,
        pdf_hash,
        output_name,
    ]))
}

fn airport_diagram_fingerprint(
    pdf_hash: &str,
    output_name: &str,
    comment: &str,
) -> anyhow::Result<String> {
    Ok(hash_fingerprint_components(&[
        TPP_AIRPORT_DIAGRAM_PIPELINE_VERSION,
        pdf_hash,
        output_name,
        comment,
    ]))
}

fn geotagged_plate_fingerprint(pdf_hash: &str, output_name: &str) -> anyhow::Result<String> {
    Ok(hash_fingerprint_components(&[
        TPP_GEOTAGGED_PIPELINE_VERSION,
        pdf_hash,
        output_name,
    ]))
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

fn package_region(work_dir: &Path, provenance_dir: &Path, region: Region) -> anyhow::Result<usize> {
    let manifest_name = format!("{}_TPP", region.code());
    let zip_name = format!("{}_TPP.zip", region.code());
    let manifest_path = work_dir.join(&manifest_name);
    let zip_path = work_dir.join(&zip_name);
    remove_if_exists(&manifest_path)?;
    remove_if_exists(&zip_path)?;

    let selected = collect_region_pngs(work_dir, region)?;
    let mut manifest_text = String::new();
    manifest_text.push_str(&current_cycle_manifest());
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
        label: format!("tpp-package-{}", region.code()),
        env: Vec::new(),
        stdin_text: Some(stdin_text),
    };
    let outcome = invocation.run_logged(work_dir.join(".rust-logs"))?;
    if !outcome.success {
        bail!("zip failed for region {}", region.code());
    }

    write_package_outputs_jsonl(
        provenance_dir,
        &[PackageOutputRecord {
            label: format!("tpp-{}", region.code().to_ascii_lowercase()),
            chart: None,
            region: region.code().to_string(),
            manifest: manifest_name,
            manifest_sha256: hash_file(&manifest_path)?,
            zip: zip_name,
            zip_sha256: hash_file(&zip_path)?,
        }],
    )?;

    Ok(1)
}

fn collect_region_pngs(work_dir: &Path, region: Region) -> anyhow::Result<Vec<String>> {
    let script = r#"import glob, sys
from pathlib import Path
root = Path(sys.argv[1])
seen = set()
for state in sys.argv[2:]:
    pattern = root / f"plates/**/*-{state}-*.png"
    for path in glob.glob(str(pattern), recursive=True):
        relative = Path(path).relative_to(root).as_posix()
        if relative not in seen:
            seen.add(relative)
            print(relative)
"#;
    let mut command = Command::new("python3");
    command.arg("-c").arg(script).arg(work_dir);
    for state in region.state_codes() {
        command.arg(state);
    }
    let output = command
        .output()
        .with_context(|| format!("failed to enumerate plates under {}", work_dir.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("python plate enumeration failed: {stderr}");
    }

    let stdout = String::from_utf8(output.stdout).context("plate enumeration was not utf-8")?;
    Ok(stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
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

fn sanitize_label(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
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
    path.join("plates").is_dir()
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
                    .is_some_and(|ext| {
                        matches!(ext, "zip" | "pdf" | "PDF" | "png" | "xml" | "php" | "tif")
                    })
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
        Some("zip" | "pdf" | "PDF" | "png" | "xml" | "php" | "tif")
    )
}

fn remove_if_exists(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{geotag_comment_from_gdalinfo, parse_dms_coordinate};

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
}
