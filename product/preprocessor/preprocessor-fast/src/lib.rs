use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::Serialize;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

#[derive(Debug, Clone)]
pub struct BuildTfrRequest {
    pub input_dir: PathBuf,
    pub output_dir: PathBuf,
    pub version_label: String,
    pub generated_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct BuildTfrResult {
    pub manifest_path: PathBuf,
    pub structured_json_path: PathBuf,
    pub zip_path: PathBuf,
    pub notam_count: usize,
    pub area_group_count: usize,
}

#[derive(Debug, Clone)]
pub struct BuildTfrAvareParityResult {
    pub tfr_manifest_path: PathBuf,
    pub tfr_text_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct TfrManifest {
    schema_version: u32,
    version_label: String,
    generated_at_utc: String,
    files: TfrManifestFiles,
    counts: TfrManifestCounts,
}

#[derive(Debug, Clone, Serialize)]
struct TfrManifestFiles {
    structured_json: String,
    zip: String,
}

#[derive(Debug, Clone, Serialize)]
struct TfrManifestCounts {
    notams: usize,
    area_groups: usize,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct TfrListEntry {
    notam_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextMode {
    DateEffective,
    DateExpire,
    Upper,
    Lower,
    UpperUnit,
    LowerUnit,
    GeoLat,
    GeoLon,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredTfrDataset {
    schema_version: u32,
    version_label: String,
    generated_at_utc: String,
    notam_count: usize,
    area_group_count: usize,
    areas: Vec<StructuredTfrArea>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredTfrArea {
    notam_id: String,
    area_index: usize,
    schedule_fragments: Vec<StructuredTfrScheduleFragment>,
    upper_limit: StructuredTfrLimit,
    lower_limit: StructuredTfrLimit,
    polygon: Vec<StructuredTfrPoint>,
    avare_text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredTfrScheduleFragment {
    kind: String,
    value_utc: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredTfrLimit {
    value_text: String,
    unit: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredTfrPoint {
    lat: f64,
    lon: f64,
}

#[derive(Debug, Clone)]
struct ParsedTfrArea {
    notam_id: String,
    area_index: usize,
    schedule_fragments: Vec<StructuredTfrScheduleFragment>,
    upper_value_text: String,
    upper_unit: String,
    lower_value_text: String,
    lower_unit: String,
    polygon: Vec<StructuredTfrPoint>,
    avare_text: String,
}

pub fn sanitize_notam_id(notam_id: &str) -> String {
    notam_id.replace('/', "_")
}

pub fn avare_tfr_manifest_timestamp(generated_at_utc: DateTime<Utc>) -> String {
    generated_at_utc.format("%m_%d_%Y_%H:%M_UTC").to_string()
}

pub fn build_tfr_dataset(request: &BuildTfrRequest) -> anyhow::Result<BuildTfrResult> {
    if request.output_dir.exists() {
        fs::remove_dir_all(&request.output_dir)
            .with_context(|| format!("failed to clear {}", request.output_dir.display()))?;
    }
    fs::create_dir_all(&request.output_dir)
        .with_context(|| format!("failed to create {}", request.output_dir.display()))?;

    let (entries, parsed_areas) = load_parsed_tfr_areas(&request.input_dir)?;
    let structured_areas = parsed_areas
        .iter()
        .cloned()
        .map(|area| {
            StructuredTfrArea {
                notam_id: area.notam_id,
                area_index: area.area_index,
                schedule_fragments: area.schedule_fragments,
                upper_limit: StructuredTfrLimit {
                    value_text: area.upper_value_text,
                    unit: area.upper_unit,
                },
                lower_limit: StructuredTfrLimit {
                    value_text: area.lower_value_text,
                    unit: area.lower_unit,
                },
                polygon: area.polygon.clone(),
                avare_text: area.avare_text,
            }
        })
        .collect::<Vec<_>>();
    let structured_json_path = request.output_dir.join("tfrs.json");
    let manifest_path = request
        .output_dir
        .join(format!("tfrs_{}.manifest.json", request.version_label));
    let zip_path = request
        .output_dir
        .join(format!("tfrs_{}.zip", request.version_label));

    write_json_pretty(
        &structured_json_path,
        &StructuredTfrDataset {
            schema_version: 1,
            version_label: request.version_label.clone(),
            generated_at_utc: request.generated_at_utc.to_rfc3339(),
            notam_count: entries.len(),
            area_group_count: structured_areas.len(),
            areas: structured_areas,
        },
    )?;
    write_json_pretty(
        &manifest_path,
        &TfrManifest {
            schema_version: 1,
            version_label: request.version_label.clone(),
            generated_at_utc: request.generated_at_utc.to_rfc3339(),
            files: TfrManifestFiles {
                structured_json: "tfrs.json".to_string(),
                zip: zip_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string(),
            },
            counts: TfrManifestCounts {
                notams: entries.len(),
                area_groups: parsed_areas.len(),
            },
        },
    )?;
    write_zip(&zip_path, &[("tfrs.json", &structured_json_path)])?;

    Ok(BuildTfrResult {
        manifest_path,
        structured_json_path,
        zip_path,
        notam_count: entries.len(),
        area_group_count: parsed_areas.len(),
    })
}

pub fn build_tfr_avare_parity_artifacts(
    request: &BuildTfrRequest,
) -> anyhow::Result<BuildTfrAvareParityResult> {
    if request.output_dir.exists() {
        fs::remove_dir_all(&request.output_dir)
            .with_context(|| format!("failed to clear {}", request.output_dir.display()))?;
    }
    fs::create_dir_all(&request.output_dir)
        .with_context(|| format!("failed to create {}", request.output_dir.display()))?;

    let (_entries, parsed_areas) = load_parsed_tfr_areas(&request.input_dir)?;
    let tfr_text = parsed_areas
        .iter()
        .map(|area| area.avare_text.clone())
        .collect::<Vec<_>>()
        .join(",");
    let tfr_manifest_path = request.output_dir.join("TFRs");
    let tfr_text_path = request.output_dir.join("tfr.txt");
    fs::write(&tfr_text_path, &tfr_text)
        .with_context(|| format!("failed to write {}", tfr_text_path.display()))?;
    fs::write(
        &tfr_manifest_path,
        format!(
            "{}\ntfr.txt\n",
            avare_tfr_manifest_timestamp(request.generated_at_utc)
        ),
    )
    .with_context(|| format!("failed to write {}", tfr_manifest_path.display()))?;
    Ok(BuildTfrAvareParityResult {
        tfr_manifest_path,
        tfr_text_path,
    })
}

pub fn load_tfr_notam_ids(input_dir: &Path) -> anyhow::Result<Vec<String>> {
    Ok(load_tfr_list_entries(input_dir)?
        .into_iter()
        .map(|entry| entry.notam_id)
        .collect())
}

fn load_tfr_list_entries(input_dir: &Path) -> anyhow::Result<Vec<TfrListEntry>> {
    let path = input_dir.join("list.json");
    let bytes = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("failed to parse {}", path.display()))
}

fn load_parsed_tfr_areas(input_dir: &Path) -> anyhow::Result<(Vec<TfrListEntry>, Vec<ParsedTfrArea>)> {
    let entries = load_tfr_list_entries(input_dir)?;
    let mut parsed_areas = Vec::new();
    for entry in &entries {
        let detail_path = input_dir
            .join("details")
            .join(format!("{}.xml", sanitize_notam_id(&entry.notam_id)));
        parsed_areas.extend(parse_detail_xml_groups(&detail_path, &entry.notam_id)?);
    }
    Ok((entries, parsed_areas))
}

fn parse_detail_xml_groups(path: &Path, notam_id: &str) -> anyhow::Result<Vec<ParsedTfrArea>> {
    let xml = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut reader = Reader::from_str(&xml);
    let mut buffer = Vec::new();
    let mut groups = Vec::new();
    let mut current_group = ParsedTfrArea {
        notam_id: notam_id.to_string(),
        area_index: 0,
        schedule_fragments: Vec::new(),
        upper_value_text: String::new(),
        upper_unit: String::new(),
        lower_value_text: String::new(),
        lower_unit: String::new(),
        polygon: Vec::new(),
        avare_text: String::new(),
    };
    let mut in_area_group = false;
    let mut in_area = false;
    let mut mode = None;
    let mut pending_lat = None;

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => match event.name().as_ref() {
                b"TFRAreaGroup" => {
                    in_area_group = true;
                    current_group = ParsedTfrArea {
                        notam_id: notam_id.to_string(),
                        area_index: groups.len(),
                        schedule_fragments: Vec::new(),
                        upper_value_text: String::new(),
                        upper_unit: String::new(),
                        lower_value_text: String::new(),
                        lower_unit: String::new(),
                        polygon: Vec::new(),
                        avare_text: "TFR:: ".to_string(),
                    };
                }
                b"dateEffective" if in_area_group => mode = Some(TextMode::DateEffective),
                b"dateExpire" if in_area_group => mode = Some(TextMode::DateExpire),
                b"valDistVerUpper" if in_area_group => mode = Some(TextMode::Upper),
                b"valDistVerLower" if in_area_group => mode = Some(TextMode::Lower),
                b"uomDistVerUpper" if in_area_group => mode = Some(TextMode::UpperUnit),
                b"uomDistVerLower" if in_area_group => mode = Some(TextMode::LowerUnit),
                b"abdMergedArea" if in_area_group => {
                    in_area = true;
                    pending_lat = None;
                }
                b"geoLat" if in_area_group => mode = Some(TextMode::GeoLat),
                b"geoLong" if in_area_group => mode = Some(TextMode::GeoLon),
                _ => {}
            },
            Ok(Event::End(event)) => match event.name().as_ref() {
                b"TFRAreaGroup" => {
                    groups.push(current_group.clone());
                    in_area_group = false;
                    in_area = false;
                    mode = None;
                    pending_lat = None;
                }
                b"abdMergedArea" if in_area_group => {
                    in_area = false;
                    pending_lat = None;
                }
                b"dateEffective"
                | b"dateExpire"
                | b"valDistVerUpper"
                | b"valDistVerLower"
                | b"uomDistVerUpper"
                | b"uomDistVerLower"
                | b"geoLat"
                | b"geoLong" => mode = None,
                _ => {}
            },
            Ok(Event::Text(event)) if in_area_group => {
                let text = event
                    .xml_content()
                    .context("failed to decode TFR XML text")?
                    .trim()
                    .to_string();
                if text.is_empty() {
                    buffer.clear();
                    continue;
                }
                match mode {
                    Some(TextMode::DateEffective) => {
                        current_group.avare_text.push_str("Eff ");
                        current_group.avare_text.push_str(&text);
                        current_group.avare_text.push(' ');
                        current_group.schedule_fragments.push(StructuredTfrScheduleFragment {
                            kind: "effective".to_string(),
                            value_utc: text,
                        });
                    }
                    Some(TextMode::DateExpire) => {
                        current_group.avare_text.push_str("Exp ");
                        current_group.avare_text.push_str(&text);
                        current_group.avare_text.push(' ');
                        current_group.schedule_fragments.push(StructuredTfrScheduleFragment {
                            kind: "expires".to_string(),
                            value_utc: text,
                        });
                    }
                    Some(TextMode::Upper) => {
                        current_group.avare_text.push_str("Top ");
                        current_group.avare_text.push_str(&text);
                        current_group.avare_text.push(' ');
                        current_group.upper_value_text = text;
                    }
                    Some(TextMode::Lower) => {
                        current_group.avare_text.push_str("Low ");
                        current_group.avare_text.push_str(&text);
                        current_group.avare_text.push(' ');
                        current_group.lower_value_text = text;
                    }
                    Some(TextMode::UpperUnit) => {
                        current_group.avare_text.push_str(&text);
                        current_group.avare_text.push(' ');
                        current_group.upper_unit = text;
                    }
                    Some(TextMode::LowerUnit) => {
                        current_group.avare_text.push_str(&text);
                        current_group.avare_text.push(' ');
                        current_group.lower_unit = text;
                    }
                    Some(TextMode::GeoLat) if in_area => {
                        current_group.avare_text.push(',');
                        current_group
                            .avare_text
                            .push_str(&normalize_geo_number_string(&text)?);
                        pending_lat = Some(parse_geo_value(&text)?);
                    }
                    Some(TextMode::GeoLon) if in_area => {
                        let lon = parse_geo_value(&text)?;
                        current_group.avare_text.push(',');
                        current_group
                            .avare_text
                            .push_str(&normalize_geo_number_string(&text)?);
                        let lat = pending_lat.take().ok_or_else(|| {
                            anyhow::anyhow!("encountered geoLong before geoLat in {}", path.display())
                        })?;
                        current_group.polygon.push(StructuredTfrPoint { lat, lon });
                    }
                    _ => {}
                }
            }
            Ok(Event::CData(event)) if in_area_group => {
                let text = event
                    .xml_content()
                    .context("failed to decode TFR XML cdata")?
                    .trim()
                    .to_string();
                if text.is_empty() {
                    buffer.clear();
                    continue;
                }
                match mode {
                    Some(TextMode::GeoLat) if in_area => {
                        current_group.avare_text.push(',');
                        current_group
                            .avare_text
                            .push_str(&normalize_geo_number_string(&text)?);
                        pending_lat = Some(parse_geo_value(&text)?);
                    }
                    Some(TextMode::GeoLon) if in_area => {
                        let lon = parse_geo_value(&text)?;
                        current_group.avare_text.push(',');
                        current_group
                            .avare_text
                            .push_str(&normalize_geo_number_string(&text)?);
                        let lat = pending_lat.take().ok_or_else(|| {
                            anyhow::anyhow!("encountered geoLong before geoLat in {}", path.display())
                        })?;
                        current_group.polygon.push(StructuredTfrPoint { lat, lon });
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to parse {}", path.display()));
            }
        }
        buffer.clear();
    }

    Ok(groups)
}

fn parse_geo_value(raw: &str) -> anyhow::Result<f64> {
    let normalized = normalize_geo_number_string(raw)?;
    normalized
        .parse::<f64>()
        .with_context(|| format!("failed to parse geo value {raw}"))
}

fn normalize_geo_number_string(raw: &str) -> anyhow::Result<String> {
    let token = raw.trim();
    if token.is_empty() {
        bail!("empty geo token");
    }
    let mut negative = false;
    let mut body = token.to_string();
    if let Some(stripped) = body.strip_suffix('N') {
        body = stripped.to_string();
    } else if let Some(stripped) = body.strip_suffix('E') {
        body = stripped.to_string();
    } else if let Some(stripped) = body.strip_suffix('S') {
        body = stripped.to_string();
        negative = true;
    } else if let Some(stripped) = body.strip_suffix('W') {
        body = stripped.to_string();
        negative = true;
    }

    let (integer_raw, fractional_raw) = match body.split_once('.') {
        Some((integer, fractional)) => (integer, Some(fractional)),
        None => (body.as_str(), None),
    };
    let integer = integer_raw.trim_start_matches('0');
    let integer = if integer.is_empty() { "0" } else { integer };

    let mut normalized = integer.to_string();
    if let Some(fractional) = fractional_raw {
        let fractional = fractional.trim_end_matches('0');
        if !fractional.is_empty() {
            normalized.push('.');
            normalized.push_str(fractional);
        }
    }
    if negative && normalized != "0" {
        normalized.insert(0, '-');
    }
    Ok(normalized)
}

fn write_json_pretty(path: &Path, value: &impl Serialize) -> anyhow::Result<()> {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).context("failed to encode json")?,
    )
    .with_context(|| format!("failed to write {}", path.display()))
}

fn write_zip(path: &Path, members: &[(&str, &Path)]) -> anyhow::Result<()> {
    let file = fs::File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, source_path) in members {
        writer
            .start_file(name, options)
            .with_context(|| format!("failed to add {name} to {}", path.display()))?;
        let bytes = fs::read(source_path)
            .with_context(|| format!("failed to read {}", source_path.display()))?;
        writer
            .write_all(&bytes)
            .with_context(|| format!("failed to write {name} to {}", path.display()))?;
    }
    writer
        .finish()
        .with_context(|| format!("failed to finish {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn avare_fixture_parity() -> anyhow::Result<()> {
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("tfr_parity");
        let output_dir = TempDir::new().context("failed to create temp dir")?;
        let generated_at_utc = DateTime::parse_from_rfc3339("2026-04-15T03:30:00Z")
            .expect("valid fixture timestamp")
            .with_timezone(&Utc);
        let result = build_tfr_avare_parity_artifacts(&BuildTfrRequest {
            input_dir: fixture_root.join("input"),
            output_dir: output_dir.path().join("out"),
            version_label: "fixture".to_string(),
            generated_at_utc,
        })?;

        assert_eq!(
            fs::read_to_string(fixture_root.join("expected").join("tfr.txt"))?,
            fs::read_to_string(&result.tfr_text_path)?,
        );
        assert_eq!(
            fs::read_to_string(fixture_root.join("expected").join("TFRs"))?,
            fs::read_to_string(&result.tfr_manifest_path)?,
        );
        Ok(())
    }
}
