use std::{collections::BTreeMap, fs, path::Path, process::Command};

use anyhow::{bail, Context};
use chrono::Utc;
use preprocessor_core::{
    PackageAssetManifest, PackageAssetRecord, PlateGeoref, Region, PACKAGE_ASSET_MANIFEST_NAME,
};
use preprocessor_fetch::{hash_file, write_package_outputs_jsonl, PackageOutputRecord};
use preprocessor_tools::{write_thumbnail_from_png, ToolInvocation};

use crate::calculate_cycle;

pub(crate) fn package_region(
    asset_root: &Path,
    output_root: &Path,
    provenance_dir: &Path,
    region: Region,
) -> anyhow::Result<usize> {
    let manifest_cycle = current_cycle_manifest();
    package_region_versioned(
        asset_root,
        output_root,
        provenance_dir,
        region,
        &manifest_cycle,
        &manifest_cycle,
    )
}

pub(crate) fn package_region_versioned(
    asset_root: &Path,
    output_root: &Path,
    provenance_dir: &Path,
    region: Region,
    manifest_version: &str,
    artifact_version: &str,
) -> anyhow::Result<usize> {
    let package_id = format!("{}_TPP_{}", region.code(), artifact_version);
    let manifest_name = format!("{package_id}.manifest");
    let zip_name = format!("{}_TPP_{}.zip", region.code(), artifact_version);
    fs::create_dir_all(output_root)
        .with_context(|| format!("failed to create {}", output_root.display()))?;
    let manifest_path = output_root.join(&manifest_name);
    let zip_path = output_root.join(&zip_name);
    remove_if_exists(&manifest_path)?;
    remove_if_exists(&zip_path)?;

    let selected = collect_region_pngs(asset_root, region)?;
    let package_assets_path = output_root.join(PACKAGE_ASSET_MANIFEST_NAME);
    remove_if_exists(&package_assets_path)?;
    stage_member_files(asset_root, output_root, &selected)?;
    let selected = with_thumbnail_members(asset_root, output_root, &selected)?;
    write_package_asset_manifest(asset_root, &package_assets_path, &package_id, &selected)?;
    let mut manifest_text = String::new();
    manifest_text.push_str(manifest_version);
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
    stdin_text.push_str(PACKAGE_ASSET_MANIFEST_NAME);
    stdin_text.push('\n');
    stdin_text.push_str(&manifest_name);
    stdin_text.push('\n');

    let invocation = ToolInvocation {
        program: "zip".to_string(),
        args: vec![
            "-q".to_string(),
            "-0".to_string(),
            zip_name.clone(),
            "-@".to_string(),
        ],
        cwd: output_root.to_path_buf(),
        label: format!("tpp-package-{}", region.code()),
        env: Vec::new(),
        stdin_text: Some(stdin_text),
    };
    let outcome = invocation.run_logged(output_root.join(".rust-logs"))?;
    if !outcome.success {
        bail!("zip failed for region {}", region.code());
    }

    write_package_outputs_jsonl(
        provenance_dir,
        &[PackageOutputRecord {
            label: format!("tpp-{}", region.code().to_ascii_lowercase()),
            chart: None,
            region: region.code().to_ascii_lowercase(),
            manifest: manifest_name,
            manifest_sha256: hash_file(&manifest_path)?,
            zip: zip_name,
            zip_sha256: hash_file(&zip_path)?,
            metadata: BTreeMap::new(),
        }],
    )?;

    Ok(1)
}

