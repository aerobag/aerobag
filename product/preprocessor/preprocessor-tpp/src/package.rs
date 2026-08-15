// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{bail, Context};
use chrono::Utc;
use preprocessor_core::{
    PackageAssetManifest, PackageAssetRecord, PlateGeoref, Region, PACKAGE_ASSET_MANIFEST_NAME,
};
use preprocessor_fetch::{hash_file, hash_text, write_package_outputs_jsonl, PackageOutputRecord};
use preprocessor_tools::{command_output_diagnostic_summary, ToolInvocation};
use serde::{Deserialize, Serialize};

use crate::{calculate_cycle, thumbnail::write_tpp_thumbnail, tpp_record_is_deleted};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TppPackagePlan {
    pub package_id: String,
    pub manifest_name: String,
    pub zip_name: String,
    pub manifest_version: String,
    pub region_id: String,
    pub plate_members: Vec<String>,
    pub thumbnails: Vec<TppThumbnailPlan>,
}

impl TppPackagePlan {
    pub fn archive_members(&self) -> anyhow::Result<Vec<String>> {
        if self.plate_members.len() != self.thumbnails.len() {
            bail!(
                "tpp package plan has {} plates but {} thumbnails",
                self.plate_members.len(),
                self.thumbnails.len()
            );
        }
        let mut members = Vec::with_capacity(self.plate_members.len() + self.thumbnails.len());
        for (plate, thumbnail) in self.plate_members.iter().zip(self.thumbnails.iter()) {
            if thumbnail.asset_path != *plate {
                bail!(
                    "tpp package plan thumbnail {} points at {} but expected {}",
                    thumbnail.id,
                    thumbnail.asset_path,
                    plate
                );
            }
            members.push(plate.clone());
            members.push(thumbnail.thumbnail_path.clone());
        }
        Ok(members)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TppThumbnailPlan {
    pub id: String,
    pub asset_path: String,
    pub thumbnail_path: String,
}

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
    let plan = plan_package_region(asset_root, region, manifest_version, artifact_version)?;
    fs::create_dir_all(output_root)
        .with_context(|| format!("failed to create {}", output_root.display()))?;
    let mut thumbnail_sources = BTreeMap::new();
    for thumbnail in &plan.thumbnails {
        let thumbnail_path = write_tpp_thumbnail(asset_root, output_root, thumbnail)?;
        thumbnail_sources.insert(thumbnail.thumbnail_path.clone(), thumbnail_path);
    }
    assemble_package_region(
        asset_root,
        output_root,
        provenance_dir,
        region,
        &plan,
        &thumbnail_sources,
    )
}

pub fn plan_package_region(
    asset_root: &Path,
    region: Region,
    manifest_version: &str,
    artifact_version: &str,
) -> anyhow::Result<TppPackagePlan> {
    let plate_members = collect_region_pngs(asset_root, region)?;
    plan_package_region_from_members(region, manifest_version, artifact_version, plate_members)
}

pub fn plan_package_region_from_members(
    region: Region,
    manifest_version: &str,
    artifact_version: &str,
    plate_members: Vec<String>,
) -> anyhow::Result<TppPackagePlan> {
    let package_id = format!("{}_TPP_{}", region.code(), artifact_version);
    let thumbnails = plate_members.iter().map(thumbnail_plan).collect();
    Ok(TppPackagePlan {
        package_id: package_id.clone(),
        manifest_name: format!("{package_id}.manifest"),
        zip_name: format!("{}_TPP_{}.zip", region.code(), artifact_version),
        manifest_version: manifest_version.to_string(),
        region_id: region.code().to_ascii_lowercase(),
        plate_members,
        thumbnails,
    })
}

pub fn assemble_package_region(
    asset_root: &Path,
    output_root: &Path,
    provenance_dir: &Path,
    region: Region,
    plan: &TppPackagePlan,
    thumbnail_sources: &BTreeMap<String, PathBuf>,
) -> anyhow::Result<usize> {
    let plate_sources = plan
        .plate_members
        .iter()
        .map(|member| (member.clone(), asset_root.join(member)))
        .collect::<BTreeMap<_, _>>();
    assemble_package_region_from_sources(
        asset_root,
        output_root,
        provenance_dir,
        region,
        plan,
        &plate_sources,
        thumbnail_sources,
    )
}

pub fn assemble_package_region_from_sources(
    metadata_root: &Path,
    output_root: &Path,
    provenance_dir: &Path,
    region: Region,
    plan: &TppPackagePlan,
    plate_sources: &BTreeMap<String, PathBuf>,
    thumbnail_sources: &BTreeMap<String, PathBuf>,
) -> anyhow::Result<usize> {
    fs::create_dir_all(output_root)
        .with_context(|| format!("failed to create {}", output_root.display()))?;
    let manifest_path = output_root.join(&plan.manifest_name);
    let zip_path = output_root.join(&plan.zip_name);
    let package_assets_path = output_root.join(PACKAGE_ASSET_MANIFEST_NAME);
    remove_if_exists(&manifest_path)?;
    remove_if_exists(&zip_path)?;
    remove_if_exists(&package_assets_path)?;

    stage_member_source_files(output_root, &plan.plate_members, plate_sources)?;
    stage_thumbnail_files(output_root, thumbnail_sources)?;
    let archive_members = plan.archive_members()?;
    write_package_asset_manifest(
        metadata_root,
        &package_assets_path,
        &plan.package_id,
        &archive_members,
        plate_sources,
    )?;
    write_tpp_manifest(&manifest_path, &plan.manifest_version, &archive_members)?;
    write_tpp_zip(
        output_root,
        region,
        &plan.zip_name,
        &plan.manifest_name,
        &archive_members,
    )?;

    write_package_outputs_jsonl(
        provenance_dir,
        &[PackageOutputRecord {
            label: format!("tpp-{}", region.code().to_ascii_lowercase()),
            chart: None,
            region: region.code().to_ascii_lowercase(),
            manifest: plan.manifest_name.clone(),
            manifest_sha256: hash_file(&manifest_path)?,
            zip: plan.zip_name.clone(),
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
        bail!(
            "python plate enumeration failed under {}; {}",
            asset_root.display(),
            command_output_diagnostic_summary(&output)
        );
    }

    let stdout = String::from_utf8(output.stdout).context("plate enumeration was not utf-8")?;
    let mut members = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    members.sort();
    Ok(members)
}

fn thumbnail_plan(member: &String) -> TppThumbnailPlan {
    let thumbnail_path = Path::new("thumbnails")
        .join(member)
        .to_string_lossy()
        .replace('\\', "/");
    let id = short_stable_hash(&thumbnail_path);
    TppThumbnailPlan {
        id,
        asset_path: member.clone(),
        thumbnail_path,
    }
}

fn short_stable_hash(value: &str) -> String {
    hash_text(value).chars().take(24).collect()
}

fn stage_member_source_files(
    output_root: &Path,
    members: &[String],
    sources: &BTreeMap<String, PathBuf>,
) -> anyhow::Result<()> {
    for member in members {
        let relative_path = Path::new(member);
        let source = sources
            .get(member)
            .with_context(|| format!("tpp package plan missing source for {member}"))?;
        let destination = output_root.join(relative_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        remove_if_exists(&destination)?;
        match fs::hard_link(source, &destination) {
            Ok(()) => {}
            Err(_) => {
                fs::copy(source, &destination).with_context(|| {
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

fn stage_thumbnail_files(
    output_root: &Path,
    thumbnail_sources: &BTreeMap<String, PathBuf>,
) -> anyhow::Result<()> {
    for (member, source) in thumbnail_sources {
        let destination = output_root.join(member);
        if source == &destination {
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        remove_if_exists(&destination)?;
        match fs::hard_link(source, &destination) {
            Ok(()) => {}
            Err(_) => {
                fs::copy(source, &destination).with_context(|| {
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

fn write_tpp_manifest(
    manifest_path: &Path,
    manifest_version: &str,
    members: &[String],
) -> anyhow::Result<()> {
    let mut manifest_text = String::new();
    manifest_text.push_str(manifest_version);
    manifest_text.push('\n');
    for path in members {
        manifest_text.push_str(path);
        manifest_text.push('\n');
    }
    fs::write(manifest_path, manifest_text)
        .with_context(|| format!("failed to write {}", manifest_path.display()))
}

fn write_tpp_zip(
    output_root: &Path,
    region: Region,
    zip_name: &str,
    manifest_name: &str,
    members: &[String],
) -> anyhow::Result<()> {
    let mut stdin_text = String::new();
    for path in members {
        stdin_text.push_str(path);
        stdin_text.push('\n');
    }
    stdin_text.push_str(PACKAGE_ASSET_MANIFEST_NAME);
    stdin_text.push('\n');
    stdin_text.push_str(manifest_name);
    stdin_text.push('\n');

    let invocation = ToolInvocation {
        program: "zip".to_string(),
        args: vec![
            "-q".to_string(),
            "-0".to_string(),
            zip_name.to_string(),
            "-@".to_string(),
        ],
        cwd: output_root.to_path_buf(),
        label: format!("tpp-package-{}", region.code()),
        env: Vec::new(),
        stdin_text: Some(stdin_text),
    };
    let outcome = invocation.run_logged(output_root.join(".rust-logs"))?;
    invocation.ensure_success(
        &outcome,
        &format!("zip failed for region {}", region.code()),
    )
}

fn write_package_asset_manifest(
    metadata_root: &Path,
    output_path: &Path,
    package_id: &str,
    members: &[String],
    plate_sources: &BTreeMap<String, PathBuf>,
) -> anyhow::Result<()> {
    let tpp_metadata = load_tpp_asset_metadata(metadata_root)?;
    let assets = members
        .iter()
        .filter(|member| member.ends_with(".png") && !member.starts_with("thumbnails/"))
        .map(|member| {
            let source = plate_sources
                .get(member)
                .with_context(|| format!("package asset manifest missing source for {member}"))?;
            build_package_asset_records_for_member(member, source, &tpp_metadata)
        })
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
    cifp_procedure_id: Option<String>,
}

fn cifp_procedure_id_from_faanfd18(chart_code: &str, faanfd18: &str) -> Option<String> {
    let fields = faanfd18
        .trim()
        .split('.')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    let value = match chart_code.trim().to_ascii_uppercase().as_str() {
        "DP" | "ODP" => fields.first(),
        "STR" => fields.last(),
        _ => None,
    }?;
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        .then(|| value.to_ascii_uppercase())
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
                    if tpp_record_is_deleted(record) {
                        continue;
                    }
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
                    let faanfd18 = record
                        .children()
                        .find(|node| node.has_tag_name("faanfd18"))
                        .and_then(|node| node.text())
                        .unwrap_or_default();
                    let label = rendered_plate_label(&chart_code, &state_id, &chart_name);
                    metadata.insert(
                        (apt_id.clone(), label),
                        TppAssetMetadata {
                            chart_code: chart_code.trim().to_uppercase(),
                            icao_airport_id: icao_airport_id.clone(),
                            procedure_uid,
                            cifp_procedure_id: cifp_procedure_id_from_faanfd18(
                                &chart_code,
                                faanfd18,
                            ),
                        },
                    );
                }
            }
        }
    }
    Ok(metadata)
}

fn build_package_asset_records_for_member(
    member: &str,
    source_png: &Path,
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
    let georef = read_plate_georef_from_png(source_png)?;

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
                    cifp_procedure_id: metadata.cifp_procedure_id.clone(),
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
        cifp_procedure_id: metadata.and_then(|value| value.cifp_procedure_id.clone()),
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
        bail!(
            "exiftool failed while reading {}; command=\"exiftool -s3 -UserComment {}\" {}",
            png_path.display(),
            png_path.display(),
            command_output_diagnostic_summary(&output)
        );
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
            pixel_x_from_lon: *a,
            pixel_x_from_lat: *c,
            pixel_x_offset: *e,
            pixel_y_from_lon: *b,
            pixel_y_from_lat: *d,
            pixel_y_offset: *f,
        })),
        _ => bail!("unsupported plate georef comment: {comment}"),
    }
}

fn infer_plate_document_type(chart_code: Option<&str>, label: &str) -> &'static str {
    match chart_code.map(|value| value.trim().to_ascii_uppercase()) {
        Some(code) if code == "APD" => "airport_diagram",
        Some(code) if code == "IAP" => "approach",
        Some(code) if code == "DP" || code == "ODP" => "departure",
        Some(code) if code == "STR" => "star",
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
            } else if label.starts_with("STR-") {
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
    use crate::{
        read_airport_diagram_tags, thumbnail::write_tpp_thumbnail_from_source,
        AirportDiagramGeoref, PlateRotation,
    };
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
        fs::write(airport_dir.join("STR-WA-GLASR THREE.png"), ONE_BY_ONE_PNG).unwrap();

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
            "plates/RNT/STR-WA-GLASR THREE.png"
        );
        assert_eq!(
            manifest.assets[0].thumbnail_path,
            "thumbnails/plates/RNT/STR-WA-GLASR THREE.png"
        );
        assert_eq!(manifest.assets[0].document_type, "star");
        assert_eq!(manifest.assets[0].georef, None);
        assert_eq!(
            archive
                .by_name("plates/RNT/STR-WA-GLASR THREE.png")
                .unwrap()
                .compression(),
            CompressionMethod::Stored
        );
        assert_eq!(
            archive
                .by_name("thumbnails/plates/RNT/STR-WA-GLASR THREE.png")
                .unwrap()
                .compression(),
            CompressionMethod::Stored
        );
    }

    #[test]
    fn tpp_package_can_assemble_from_explicit_plate_sources() {
        let temp = tempdir().unwrap();
        let metadata_root = temp.path().join("metadata-root");
        let source_root = temp.path().join("source-root");
        let output_root = temp.path().join("output-root");
        let provenance_dir = temp.path().join("meta/provenance/tpp-nw");
        fs::create_dir_all(&metadata_root).unwrap();
        fs::create_dir_all(&provenance_dir).unwrap();
        let source_png = source_root.join("unit-a/plates/RNT/STAR-WA-GLASR THREE.png");
        fs::create_dir_all(source_png.parent().unwrap()).unwrap();
        fs::write(&source_png, ONE_BY_ONE_PNG).unwrap();

        let member = "plates/RNT/STAR-WA-GLASR THREE.png".to_string();
        let plan =
            plan_package_region_from_members(Region::Nw, "2604", "2604", vec![member.clone()])
                .unwrap();
        let thumbnail = write_tpp_thumbnail_from_source(
            &source_png,
            &output_root,
            plan.thumbnails.first().unwrap(),
        )
        .unwrap();
        let plate_sources = BTreeMap::from([(member.clone(), source_png)]);
        let thumbnail_sources = BTreeMap::from([(
            plan.thumbnails.first().unwrap().thumbnail_path.clone(),
            thumbnail,
        )]);

        assemble_package_region_from_sources(
            &metadata_root,
            &output_root,
            &provenance_dir,
            Region::Nw,
            &plan,
            &plate_sources,
            &thumbnail_sources,
        )
        .unwrap();

        let zip_path = output_root.join(&plan.zip_name);
        let file = fs::File::open(zip_path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        assert_eq!(
            archive.by_name(&member).unwrap().compression(),
            CompressionMethod::Stored
        );
        assert!(archive
            .by_name("thumbnails/plates/RNT/STAR-WA-GLASR THREE.png")
            .is_ok());
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
                pixel_x_from_lon: 10.0,
                pixel_x_from_lat: 30.0,
                pixel_x_offset: 50.0,
                pixel_y_from_lon: 20.0,
                pixel_y_from_lat: 40.0,
                pixel_y_offset: 60.0,
            })
        );
    }

    #[test]
    fn krnt_airport_diagram_georef_maps_airport_onto_225_dpi_image() {
        let temp = tempdir().unwrap();
        let tags_path = temp.path().join("avare_aptdiags.php");
        fs::write(
            &tags_path,
            "RNT,1.360544217687E-05,0,0,-9.2182890855452E-06,-122.227068027211,47.5041159660767,73500.0000000042,0,0,-108480.000000006,8983689.50000051,5153246.50000029\n",
        )
        .unwrap();

        let tags = read_airport_diagram_tags(&tags_path).unwrap();
        let georef = parse_plate_georef_comment(&tags.get("RNT").unwrap().to_comment())
            .unwrap()
            .expect("KRNT georef");
        let PlateGeoref::AirportDiagramTransformV1 {
            pixel_x_from_lon,
            pixel_x_from_lat,
            pixel_x_offset,
            pixel_y_from_lon,
            pixel_y_from_lat,
            pixel_y_offset,
        } = georef
        else {
            panic!("expected airport diagram transform");
        };

        let airport_lon = -122.21575;
        let airport_lat = 47.49313888888889;
        let image_x =
            airport_lon * pixel_x_from_lon + airport_lat * pixel_x_from_lat + pixel_x_offset;
        let image_y =
            airport_lon * pixel_y_from_lon + airport_lat * pixel_y_from_lat + pixel_y_offset;

        // The source inverse transform is expressed at 300 DPI; production renders at 225 DPI.
        let render_scale =
            f64::from(crate::TPP_RENDER_DPI) / crate::TPP_AIRPORT_DIAGRAM_GEOREF_SOURCE_DPI;
        let expected_x =
            (airport_lon * 73500.0000000042 + airport_lat * 0.0 + 8983689.50000051) * render_scale;
        let expected_y =
            (airport_lon * 0.0 + airport_lat * -108480.000000006 + 5153246.50000029) * render_scale;

        assert!(
            (image_x - expected_x).abs() < 0.01 && (image_y - expected_y).abs() < 0.01,
            "KRNT airport should map near ({expected_x:.2}, {expected_y:.2}) in the 1210x1856 image; got ({image_x:.2}, {image_y:.2})"
        );
    }

    #[test]
    fn sideways_airport_diagram_georef_drives_rotation_and_remains_aligned() {
        let source = AirportDiagramGeoref::from_source_inverse(&[
            "0",
            "-49260.0000000028",
            "-65580.0000000037",
            "0",
            "2728481.00000016",
            "-5371284.5000003",
        ])
        .unwrap();
        assert_eq!(source.north_up_rotation(), PlateRotation::Clockwise90);

        let lon = -109.0;
        let lat = 41.6;
        let source_x =
            lon * source.pixel_x_from_lon + lat * source.pixel_x_from_lat + source.pixel_x_offset;
        let source_y =
            lon * source.pixel_y_from_lon + lat * source.pixel_y_from_lat + source.pixel_y_offset;
        let rotated = source.rotated(PlateRotation::Clockwise90, 1_200, 1_800);
        let rotated_x = lon * rotated.pixel_x_from_lon
            + lat * rotated.pixel_x_from_lat
            + rotated.pixel_x_offset;
        let rotated_y = lon * rotated.pixel_y_from_lon
            + lat * rotated.pixel_y_from_lat
            + rotated.pixel_y_offset;

        assert!((rotated_x - (1_799.0 - source_y)).abs() < 0.01);
        assert!((rotated_y - source_x).abs() < 0.01);
    }

    #[test]
    fn upside_down_airport_diagram_georef_drives_half_turn() {
        let source = AirportDiagramGeoref::from_source_inverse(&[
            "81599.9966997059",
            "23.2081901876297",
            "-28.703070511441",
            "100919.995918227",
            "7834248.78300751",
            "-3632429.70570752",
        ])
        .unwrap();
        assert_eq!(source.north_up_rotation(), PlateRotation::HalfTurn);
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
    fn extracts_cifp_ids_from_faa_departure_and_arrival_metadata() {
        assert_eq!(
            cifp_procedure_id_from_faanfd18("ODP", "LGD1.LGD"),
            Some("LGD1".to_string())
        );
        assert_eq!(
            cifp_procedure_id_from_faanfd18("STR", "CHINS.CHINS5"),
            Some("CHINS5".to_string())
        );
    }

    #[test]
    fn loads_faa_cifp_ids_into_tpp_asset_metadata() {
        let temp = tempdir().unwrap();
        fs::write(
            temp.path().join("d-TPP_Metafile.xml"),
            r#"<digital_tpp>
                <state_code ID="OR">
                  <city_name>
                    <airport_name apt_ident="LGD" icao_ident="KLGD">
                      <record>
                        <chart_code>ODP</chart_code>
                        <chart_name>LA GRANDE ONE (OBSTACLE)</chart_name>
                        <useraction>C</useraction>
                        <procuid>40571</procuid>
                        <faanfd18>LGD1.LGD</faanfd18>
                      </record>
                      <record>
                        <chart_code>ODP</chart_code>
                        <chart_name>LA GRANDE ONE (OBSTACLE)</chart_name>
                        <useraction>D</useraction>
                        <procuid>deleted-record</procuid>
                        <faanfd18>DELETED.LGD</faanfd18>
                      </record>
                    </airport_name>
                  </city_name>
                </state_code>
              </digital_tpp>"#,
        )
        .unwrap();

        let metadata = load_tpp_asset_metadata(temp.path()).unwrap();
        let record = metadata
            .get(&(
                "LGD".to_string(),
                "ODP-OR-LA GRANDE ONE (OBSTACLE)".to_string(),
            ))
            .expect("LGD1 plate metadata");
        assert_eq!(record.procedure_uid.as_deref(), Some("40571"));
        assert_eq!(record.cifp_procedure_id.as_deref(), Some("LGD1"));
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