fn collect_region_pngs(asset_root: &Path, region: Region) -> anyhow::Result<Vec<String>> {
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
    command.arg("-c").arg(script).arg(asset_root);
    for state in region.state_codes() {
        command.arg(state);
    }
    let output = command
        .output()
        .with_context(|| format!("failed to enumerate plates under {}", asset_root.display()))?;
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

fn with_thumbnail_members(
    asset_root: &Path,
    output_root: &Path,
    members: &[String],
) -> anyhow::Result<Vec<String>> {
    let mut all = Vec::with_capacity(members.len() * 2);
    let thumbnail_root = output_root.join("thumbnails");
    for member in members {
        all.push(member.clone());
        let asset_path = Path::new(member);
        let source = asset_root.join(asset_path);
        let thumbnail_path = Path::new("thumbnails")
            .join(asset_path)
            .to_string_lossy()
            .replace('\\', "/");
        if !output_root.join(&thumbnail_path).is_file() {
            write_thumbnail_from_png(&source, &thumbnail_root, asset_path)?;
        }
        all.push(thumbnail_path);
    }
    Ok(all)
}

fn stage_member_files(
    asset_root: &Path,
    output_root: &Path,
    members: &[String],
) -> anyhow::Result<()> {
    for member in members {
        let relative_path = Path::new(member);
        let source = asset_root.join(relative_path);
        let destination = output_root.join(relative_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        remove_if_exists(&destination)?;
        match fs::hard_link(&source, &destination) {
            Ok(()) => {}
            Err(_) => {
                fs::copy(&source, &destination).with_context(|| {
                    format!(
                        "failed to copy {} to {}",
                        source.display(),
                        destination.display()
                    )
                })?;
            }
        }
    }
    Ok(())
}

fn write_package_asset_manifest(
    asset_root: &Path,
    output_path: &Path,
    package_id: &str,
    members: &[String],
) -> anyhow::Result<()> {
    let tpp_metadata = load_tpp_asset_metadata(asset_root)?;
    let assets = members
        .iter()
        .filter(|member| member.ends_with(".png") && !member.starts_with("thumbnails/"))
        .map(|member| build_package_asset_records_for_member(asset_root, member, &tpp_metadata))
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let manifest = PackageAssetManifest {
        schema_version: 2,
        family_id: "tpp".to_string(),
        package_id: package_id.to_string(),
        assets,
    };
    fs::write(
        output_path,
        serde_json::to_vec_pretty(&manifest)
            .context("failed to encode tpp package asset manifest")?,
    )
    .with_context(|| format!("failed to write {}", output_path.display()))
}

fn rendered_plate_label(chart_code: &str, state_id: &str, chart_name: &str) -> String {
    format!(
        "{}-{}-{}",
        chart_code.trim().to_uppercase(),
        state_id.trim().to_uppercase(),
        chart_name.trim().to_uppercase().replace('/', " AND ")
    )
}

#[derive(Debug, Clone)]
struct TppAssetMetadata {
    chart_code: String,
    icao_airport_id: Option<String>,
    procedure_uid: Option<String>,
}

fn load_tpp_asset_metadata(
    asset_root: &Path,
) -> anyhow::Result<BTreeMap<(String, String), TppAssetMetadata>> {
    let xml_path = asset_root.join("d-TPP_Metafile.xml");
    if !xml_path.is_file() {
        return Ok(BTreeMap::new());
    }
    let text = fs::read_to_string(&xml_path)
        .with_context(|| format!("failed to read {}", xml_path.display()))?;
    let document = roxmltree::Document::parse(&text)
        .with_context(|| format!("failed to parse {}", xml_path.display()))?;

    let mut metadata = BTreeMap::new();
    for state in document
        .descendants()
        .filter(|node| node.has_tag_name("state_code"))
    {
        let state_id = state.attribute("ID").unwrap_or("").trim().to_string();
        if state_id.is_empty() {
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
                let icao_airport_id = airport
                    .attribute("icao_ident")
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
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
                        .to_string();
                    let chart_code = record
                        .children()
                        .find(|node| node.has_tag_name("chart_code"))
                        .and_then(|node| node.text())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if chart_name.is_empty() || chart_code.is_empty() {
                        continue;
                    }
                    let procedure_uid = record
                        .children()
                        .find(|node| node.has_tag_name("procuid"))
                        .and_then(|node| node.text())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string);
                    let label = rendered_plate_label(&chart_code, &state_id, &chart_name);
                    metadata.insert(
                        (apt_id.clone(), label),
                        TppAssetMetadata {
                            chart_code: chart_code.trim().to_uppercase(),
                            icao_airport_id: icao_airport_id.clone(),
                            procedure_uid,
                        },
                    );
                }
            }
        }
    }
    Ok(metadata)
}

fn build_package_asset_records_for_member(
    asset_root: &Path,
    member: &str,
    tpp_metadata: &BTreeMap<(String, String), TppAssetMetadata>,
) -> anyhow::Result<Vec<PackageAssetRecord>> {
    let asset_path = Path::new(member);
    let owner_id = asset_path
        .components()
        .nth(1)
        .and_then(|value| value.as_os_str().to_str())
        .unwrap_or_default()
        .to_string();
    let filename = asset_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    let label = asset_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    let normalized_hotspot_label =
        strip_rendered_hotspot_page_suffix(&label).unwrap_or_else(|| label.clone());
    let display_label = pretty_packaged_plate_label(&normalized_hotspot_label);
    let thumbnail_path = Path::new("thumbnails")
        .join(asset_path)
        .to_string_lossy()
        .replace('\\', "/");
    let georef = read_plate_georef_from_png(&asset_root.join(member))?;

    let hotspot_records = if normalized_hotspot_label.starts_with("HOT-") {
        let hotspot_prefix = format!("{normalized_hotspot_label}-");
        let mut deduped = BTreeMap::new();
        for ((apt_id, asset_label), metadata) in tpp_metadata.iter() {
            if metadata.chart_code != "HOT" {
                continue;
            }
            if asset_label != &normalized_hotspot_label && !asset_label.starts_with(&hotspot_prefix)
            {
                continue;
            }
            deduped
                .entry(apt_id.clone())
                .or_insert_with(|| metadata.clone());
        }
        deduped.into_iter().collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    if !hotspot_records.is_empty() {
        return Ok(hotspot_records
            .into_iter()
            .map(|(apt_id, metadata)| {
                let canonical_airport_id = metadata
                    .icao_airport_id
                    .clone()
                    .unwrap_or_else(|| apt_id.clone());
                PackageAssetRecord {
                    id: format!("plate:{canonical_airport_id}:{filename}"),
                    airport_id: canonical_airport_id,
                    icao_airport_id: metadata.icao_airport_id.clone(),
                    label: display_label.clone(),
                    asset_kind: "png".to_string(),
                    document_type: infer_plate_document_type(
                        Some(metadata.chart_code.as_str()),
                        &normalized_hotspot_label,
                    )
                    .to_string(),
                    asset_path: member.to_string(),
                    thumbnail_path: thumbnail_path.clone(),
                    procedure_uid: metadata.procedure_uid.clone(),
                    georef: georef.clone(),
                }
            })
            .collect());
    }

    let metadata = tpp_metadata.get(&(owner_id.clone(), label.clone()));
    let canonical_airport_id = metadata
        .and_then(|value| value.icao_airport_id.clone())
        .unwrap_or_else(|| owner_id.clone());
    Ok(vec![PackageAssetRecord {
        id: format!("plate:{canonical_airport_id}:{filename}"),
        airport_id: canonical_airport_id,
        icao_airport_id: metadata.and_then(|value| value.icao_airport_id.clone()),
        label: display_label,
        asset_kind: "png".to_string(),
        document_type: infer_plate_document_type(
            metadata.map(|value| value.chart_code.as_str()),
            &normalized_hotspot_label,
        )
        .to_string(),
        asset_path: member.to_string(),
        thumbnail_path,
        procedure_uid: metadata.and_then(|value| value.procedure_uid.clone()),
        georef,
    }])
}

fn strip_rendered_hotspot_page_suffix(label: &str) -> Option<String> {
    label.rsplit_once('-').and_then(|(base, suffix)| {
        if base.trim_end().ends_with("HOT SPOT") && suffix.trim().parse::<u32>().is_ok() {
            Some(base.trim().to_string())
        } else {
            None
        }
    })
}

fn pretty_packaged_plate_label(label: &str) -> String {
    if let Some((prefix, state_code, remainder)) = split_tpp_prefix(label) {
        if prefix == "HOT" {
            return pretty_hotspot_label(state_code, remainder);
        }
    }
    label.to_string()
}

fn split_tpp_prefix(label: &str) -> Option<(&str, &str, &str)> {
    let mut parts = label.splitn(3, '-');
    let chart_code = parts.next()?;
    let state_code = parts.next()?;
    let remainder = parts.next()?;
    Some((chart_code, state_code, remainder))
}

fn pretty_hotspot_label(state_code: &str, remainder: &str) -> String {
    if remainder == "HOT SPOT" {
        return format!("{state_code} Hot Spots");
    }
    remainder
        .strip_prefix("HOT SPOT-")
        .map(|suffix| format!("{state_code} Hot Spots {suffix}"))
        .unwrap_or_else(|| remainder.to_string())
}

fn read_plate_georef_from_png(png_path: &Path) -> anyhow::Result<Option<PlateGeoref>> {
    let output = Command::new("exiftool")
        .arg("-s3")
        .arg("-UserComment")
        .arg(png_path)
        .output()
        .with_context(|| format!("failed to run exiftool on {}", png_path.display()))?;
    if !output.status.success() {
        bail!("exiftool failed while reading {}", png_path.display());
    }
    let comment = String::from_utf8(output.stdout).context("exiftool output was not utf-8")?;
    parse_plate_georef_comment(comment.trim())
}

fn parse_plate_georef_comment(comment: &str) -> anyhow::Result<Option<PlateGeoref>> {
    if comment.is_empty() {
        return Ok(None);
    }
    let values = comment
        .split('|')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.parse::<f64>())
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("invalid plate georef comment: {comment}"))?;
    match values.as_slice() {
        [pixels_per_longitude, pixels_per_latitude, top_left_lon, top_left_lat] => {
            Ok(Some(PlateGeoref::PlateTransformV1 {
                pixels_per_longitude: *pixels_per_longitude,
                pixels_per_latitude: *pixels_per_latitude,
                top_left_lon: *top_left_lon,
                top_left_lat: *top_left_lat,
            }))
        }
        [a, b, c, d, e, f] => Ok(Some(PlateGeoref::AirportDiagramTransformV1 {
            pixel_x_from_lon: *a / 2.0,
            pixel_x_from_lat: *c / 2.0,
            pixel_x_offset: *e / 2.0,
            pixel_y_from_lon: *b / 2.0,
            pixel_y_from_lat: *d / 2.0,
            pixel_y_offset: *f / 2.0,
        })),
        _ => bail!("unsupported plate georef comment: {comment}"),
    }
}

fn infer_plate_document_type(chart_code: Option<&str>, label: &str) -> &'static str {
    match chart_code.map(|value| value.trim().to_ascii_uppercase()) {
        Some(code) if code == "APD" => "airport_diagram",
        Some(code) if code == "IAP" => "approach",
        Some(code) if code == "DP" || code == "ODP" => "departure",
        Some(code) if code == "STAR" => "star",
        Some(code) if code == "HOT" => "hotspot",
        Some(code) if code == "MIN" => {
            if label.contains("TAKEOFF MINIMUMS") {
                "takeoff_minimums"
            } else if label.contains("ALTERNATE MINIMUMS") {
                "alternate_minimums"
            } else {
                "minimums"
            }
        }
        Some(_) => "other",
        None => {
            if label.starts_with("APD-") {
                "airport_diagram"
            } else if label.starts_with("MIN-") && label.contains("TAKEOFF MINIMUMS") {
                "takeoff_minimums"
            } else if label.starts_with("MIN-") && label.contains("ALTERNATE MINIMUMS") {
                "alternate_minimums"
            } else if label.starts_with("MIN-") {
                "minimums"
            } else if label.starts_with("HOT-") {
                "hotspot"
            } else if label.starts_with("IAP-") {
                "approach"
            } else if label.starts_with("DP-") || label.starts_with("ODP-") {
                "departure"
            } else if label.starts_with("STAR-") {
                "star"
            } else {
                "other"
            }
        }
    }
}

fn current_cycle_manifest() -> String {
    let (manifest_cycle, _) = calculate_cycle(1, Utc::now());
    manifest_cycle.to_string()
}

fn remove_if_exists(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use zip::{CompressionMethod, ZipArchive};

    const ONE_BY_ONE_PNG: &[u8] = &[
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D',
        b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, b'I', b'D', b'A', b'T', 0x78, 0x9C, 0x63, 0xF8,
        0xCF, 0xC0, 0xF0, 0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00,
        0x00, 0x00, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn tpp_package_includes_package_asset_manifest() {
        let temp = tempdir().unwrap();
        let asset_root = temp.path().join("asset-root");
        let output_root = temp.path().join("output-root");
        let provenance_dir = temp.path().join("meta/provenance/tpp-nw");
        fs::create_dir_all(&provenance_dir).unwrap();
        let airport_dir = asset_root.join("plates/RNT");
        fs::create_dir_all(&airport_dir).unwrap();
        fs::write(airport_dir.join("STAR-WA-GLASR THREE.png"), ONE_BY_ONE_PNG).unwrap();

        package_region_versioned(
            &asset_root,
            &output_root,
            &provenance_dir,
            Region::Nw,
            "2604",
            "2604",
        )
        .unwrap();
        let zip_path = output_root.join("NW_TPP_2604.zip");
        let file = fs::File::open(zip_path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        let manifest: PackageAssetManifest =
            serde_json::from_reader(archive.by_name(PACKAGE_ASSET_MANIFEST_NAME).unwrap()).unwrap();

        assert_eq!(manifest.family_id, "tpp");
        assert_eq!(manifest.package_id, "NW_TPP_2604");
        assert_eq!(manifest.assets.len(), 1);
        assert_eq!(
            manifest.assets[0].asset_path,
            "plates/RNT/STAR-WA-GLASR THREE.png"
        );
        assert_eq!(
            manifest.assets[0].thumbnail_path,
            "thumbnails/plates/RNT/STAR-WA-GLASR THREE.png"
        );
        assert_eq!(manifest.assets[0].document_type, "star");
        assert_eq!(manifest.assets[0].georef, None);
        assert_eq!(
            archive
                .by_name("plates/RNT/STAR-WA-GLASR THREE.png")
                .unwrap()
                .compression(),
            CompressionMethod::Stored
        );
        assert_eq!(
            archive
                .by_name("thumbnails/plates/RNT/STAR-WA-GLASR THREE.png")
                .unwrap()
                .compression(),
            CompressionMethod::Stored
        );
    }

    #[test]
    fn parses_plate_georef_comment() {
        assert_eq!(
            parse_plate_georef_comment("2.5|-3.5|-122.3|47.5").unwrap(),
            Some(PlateGeoref::PlateTransformV1 {
                pixels_per_longitude: 2.5,
                pixels_per_latitude: -3.5,
                top_left_lon: -122.3,
                top_left_lat: 47.5,
            })
        );
    }

    #[test]
    fn parses_airport_diagram_georef_comment() {
        assert_eq!(
            parse_plate_georef_comment("10|20|30|40|50|60").unwrap(),
            Some(PlateGeoref::AirportDiagramTransformV1 {
                pixel_x_from_lon: 5.0,
                pixel_x_from_lat: 15.0,
                pixel_x_offset: 25.0,
                pixel_y_from_lon: 10.0,
                pixel_y_from_lat: 20.0,
                pixel_y_offset: 30.0,
            })
        );
    }

    #[test]
    fn infers_hotspot_document_type() {
        assert_eq!(
            infer_plate_document_type(Some("HOT"), "HOT-WA-HOT SPOT-0"),
            "hotspot"
        );
        assert_eq!(
            infer_plate_document_type(None, "HOT-WA-HOT SPOT-1"),
            "hotspot"
        );
    }

    #[test]
    fn pretty_packaged_hotspot_label_keeps_state_and_pluralizes() {
        assert_eq!(
            pretty_packaged_plate_label("HOT-WA-HOT SPOT"),
            "WA Hot Spots"
        );
        assert_eq!(
            pretty_packaged_plate_label("HOT-WY-HOT SPOT-1"),
            "WY Hot Spots 1"
        );
    }
}
