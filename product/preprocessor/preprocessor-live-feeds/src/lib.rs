use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use chrono::{DateTime, NaiveDateTime, Utc};
use preprocessor_zip::{write_deterministic_zip, ZipSource};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub mod engine;
pub mod notam_store;
pub mod products;
pub mod simulation;

const METAR_PRODUCT_CONTRACT_VERSION: u32 = 9;
const TAF_PRODUCT_CONTRACT_VERSION: u32 = 1;
const METAR_TREND_TOKENS: &[&str] = &["BECMG", "TEMPO", "INTER", "NOSIG", "PROB30", "PROB40"];

#[derive(Debug, Clone)]
pub struct BuildTfrRequest {
    pub input_dir: PathBuf,
    pub output_dir: PathBuf,
    pub version_label: String,
    pub generated_at_utc: DateTime<Utc>,
    pub notams_by_fdc_id: BTreeMap<String, StructuredTfrNotamMetadata>,
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
pub struct BuildMetarRequest {
    pub metar_xml_path: PathBuf,
    pub output_dir: PathBuf,
    pub version_label: String,
    pub generated_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct BuildMetarResult {
    pub manifest_path: PathBuf,
    pub structured_json_path: PathBuf,
    pub zip_path: PathBuf,
    pub metar_count: usize,
}

#[derive(Debug, Clone)]
pub struct BuildTafRequest {
    pub taf_xml_path: PathBuf,
    pub output_dir: PathBuf,
    pub version_label: String,
    pub generated_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct BuildTafResult {
    pub manifest_path: PathBuf,
    pub structured_json_path: PathBuf,
    pub zip_path: PathBuf,
    pub taf_count: usize,
}

#[derive(Debug, Clone)]
pub struct BuildNotamRequest {
    pub input_jsonl_path: PathBuf,
    pub output_dir: PathBuf,
    pub version_label: String,
    pub generated_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct BuildNotamResult {
    pub manifest_path: PathBuf,
    pub structured_json_path: PathBuf,
    pub zip_path: PathBuf,
    pub notam_count: usize,
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

#[derive(Debug, Clone, Serialize)]
struct MetarManifest {
    schema_version: u32,
    version_label: String,
    generated_at_utc: String,
    files: MetarManifestFiles,
    counts: MetarManifestCounts,
}

#[derive(Debug, Clone, Serialize)]
struct MetarManifestFiles {
    manifest: String,
    structured_json: String,
    metars: String,
    zip: String,
}

#[derive(Debug, Clone, Serialize)]
struct MetarManifestCounts {
    metars: usize,
}

#[derive(Debug, Clone, Serialize)]
struct TafManifest {
    schema_version: u32,
    version_label: String,
    generated_at_utc: String,
    files: TafManifestFiles,
    counts: TafManifestCounts,
}

#[derive(Debug, Clone, Serialize)]
struct TafManifestFiles {
    manifest: String,
    structured_json: String,
    tafs: String,
    zip: String,
}

#[derive(Debug, Clone, Serialize)]
struct TafManifestCounts {
    tafs: usize,
}

#[derive(Debug, Clone, Serialize)]
struct NotamManifest {
    schema_version: u32,
    version_label: String,
    generated_at_utc: String,
    files: NotamManifestFiles,
    counts: NotamManifestCounts,
}

#[derive(Debug, Clone, Serialize)]
struct NotamManifestFiles {
    structured_json: String,
    zip: String,
}

#[derive(Debug, Clone, Serialize)]
struct NotamManifestCounts {
    notams: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredNotamDataset {
    schema_version: u32,
    version_label: String,
    notam_count: usize,
    notams: Vec<StructuredNotamRecord>,
    notams_by_id: BTreeMap<String, StructuredNotamRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StructuredNotamRecord {
    pub id: String,
    pub jms_message_id: Option<String>,
    pub nms_id: Option<String>,
    pub correlation_id: Option<String>,
    pub source_type: Option<String>,
    pub notam_status: Option<String>,
    pub notam_function: Option<String>,
    pub notam_keyword: Option<String>,
    pub last_updated_utc: Option<String>,
    pub location_designator: Option<String>,
    pub icao_id: Option<String>,
    pub airport_name: Option<String>,
    pub airport_position: Option<StructuredPoint>,
    pub location: Option<String>,
    pub classification: Option<String>,
    pub account_id: Option<String>,
    pub xover_account_id: Option<String>,
    pub xover_notam_id: Option<String>,
    pub notam_number: Option<String>,
    pub notam_year: Option<String>,
    pub notam_type: Option<String>,
    pub issued_utc: Option<String>,
    pub effective_start_utc: Option<String>,
    pub effective_end_utc: Option<String>,
    pub text: Option<String>,
    pub local_text: Option<String>,
    pub icao_text: Option<String>,
    pub scenario: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StructuredPoint {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct CapturedNotamMessage {
    #[serde(default)]
    #[serde(rename = "jmsMessageId")]
    jms_message_id: Option<String>,
    #[serde(default)]
    properties: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    #[serde(rename = "bodyText")]
    body_text: Option<String>,
    #[serde(default)]
    #[serde(rename = "bodyUtf8")]
    body_utf8: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetarTextMode {
    RawText,
    ObservationTime,
    Latitude,
    Longitude,
    StationId,
    FlightCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TafTextMode {
    RawText,
    IssueTime,
    Latitude,
    Longitude,
    StationId,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredMetarDataset {
    schema_version: u32,
    version_label: String,
    generated_at_utc: String,
    observed_at_utc: String,
    metar_count: usize,
    metars_by_station: BTreeMap<String, StructuredMetarRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredMetarRecord {
    raw_text: String,
    observed_at_utc: String,
    station_id: String,
    flight_category: Option<String>,
    clouds: StructuredMetarClouds,
    longitude: Option<f64>,
    latitude: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredMetarClouds {
    symbol: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredTafDataset {
    schema_version: u32,
    version_label: String,
    taf_count: usize,
    tafs_by_station: BTreeMap<String, StructuredTafRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredTafRecord {
    raw_text: String,
    issued_at_utc: String,
    station_id: String,
    longitude: Option<f64>,
    latitude: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredPirepDataset {
    schema_version: u32,
    version_label: String,
    pirep_count: usize,
    pireps: Vec<StructuredPirepRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredPirepRecord {
    id: String,
    raw_text: String,
    observed_at_utc: String,
    report_type: Option<String>,
    longitude: Option<f64>,
    latitude: Option<f64>,
    symbol: String,
    icing: String,
    turbulence: String,
}

#[derive(Debug, Clone, Serialize)]
struct MetarProductModel {
    metars_by_station: BTreeMap<String, StructuredMetarRecord>,
}

#[derive(Debug, Clone, Serialize)]
struct TafProductModel {
    tafs_by_station: BTreeMap<String, StructuredTafRecord>,
}

#[derive(Debug, Clone)]
struct ParsedMetarRecord {
    raw_text: String,
    observation_time: String,
    station_id: String,
    flight_category: String,
    longitude: String,
    latitude: String,
}

#[derive(Debug, Clone)]
struct ParsedTafRecord {
    raw_text: String,
    issue_time: String,
    station_id: String,
    longitude: String,
    latitude: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredTfrDataset {
    schema_version: u32,
    version_label: String,
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
    summary_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    notam: Option<StructuredTfrNotamMetadata>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredTfrNotamMetadata {
    record_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    keyword: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    facility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    issued_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    effective_start_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    effective_end_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    icao_text: Option<String>,
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
    summary_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GeoGridPoint {
    latitude: i32,
    longitude: i32,
    geoid_height_feet: i32,
}

#[derive(Debug, Clone)]
pub struct GeoidGrid {
    geoid_height_feet_by_lat_lon: Vec<i32>,
}

impl GeoidGrid {
    const MIN_LAT: i32 = -90;
    const MAX_LAT_EXCLUSIVE: i32 = 90;
    const MIN_LON: i32 = -180;
    const MAX_LON_EXCLUSIVE: i32 = 180;
    const LON_COUNT: usize = 360;

    pub fn from_geo_csv(path: &Path) -> anyhow::Result<Self> {
        let mut values = vec![0; 180 * Self::LON_COUNT];
        let mut seen = vec![false; values.len()];
        for point in parse_geo_csv(path)? {
            if !(Self::MIN_LAT..Self::MAX_LAT_EXCLUSIVE).contains(&point.latitude) {
                bail!("geo latitude {} is outside [-90, 90)", point.latitude);
            }
            if !(Self::MIN_LON..Self::MAX_LON_EXCLUSIVE).contains(&point.longitude) {
                bail!("geo longitude {} is outside [-180, 180)", point.longitude);
            }
            let index = Self::index(point.latitude, point.longitude);
            values[index] = point.geoid_height_feet;
            seen[index] = true;
        }
        if let Some(missing) = seen.iter().position(|value| !*value) {
            let lat = missing / Self::LON_COUNT;
            let lon = missing % Self::LON_COUNT;
            bail!(
                "geo grid is missing latitude {}, longitude {}",
                lat as i32 + Self::MIN_LAT,
                lon as i32 + Self::MIN_LON
            );
        }
        Ok(Self {
            geoid_height_feet_by_lat_lon: values,
        })
    }

    pub fn geoid_height_feet_bilinear(&self, latitude: f64, longitude: f64) -> f64 {
        let lat = latitude.clamp(
            f64::from(Self::MIN_LAT),
            f64::from(Self::MAX_LAT_EXCLUSIVE - 1),
        );
        let lon = normalize_longitude(longitude);
        let lat0 = lat.floor() as i32;
        let lat1 = (lat0 + 1).min(Self::MAX_LAT_EXCLUSIVE - 1);
        let lon0 = lon.floor() as i32;
        let lon1 = wrap_longitude(lon0 + 1);
        let lat_t = lat - f64::from(lat0);
        let lon_t = lon - f64::from(lon0);

        let sw = f64::from(self.value(lat0, lon0));
        let se = f64::from(self.value(lat0, lon1));
        let nw = f64::from(self.value(lat1, lon0));
        let ne = f64::from(self.value(lat1, lon1));
        let south = sw * (1.0 - lon_t) + se * lon_t;
        let north = nw * (1.0 - lon_t) + ne * lon_t;
        south * (1.0 - lat_t) + north * lat_t
    }

    fn value(&self, latitude: i32, longitude: i32) -> i32 {
        self.geoid_height_feet_by_lat_lon[Self::index(latitude, longitude)]
    }

    fn index(latitude: i32, longitude: i32) -> usize {
        ((latitude - Self::MIN_LAT) as usize) * Self::LON_COUNT
            + (longitude - Self::MIN_LON) as usize
    }
}

pub fn terrain_ellipsoid_height_feet_from_navd88_meters(
    navd88_height_meters: f64,
    latitude: f64,
    longitude: f64,
    geoid_grid: &GeoidGrid,
) -> f64 {
    navd88_height_meters * 3.280_839_895
        + geoid_grid.geoid_height_feet_bilinear(latitude, longitude)
}

pub fn sanitize_notam_id(notam_id: &str) -> String {
    notam_id.replace('/', "_")
}

pub fn tfr_notam_metadata_by_fdc_id(
    records: &[StructuredNotamRecord],
) -> BTreeMap<String, StructuredTfrNotamMetadata> {
    let mut by_fdc_id = BTreeMap::new();
    for record in records {
        let metadata = structured_tfr_notam_metadata(record);
        for fdc_id in fdc_ids_for_notam(record) {
            match by_fdc_id.get(&fdc_id) {
                Some(existing)
                    if tfr_notam_metadata_rank(existing) >= tfr_notam_metadata_rank(&metadata) => {}
                _ => {
                    by_fdc_id.insert(fdc_id, metadata.clone());
                }
            }
        }
    }
    by_fdc_id
}

fn structured_tfr_notam_metadata(record: &StructuredNotamRecord) -> StructuredTfrNotamMetadata {
    StructuredTfrNotamMetadata {
        record_id: record.id.clone(),
        source_type: record.source_type.clone(),
        status: record.notam_status.clone(),
        function: record.notam_function.clone(),
        keyword: record.notam_keyword.clone(),
        facility: record
            .location_designator
            .clone()
            .or_else(|| record.icao_id.clone())
            .or_else(|| record.location.clone()),
        issued_utc: record.issued_utc.clone(),
        effective_start_utc: record.effective_start_utc.clone(),
        effective_end_utc: record.effective_end_utc.clone(),
        text: record.text.clone(),
        local_text: record.local_text.clone(),
        icao_text: record.icao_text.clone(),
    }
}

fn tfr_notam_metadata_rank(metadata: &StructuredTfrNotamMetadata) -> u8 {
    let source_rank = metadata
        .source_type
        .as_deref()
        .is_some_and(|source| source.eq_ignore_ascii_case("F")) as u8;
    let keyword_rank = metadata
        .keyword
        .as_deref()
        .is_some_and(|keyword| keyword.eq_ignore_ascii_case("AIRSPACE"))
        as u8;
    let text_rank = metadata
        .text
        .as_deref()
        .is_some_and(|text| text_contains_tfr(text)) as u8;
    source_rank + keyword_rank + text_rank
}

fn fdc_ids_for_notam(record: &StructuredNotamRecord) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    if record
        .source_type
        .as_deref()
        .is_some_and(|source| source.eq_ignore_ascii_case("F"))
    {
        if let (Some(year), Some(number)) = (&record.notam_year, &record.notam_number) {
            if let Some(id) = fdc_id_from_year_and_number(year, number) {
                ids.insert(id);
            }
        }
    }
    for text in [&record.text, &record.local_text, &record.icao_text]
        .into_iter()
        .flatten()
    {
        ids.extend(fdc_ids_in_text(text));
    }
    ids
}

fn fdc_id_from_year_and_number(year: &str, number: &str) -> Option<String> {
    let year_digit = year.trim().chars().rev().find(|ch| ch.is_ascii_digit())?;
    let number = number.trim();
    if number.is_empty() || !number.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let number = number.parse::<u32>().ok()?;
    Some(format!("{year_digit}/{number:04}"))
}

fn fdc_ids_in_text(text: &str) -> BTreeSet<String> {
    let chars = text.chars().collect::<Vec<_>>();
    let mut ids = BTreeSet::new();
    for index in 0..chars.len().saturating_sub(5) {
        if !chars[index].is_ascii_digit() || chars.get(index + 1) != Some(&'/') {
            continue;
        }
        let Some(number) = chars.get(index + 2..index + 6) else {
            continue;
        };
        if !number.iter().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        let before_ok = index == 0 || !chars[index - 1].is_ascii_alphanumeric();
        let after_ok = chars
            .get(index + 6)
            .map(|ch| !ch.is_ascii_alphanumeric())
            .unwrap_or(true);
        if before_ok && after_ok {
            ids.insert(chars[index..index + 6].iter().collect::<String>());
        }
    }
    ids
}

fn text_contains_tfr(text: &str) -> bool {
    let text = text.to_ascii_uppercase();
    text.contains("TEMPORARY FLIGHT RESTRICTIONS")
        || text.contains(" TFR")
        || text.contains("91.137")
        || text.contains("91.138")
        || text.contains("91.139")
        || text.contains("91.141")
        || text.contains("91.143")
        || text.contains("91.145")
}

fn enriched_tfr_limits(
    area: &ParsedTfrArea,
    notam_text: Option<&str>,
) -> (StructuredTfrLimit, StructuredTfrLimit) {
    let mut lower_limit = StructuredTfrLimit {
        value_text: area.lower_value_text.clone(),
        unit: area.lower_unit.clone(),
    };
    let mut upper_limit = StructuredTfrLimit {
        value_text: area.upper_value_text.clone(),
        unit: area.upper_unit.clone(),
    };
    if lower_limit.value_text.trim().is_empty() || upper_limit.value_text.trim().is_empty() {
        if let Some((notam_lower, notam_upper)) = notam_text.and_then(tfr_altitude_limits_from_text)
        {
            if lower_limit.value_text.trim().is_empty() {
                lower_limit = notam_lower;
            }
            if upper_limit.value_text.trim().is_empty() {
                upper_limit = notam_upper;
            }
        }
    }
    (lower_limit, upper_limit)
}

fn tfr_notam_altitude_text(metadata: &StructuredTfrNotamMetadata) -> Option<&str> {
    metadata
        .local_text
        .as_deref()
        .or(metadata.text.as_deref())
        .or(metadata.icao_text.as_deref())
}

fn tfr_altitude_limits_from_text(text: &str) -> Option<(StructuredTfrLimit, StructuredTfrLimit)> {
    text.split_whitespace()
        .filter_map(parse_tfr_altitude_pair_token)
        .next()
}

fn parse_tfr_altitude_pair_token(token: &str) -> Option<(StructuredTfrLimit, StructuredTfrLimit)> {
    let token = token
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-')
        .to_ascii_uppercase();
    let (lower, upper) = token.split_once('-')?;
    if lower.is_empty() || upper.is_empty() {
        return None;
    }
    Some((
        parse_tfr_altitude_limit(lower)?,
        parse_tfr_altitude_limit(upper)?,
    ))
}

fn parse_tfr_altitude_limit(value: &str) -> Option<StructuredTfrLimit> {
    let value = value
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric())
        .to_ascii_uppercase();
    if value == "SFC" {
        return Some(StructuredTfrLimit {
            value_text: "SFC".to_string(),
            unit: String::new(),
        });
    }
    if let Some(number) = value.strip_suffix("FT") {
        if !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit()) {
            return Some(StructuredTfrLimit {
                value_text: number.to_string(),
                unit: "FT".to_string(),
            });
        }
    }
    if !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit()) {
        return Some(StructuredTfrLimit {
            value_text: value,
            unit: String::new(),
        });
    }
    None
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
            let notam = request.notams_by_fdc_id.get(&area.notam_id).cloned();
            let (lower_limit, upper_limit) =
                enriched_tfr_limits(&area, notam.as_ref().and_then(tfr_notam_altitude_text));
            StructuredTfrArea {
                notam_id: area.notam_id,
                area_index: area.area_index,
                schedule_fragments: area.schedule_fragments,
                upper_limit,
                lower_limit,
                polygon: area.polygon.clone(),
                summary_text: area.summary_text,
                notam,
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

pub fn build_metar_dataset(request: &BuildMetarRequest) -> anyhow::Result<BuildMetarResult> {
    if request.output_dir.exists() {
        fs::remove_dir_all(&request.output_dir)
            .with_context(|| format!("failed to clear {}", request.output_dir.display()))?;
    }
    fs::create_dir_all(&request.output_dir)
        .with_context(|| format!("failed to create {}", request.output_dir.display()))?;

    let model = metar_product_model(&request.metar_xml_path)?;
    let content_timestamp = metar_product_content_timestamp(&model, request.generated_at_utc)?;
    let content_timestamp_text = content_timestamp.to_rfc3339();
    let metar_count = model.metars_by_station.len();

    let structured_json_path = request.output_dir.join("metars.json");
    let canonical_manifest_path = request.output_dir.join("manifest.json");
    let manifest_path = request
        .output_dir
        .join(format!("metars_{}.manifest.json", request.version_label));
    let zip_path = request
        .output_dir
        .join(format!("metars_{}.zip", request.version_label));

    write_json_pretty(
        &structured_json_path,
        &StructuredMetarDataset {
            schema_version: 4,
            version_label: request.version_label.clone(),
            generated_at_utc: content_timestamp_text.clone(),
            observed_at_utc: content_timestamp_text.clone(),
            metar_count,
            metars_by_station: model.metars_by_station.clone(),
        },
    )?;
    let manifest = MetarManifest {
        schema_version: METAR_PRODUCT_CONTRACT_VERSION,
        version_label: request.version_label.clone(),
        generated_at_utc: content_timestamp_text,
        files: MetarManifestFiles {
            manifest: "manifest.json".to_string(),
            structured_json: "metars.json".to_string(),
            metars: "metars.json".to_string(),
            zip: zip_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string(),
        },
        counts: MetarManifestCounts {
            metars: metar_count,
        },
    };
    write_json_pretty(&canonical_manifest_path, &manifest)?;
    write_json_pretty(&manifest_path, &manifest)?;
    write_zip(
        &zip_path,
        &[
            ("metars.json", &structured_json_path),
            ("manifest.json", &canonical_manifest_path),
        ],
    )?;

    Ok(BuildMetarResult {
        manifest_path,
        structured_json_path,
        zip_path,
        metar_count,
    })
}

pub fn build_taf_dataset(request: &BuildTafRequest) -> anyhow::Result<BuildTafResult> {
    if request.output_dir.exists() {
        fs::remove_dir_all(&request.output_dir)
            .with_context(|| format!("failed to clear {}", request.output_dir.display()))?;
    }
    fs::create_dir_all(&request.output_dir)
        .with_context(|| format!("failed to create {}", request.output_dir.display()))?;

    let model = taf_product_model(&request.taf_xml_path)?;
    let content_timestamp = taf_product_content_timestamp(&model, request.generated_at_utc)?;
    let taf_count = model.tafs_by_station.len();
    let structured_json_path = request.output_dir.join("tafs.json");
    let canonical_manifest_path = request.output_dir.join("manifest.json");
    let manifest_path = request
        .output_dir
        .join(format!("tafs_{}.manifest.json", request.version_label));
    let zip_path = request
        .output_dir
        .join(format!("tafs_{}.zip", request.version_label));

    write_json_pretty(
        &structured_json_path,
        &StructuredTafDataset {
            schema_version: 1,
            version_label: request.version_label.clone(),
            taf_count,
            tafs_by_station: model.tafs_by_station.clone(),
        },
    )?;
    let manifest = TafManifest {
        schema_version: TAF_PRODUCT_CONTRACT_VERSION,
        version_label: request.version_label.clone(),
        generated_at_utc: content_timestamp.to_rfc3339(),
        files: TafManifestFiles {
            manifest: "manifest.json".to_string(),
            structured_json: "tafs.json".to_string(),
            tafs: "tafs.json".to_string(),
            zip: zip_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string(),
        },
        counts: TafManifestCounts { tafs: taf_count },
    };
    write_json_pretty(&canonical_manifest_path, &manifest)?;
    write_json_pretty(&manifest_path, &manifest)?;
    write_zip(
        &zip_path,
        &[
            ("tafs.json", &structured_json_path),
            ("manifest.json", &canonical_manifest_path),
        ],
    )?;

    Ok(BuildTafResult {
        manifest_path,
        structured_json_path,
        zip_path,
        taf_count,
    })
}

pub fn metar_content_fingerprint(metar_xml_path: &Path) -> anyhow::Result<String> {
    let model = metar_product_model(metar_xml_path)?;
    let bytes = serde_json::to_vec(&(METAR_PRODUCT_CONTRACT_VERSION, model))
        .context("failed to encode canonical METAR model")?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

pub fn taf_content_fingerprint(taf_xml_path: &Path) -> anyhow::Result<String> {
    let model = taf_product_model(taf_xml_path)?;
    let bytes = serde_json::to_vec(&(TAF_PRODUCT_CONTRACT_VERSION, model))
        .context("failed to encode canonical TAF model")?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

fn metar_product_model(metar_xml_path: &Path) -> anyhow::Result<MetarProductModel> {
    let mut metars_by_station = BTreeMap::new();
    for mut record in structured_metar_records(metar_xml_path)? {
        let station_id = record.station_id.trim().to_ascii_uppercase();
        if station_id.is_empty() {
            continue;
        }
        record.station_id = station_id.clone();
        let replace_existing = metars_by_station
            .get(&station_id)
            .map(|existing: &StructuredMetarRecord| {
                (record.observed_at_utc.as_str(), record.raw_text.as_str())
                    >= (
                        existing.observed_at_utc.as_str(),
                        existing.raw_text.as_str(),
                    )
            })
            .unwrap_or(true);
        if replace_existing {
            metars_by_station.insert(station_id, record);
        }
    }
    Ok(MetarProductModel { metars_by_station })
}

fn taf_product_model(taf_xml_path: &Path) -> anyhow::Result<TafProductModel> {
    let mut tafs_by_station = BTreeMap::new();
    for mut record in structured_taf_records(taf_xml_path)? {
        let station_id = record.station_id.trim().to_ascii_uppercase();
        if station_id.is_empty() {
            continue;
        }
        record.station_id = station_id.clone();
        let replace_existing = tafs_by_station
            .get(&station_id)
            .map(|existing: &StructuredTafRecord| {
                (record.issued_at_utc.as_str(), record.raw_text.as_str())
                    >= (existing.issued_at_utc.as_str(), existing.raw_text.as_str())
            })
            .unwrap_or(true);
        if replace_existing {
            tafs_by_station.insert(station_id, record);
        }
    }
    Ok(TafProductModel { tafs_by_station })
}

fn metar_product_content_timestamp(
    model: &MetarProductModel,
    fallback: DateTime<Utc>,
) -> anyhow::Result<DateTime<Utc>> {
    let mut latest = None;
    for value in model
        .metars_by_station
        .values()
        .map(|record| record.observed_at_utc.as_str())
    {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let parsed = DateTime::parse_from_rfc3339(value)
            .with_context(|| format!("failed to parse METAR source timestamp {value:?}"))?
            .with_timezone(&Utc);
        if latest.is_none_or(|current| parsed > current) {
            latest = Some(parsed);
        }
    }
    Ok(latest.unwrap_or(fallback))
}

fn taf_product_content_timestamp(
    model: &TafProductModel,
    fallback: DateTime<Utc>,
) -> anyhow::Result<DateTime<Utc>> {
    let mut latest = None;
    for value in model
        .tafs_by_station
        .values()
        .map(|record| record.issued_at_utc.as_str())
    {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let parsed = DateTime::parse_from_rfc3339(value)
            .with_context(|| format!("failed to parse TAF source timestamp {value:?}"))?
            .with_timezone(&Utc);
        if latest.is_none_or(|current| parsed > current) {
            latest = Some(parsed);
        }
    }
    Ok(latest.unwrap_or(fallback))
}

fn structured_metar_records(input_xml_path: &Path) -> anyhow::Result<Vec<StructuredMetarRecord>> {
    let mut records = parse_metar_records(input_xml_path)?;
    records.sort_by(|left, right| {
        (
            &left.station_id,
            &left.observation_time,
            &left.raw_text,
            &left.flight_category,
            &left.longitude,
            &left.latitude,
        )
            .cmp(&(
                &right.station_id,
                &right.observation_time,
                &right.raw_text,
                &right.flight_category,
                &right.longitude,
                &right.latitude,
            ))
    });
    records
        .into_iter()
        .map(|record| -> anyhow::Result<StructuredMetarRecord> {
            Ok(StructuredMetarRecord {
                clouds: structured_metar_clouds(&record.raw_text),
                raw_text: record.raw_text,
                observed_at_utc: record.observation_time,
                station_id: record.station_id,
                flight_category: empty_to_none(record.flight_category),
                longitude: parse_optional_f64(&record.longitude)?,
                latitude: parse_optional_f64(&record.latitude)?,
            })
        })
        .collect()
}

fn structured_taf_records(input_xml_path: &Path) -> anyhow::Result<Vec<StructuredTafRecord>> {
    let mut records = parse_taf_records(input_xml_path)?;
    records.sort_by(|left, right| {
        (
            &left.station_id,
            &left.issue_time,
            &left.raw_text,
            &left.longitude,
            &left.latitude,
        )
            .cmp(&(
                &right.station_id,
                &right.issue_time,
                &right.raw_text,
                &right.longitude,
                &right.latitude,
            ))
    });
    records
        .into_iter()
        .map(|record| -> anyhow::Result<StructuredTafRecord> {
            Ok(StructuredTafRecord {
                raw_text: record.raw_text,
                issued_at_utc: record.issue_time,
                station_id: record.station_id,
                longitude: parse_optional_f64(&record.longitude)?,
                latitude: parse_optional_f64(&record.latitude)?,
            })
        })
        .collect()
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PirepHazardSeverity {
    None,
    Unknown,
    Light,
    Moderate,
    Severe,
}

#[cfg(test)]
impl PirepHazardSeverity {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Unknown => "unknown",
            Self::Light => "light",
            Self::Moderate => "moderate",
            Self::Severe => "severe",
        }
    }

    fn actionable(self) -> bool {
        matches!(self, Self::Light | Self::Moderate | Self::Severe)
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PirepHazards {
    icing: PirepHazardSeverity,
    turbulence: PirepHazardSeverity,
}

#[cfg(test)]
impl PirepHazards {
    fn symbol(self) -> &'static str {
        match (self.icing.actionable(), self.turbulence.actionable()) {
            (false, false) => "generic",
            (true, false) => pirep_symbol(self.icing, "icing"),
            (false, true) => pirep_symbol(self.turbulence, "turbulence"),
            (true, true) => {
                if self.icing >= self.turbulence {
                    pirep_symbol(self.icing, "icing")
                } else {
                    pirep_symbol(self.turbulence, "turbulence")
                }
            }
        }
    }
}

#[cfg(test)]
fn pirep_symbol(severity: PirepHazardSeverity, hazard: &'static str) -> &'static str {
    match (severity, hazard) {
        (PirepHazardSeverity::Light, "icing") => "light-icing",
        (PirepHazardSeverity::Moderate, "icing") => "moderate-icing",
        (PirepHazardSeverity::Severe, "icing") => "severe-icing",
        (PirepHazardSeverity::Light, "turbulence") => "light-turbulence",
        (PirepHazardSeverity::Moderate, "turbulence") => "moderate-turbulence",
        (PirepHazardSeverity::Severe, "turbulence") => "severe-turbulence",
        _ => "generic",
    }
}

#[cfg(test)]
fn parse_pirep_hazards(raw_text: &str) -> PirepHazards {
    let sections = pirep_hazard_sections(raw_text);
    let mut icing = PirepHazardSeverity::None;
    let mut turbulence = PirepHazardSeverity::None;

    for section in sections {
        let tokens = pirep_hazard_tokens(&section);
        if tokens.is_empty() {
            continue;
        }
        let section_hazards = pirep_section_hazards(&tokens);
        icing = icing.max(section_hazards.icing);
        turbulence = turbulence.max(section_hazards.turbulence);
    }

    PirepHazards { icing, turbulence }
}

#[cfg(test)]
fn pirep_hazard_sections(raw_text: &str) -> Vec<String> {
    let upper = raw_text.to_ascii_uppercase();
    let slash_sections = upper
        .split('/')
        .map(str::trim)
        .filter(|section| !section.is_empty());
    if slash_sections.clone().count() > 1 {
        slash_sections.map(str::to_string).collect()
    } else {
        vec![upper]
    }
}

#[cfg(test)]
fn pirep_hazard_tokens(section: &str) -> Vec<String> {
    section
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PirepHazardKind {
    Icing,
    Turbulence,
}

#[cfg(test)]
fn pirep_section_hazards(tokens: &[String]) -> PirepHazards {
    let anchors = tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| pirep_hazard_kind(token).map(|kind| (index, kind)))
        .collect::<Vec<_>>();
    let mut icing = PirepHazardSeverity::None;
    let mut turbulence = PirepHazardSeverity::None;

    for (anchor_index, (token_index, kind)) in anchors.iter().enumerate() {
        let next_hazard_index = anchors
            .get(anchor_index + 1)
            .map(|(index, _)| *index)
            .unwrap_or(tokens.len());
        let mut span_end = next_hazard_index;
        if let Some(relative_index) = tokens[token_index + 1..span_end]
            .iter()
            .position(|token| matches!(token.as_str(), "RM" | "RMK"))
        {
            span_end = token_index + 1 + relative_index;
        }

        let mut severity = pirep_hazard_severity(&tokens[token_index + 1..span_end]);
        if severity == PirepHazardSeverity::Unknown && *token_index > 0 {
            severity = pirep_hazard_severity(&tokens[token_index - 1..*token_index]);
        }

        match kind {
            PirepHazardKind::Icing => icing = icing.max(severity),
            PirepHazardKind::Turbulence => turbulence = turbulence.max(severity),
        }
    }

    PirepHazards { icing, turbulence }
}

#[cfg(test)]
fn pirep_hazard_kind(token: &str) -> Option<PirepHazardKind> {
    match token {
        "IC" | "ICE" | "ICING" | "RIME" | "CLRICE" | "MXD" | "MXDICE" => {
            Some(PirepHazardKind::Icing)
        }
        "TB" | "TURB" | "TURBC" | "TURBULENCE" | "CHOP" => Some(PirepHazardKind::Turbulence),
        _ => None,
    }
}

#[cfg(test)]
fn pirep_hazard_severity(tokens: &[String]) -> PirepHazardSeverity {
    let mut saw_negative = false;
    let mut best = PirepHazardSeverity::Unknown;
    for token in tokens {
        match token.as_str() {
            "NEG" | "NONE" | "NO" | "NIL" => saw_negative = true,
            "LGT" | "LIGHT" => best = best.max(PirepHazardSeverity::Light),
            "MOD" | "MODERATE" => best = best.max(PirepHazardSeverity::Moderate),
            "SEV" | "SEVERE" | "EXTREME" | "EXTRM" => best = best.max(PirepHazardSeverity::Severe),
            _ => {}
        }
    }
    if best.actionable() {
        best
    } else if saw_negative {
        PirepHazardSeverity::None
    } else {
        PirepHazardSeverity::Unknown
    }
}

fn structured_metar_clouds(raw_text: &str) -> StructuredMetarClouds {
    StructuredMetarClouds {
        symbol: metar_cloud_symbol(raw_text).map(str::to_string),
    }
}

fn metar_cloud_symbol(raw_text: &str) -> Option<&'static str> {
    let observation = raw_text
        .split_once(" RMK ")
        .map(|(observation, _)| observation)
        .unwrap_or(raw_text);
    let mut layers = Vec::new();
    for token in observation.split_whitespace() {
        if METAR_TREND_TOKENS.contains(&token) {
            break;
        }
        match parse_metar_cloud_token(token) {
            MetarCloudToken::None => {}
            MetarCloudToken::Immediate(symbol) => return Some(symbol),
            MetarCloudToken::Layer { amount, height_ft } => {
                layers.push((amount, height_ft));
            }
        }
    }
    choose_metar_cloud_symbol(&layers)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetarCloudToken {
    None,
    Immediate(&'static str),
    Layer {
        amount: &'static str,
        height_ft: Option<u32>,
    },
}

fn parse_metar_cloud_token(token: &str) -> MetarCloudToken {
    match token {
        "CAVOK" | "SKC" | "CLR" | "NCD" => return MetarCloudToken::Immediate("SKC"),
        "NSC" => return MetarCloudToken::Immediate("NSC"),
        _ => {}
    }
    if token.starts_with("VV") && parse_metar_cloud_height(&token[2..]).is_some() {
        return MetarCloudToken::Immediate("VV");
    }
    for amount in ["FEW", "SCT", "BKN", "OVC"] {
        if let Some(rest) = token.strip_prefix(amount) {
            return match parse_metar_cloud_height(rest) {
                Some(height_ft) => MetarCloudToken::Layer { amount, height_ft },
                None => MetarCloudToken::None,
            };
        }
    }
    MetarCloudToken::None
}

fn parse_metar_cloud_height(rest: &str) -> Option<Option<u32>> {
    if rest.starts_with("///") {
        return Some(None);
    }
    let digit_len = rest
        .as_bytes()
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .take(4)
        .count();
    if digit_len < 3 {
        return None;
    }
    let suffix = &rest[digit_len..];
    if !(suffix.is_empty() || suffix == "CB" || suffix == "TCU" || suffix == "///") {
        return None;
    }
    rest[..digit_len]
        .parse::<u32>()
        .ok()
        .map(|hundreds_ft| Some(hundreds_ft * 100))
}

fn choose_metar_cloud_symbol(layers: &[(&'static str, Option<u32>)]) -> Option<&'static str> {
    layers
        .iter()
        .filter(|(amount, _)| *amount == "BKN" || *amount == "OVC")
        .min_by_key(|(_, height_ft)| height_ft.unwrap_or(u32::MAX))
        .or_else(|| {
            layers
                .iter()
                .min_by_key(|(_, height_ft)| height_ft.unwrap_or(u32::MAX))
        })
        .map(|(amount, _)| *amount)
}

pub fn build_notam_dataset(request: &BuildNotamRequest) -> anyhow::Result<BuildNotamResult> {
    if request.output_dir.exists() {
        fs::remove_dir_all(&request.output_dir)
            .with_context(|| format!("failed to clear {}", request.output_dir.display()))?;
    }
    fs::create_dir_all(&request.output_dir)
        .with_context(|| format!("failed to create {}", request.output_dir.display()))?;

    let notams_by_id = structured_notam_records(&request.input_jsonl_path)?
        .into_iter()
        .map(|record| (record.id.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let notams = notams_by_id.values().cloned().collect::<Vec<_>>();
    let structured_json_path = request.output_dir.join("notams.json");
    let manifest_path = request
        .output_dir
        .join(format!("notams_{}.manifest.json", request.version_label));
    let zip_path = request
        .output_dir
        .join(format!("notams_{}.zip", request.version_label));

    write_json_pretty(
        &structured_json_path,
        &StructuredNotamDataset {
            schema_version: 1,
            version_label: request.version_label.clone(),
            notam_count: notams.len(),
            notams: notams.clone(),
            notams_by_id: notams_by_id.clone(),
        },
    )?;
    write_json_pretty(
        &manifest_path,
        &NotamManifest {
            schema_version: 1,
            version_label: request.version_label.clone(),
            generated_at_utc: request.generated_at_utc.to_rfc3339(),
            files: NotamManifestFiles {
                structured_json: "notams.json".to_string(),
                zip: zip_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string(),
            },
            counts: NotamManifestCounts {
                notams: notams.len(),
            },
        },
    )?;
    write_zip(&zip_path, &[("notams.json", &structured_json_path)])?;

    Ok(BuildNotamResult {
        manifest_path,
        structured_json_path,
        zip_path,
        notam_count: notams.len(),
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

fn parse_geo_csv(path: &Path) -> anyhow::Result<Vec<GeoGridPoint>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    text.lines()
        .enumerate()
        .map(|(index, line)| {
            let columns = line.split(',').collect::<Vec<_>>();
            if columns.len() != 4 {
                bail!(
                    "expected 4 geo columns at {}:{}, got {}",
                    path.display(),
                    index + 1,
                    columns.len()
                );
            }
            Ok(GeoGridPoint {
                latitude: parse_geo_i32(path, index, "latitude", columns[0])?,
                longitude: parse_geo_i32(path, index, "longitude", columns[1])?,
                geoid_height_feet: parse_geo_i32(path, index, "geoid height", columns[2])?,
            })
        })
        .collect()
}

pub fn structured_notam_records(
    input_jsonl_path: &Path,
) -> anyhow::Result<Vec<StructuredNotamRecord>> {
    let mut records = load_json_lines::<CapturedNotamMessage>(input_jsonl_path)?
        .into_iter()
        .filter_map(|message| normalize_captured_notam(message).transpose())
        .collect::<anyhow::Result<Vec<_>>>()?;
    records.sort_by(|left, right| {
        (
            &left.icao_id,
            &left.location_designator,
            &left.notam_year,
            &left.notam_number,
            &left.id,
        )
            .cmp(&(
                &right.icao_id,
                &right.location_designator,
                &right.notam_year,
                &right.notam_number,
                &right.id,
            ))
    });
    Ok(records)
}

fn normalize_captured_notam(
    message: CapturedNotamMessage,
) -> anyhow::Result<Option<StructuredNotamRecord>> {
    let body = message
        .body_text
        .or(message.body_utf8)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let Some(body) = body else {
        return Ok(None);
    };

    let xml = roxmltree::Document::parse(&body).context("failed to parse NOTAM XML body")?;
    let notam_node = match xml
        .descendants()
        .find(|node| node.tag_name().name() == "NOTAM")
    {
        Some(node) => node,
        None => return Ok(None),
    };
    let event_time_slice = xml
        .descendants()
        .find(|node| node.tag_name().name() == "EventTimeSlice");

    let local_text = find_translation_text(&notam_node, "LOCAL_FORMAT");
    let icao_text = find_translation_text(&notam_node, "OTHER:ICAO");
    let location_designator = property_string(
        &message.properties,
        "us_gov_dot_faa_aim_fns_nds_LocationDesignator",
    );
    let icao_id = property_string(&message.properties, "us_gov_dot_faa_aim_fns_nds_ICAOId");
    let source_type = property_string(&message.properties, "us_gov_dot_faa_aim_fns_nds_SourceType");
    let notam_number_prop = property_string(
        &message.properties,
        "us_gov_dot_faa_aim_fns_nds_NOTAMNumber",
    );
    let notam_number = child_text(&notam_node, "number").or(notam_number_prop);
    let notam_year = child_text(&notam_node, "year");
    let notam_type = child_text(&notam_node, "type");
    let location = child_text(&notam_node, "location");
    let account_id = find_first_text(&xml, "accountId");
    let xover_notam_id = find_first_text(&xml, "xovernotamID");
    let id = [
        source_type.clone(),
        location
            .clone()
            .or(location_designator.clone())
            .or(icao_id.clone()),
        notam_year.clone(),
        notam_type.clone(),
        notam_number.clone(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(":");
    let airport_position = find_airport_position(&xml)?;

    Ok(Some(StructuredNotamRecord {
        id,
        jms_message_id: message.jms_message_id,
        nms_id: property_string(&message.properties, "m_msg_nms_id"),
        correlation_id: property_string(
            &message.properties,
            "us_gov_dot_faa_aim_fns_nds_CorrelationID",
        ),
        source_type,
        notam_status: property_string(
            &message.properties,
            "us_gov_dot_faa_aim_fns_nds_NOTAMStatus",
        ),
        notam_function: property_string(
            &message.properties,
            "us_gov_dot_faa_aim_fns_nds_NOTAMFunction",
        ),
        notam_keyword: property_string(
            &message.properties,
            "us_gov_dot_faa_aim_fns_nds_NOTAMKeyword",
        ),
        last_updated_utc: property_string(&message.properties, "m_msg_last_updated"),
        location_designator,
        icao_id,
        airport_name: find_first_text(&xml, "airportname").or(find_first_text(&xml, "name")),
        airport_position,
        location,
        classification: find_first_text(&xml, "classification"),
        account_id,
        xover_account_id: find_first_text(&xml, "xoveraccountID"),
        xover_notam_id,
        notam_number,
        notam_year,
        notam_type,
        issued_utc: child_text(&notam_node, "issued"),
        effective_start_utc: child_text(&notam_node, "effectiveStart")
            .and_then(|value| parse_compact_notam_timestamp(&value).ok()),
        effective_end_utc: child_text(&notam_node, "effectiveEnd")
            .and_then(|value| parse_compact_notam_timestamp(&value).ok()),
        text: child_text(&notam_node, "text"),
        local_text,
        icao_text,
        scenario: event_time_slice.and_then(|node| child_text(&node, "scenario")),
    }))
}

fn load_json_lines<T: for<'de> Deserialize<'de>>(path: &Path) -> anyhow::Result<Vec<T>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    if let Ok(records) = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str::<T>(line).with_context(|| {
                format!("failed to parse line {} of {}", index + 1, path.display())
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()
    {
        return Ok(records);
    }
    let mut records = Vec::new();
    let mut stream = serde_json::Deserializer::from_str(&text).into_iter::<T>();
    while let Some(value) = stream.next() {
        records.push(
            value.with_context(|| {
                format!("failed to parse concatenated JSON in {}", path.display())
            })?,
        );
    }
    Ok(records)
}

fn property_string(
    properties: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    properties.get(key).and_then(|value| match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    })
}

fn child_text(node: &roxmltree::Node<'_, '_>, local_name: &str) -> Option<String> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == local_name)
        .and_then(text_content)
}

fn find_first_text(document: &roxmltree::Document<'_>, local_name: &str) -> Option<String> {
    document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == local_name)
        .and_then(text_content)
}

fn text_content(node: roxmltree::Node<'_, '_>) -> Option<String> {
    if let Some(text) = node.text() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    let text = node
        .descendants()
        .filter(|descendant| descendant.is_text())
        .filter_map(|descendant| descendant.text())
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn find_translation_text(
    notam_node: &roxmltree::Node<'_, '_>,
    translation_type: &str,
) -> Option<String> {
    notam_node
        .children()
        .filter(|child| child.is_element() && child.tag_name().name() == "translation")
        .find_map(|translation| {
            let translation_node = translation
                .descendants()
                .find(|node| node.is_element() && node.tag_name().name() == "NOTAMTranslation")?;
            let translation_kind = child_text(&translation_node, "type")?;
            if translation_kind != translation_type {
                return None;
            }
            child_text(&translation_node, "simpleText")
                .or_else(|| child_text(&translation_node, "formattedText"))
        })
}

fn parse_compact_notam_timestamp(value: &str) -> anyhow::Result<String> {
    let parsed = NaiveDateTime::parse_from_str(value, "%Y%m%d%H%M")
        .with_context(|| format!("failed to parse NOTAM timestamp {value}"))?;
    Ok(parsed.and_utc().to_rfc3339())
}

fn find_airport_position(
    document: &roxmltree::Document<'_>,
) -> anyhow::Result<Option<StructuredPoint>> {
    let pos_node = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "AirportHeliport")
        .and_then(|airport| {
            airport
                .descendants()
                .find(|node| node.is_element() && node.tag_name().name() == "pos")
        });
    let pos_text = pos_node
        .and_then(|node| node.text().map(str::trim).map(ToOwned::to_owned))
        .filter(|text| !text.is_empty());
    let Some(pos_text) = pos_text else {
        return Ok(None);
    };
    let parts = pos_text.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 2 {
        bail!("unexpected airport position format {pos_text}");
    }
    Ok(Some(StructuredPoint {
        lat: parts[0]
            .parse::<f64>()
            .with_context(|| format!("failed to parse airport latitude from {pos_text}"))?,
        lon: parts[1]
            .parse::<f64>()
            .with_context(|| format!("failed to parse airport longitude from {pos_text}"))?,
    }))
}

fn parse_geo_i32(
    path: &Path,
    zero_based_line: usize,
    field: &str,
    raw: &str,
) -> anyhow::Result<i32> {
    raw.parse::<i32>().with_context(|| {
        format!(
            "failed to parse {field} as integer at {}:{}",
            path.display(),
            zero_based_line + 1
        )
    })
}

fn normalize_longitude(longitude: f64) -> f64 {
    let mut lon = longitude;
    while lon < -180.0 {
        lon += 360.0;
    }
    while lon >= 180.0 {
        lon -= 360.0;
    }
    lon
}

fn wrap_longitude(longitude: i32) -> i32 {
    if longitude >= 180 {
        longitude - 360
    } else if longitude < -180 {
        longitude + 360
    } else {
        longitude
    }
}

fn parse_metar_records(path: &Path) -> anyhow::Result<Vec<ParsedMetarRecord>> {
    let xml =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut reader = Reader::from_str(&xml);
    let mut buffer = Vec::new();
    let mut records = Vec::new();
    let mut in_metar = false;
    let mut mode = None;
    let mut current = ParsedMetarRecord {
        raw_text: String::new(),
        observation_time: String::new(),
        station_id: String::new(),
        flight_category: String::new(),
        longitude: String::new(),
        latitude: String::new(),
    };

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => match event.name().as_ref() {
                b"METAR" => {
                    in_metar = true;
                    current = ParsedMetarRecord {
                        raw_text: String::new(),
                        observation_time: String::new(),
                        station_id: String::new(),
                        flight_category: String::new(),
                        longitude: String::new(),
                        latitude: String::new(),
                    };
                    mode = None;
                }
                b"raw_text" if in_metar => mode = Some(MetarTextMode::RawText),
                b"observation_time" if in_metar => mode = Some(MetarTextMode::ObservationTime),
                b"latitude" if in_metar => mode = Some(MetarTextMode::Latitude),
                b"longitude" if in_metar => mode = Some(MetarTextMode::Longitude),
                b"station_id" if in_metar => mode = Some(MetarTextMode::StationId),
                b"flight_category" if in_metar => mode = Some(MetarTextMode::FlightCategory),
                _ => {}
            },
            Ok(Event::End(event)) => match event.name().as_ref() {
                b"METAR" if in_metar => {
                    if !current.raw_text.is_empty() && !current.station_id.is_empty() {
                        records.push(current.clone());
                    }
                    in_metar = false;
                    mode = None;
                }
                b"raw_text" | b"observation_time" | b"latitude" | b"longitude" | b"station_id"
                | b"flight_category" => mode = None,
                _ => {}
            },
            Ok(Event::Text(event)) if in_metar => {
                let text = event
                    .xml_content()
                    .context("failed to decode METAR XML text")?
                    .into_owned();
                push_metar_text(&mut current, mode, &text);
            }
            Ok(Event::CData(event)) if in_metar => {
                let text = event
                    .xml_content()
                    .context("failed to decode METAR XML cdata")?
                    .into_owned();
                push_metar_text(&mut current, mode, &text);
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => {
                return Err(error).with_context(|| format!("failed to parse {}", path.display()));
            }
        }
        buffer.clear();
    }

    Ok(records)
}

fn push_metar_text(current: &mut ParsedMetarRecord, mode: Option<MetarTextMode>, text: &str) {
    match mode {
        Some(MetarTextMode::RawText) => current.raw_text.push_str(text),
        Some(MetarTextMode::ObservationTime) => current.observation_time.push_str(text),
        Some(MetarTextMode::Latitude) => current.latitude.push_str(text),
        Some(MetarTextMode::Longitude) => current.longitude.push_str(text),
        Some(MetarTextMode::StationId) => current.station_id.push_str(text),
        Some(MetarTextMode::FlightCategory) => current.flight_category.push_str(text),
        None => {}
    }
}

fn parse_taf_records(path: &Path) -> anyhow::Result<Vec<ParsedTafRecord>> {
    let xml =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut reader = Reader::from_str(&xml);
    let mut buffer = Vec::new();
    let mut records = Vec::new();
    let mut in_taf = false;
    let mut mode = None;
    let mut current = ParsedTafRecord {
        raw_text: String::new(),
        issue_time: String::new(),
        station_id: String::new(),
        longitude: String::new(),
        latitude: String::new(),
    };

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => match event.name().as_ref() {
                b"TAF" => {
                    in_taf = true;
                    current = ParsedTafRecord {
                        raw_text: String::new(),
                        issue_time: String::new(),
                        station_id: String::new(),
                        longitude: String::new(),
                        latitude: String::new(),
                    };
                    mode = None;
                }
                b"raw_text" if in_taf => mode = Some(TafTextMode::RawText),
                b"issue_time" if in_taf => mode = Some(TafTextMode::IssueTime),
                b"latitude" if in_taf => mode = Some(TafTextMode::Latitude),
                b"longitude" if in_taf => mode = Some(TafTextMode::Longitude),
                b"station_id" if in_taf => mode = Some(TafTextMode::StationId),
                _ => {}
            },
            Ok(Event::End(event)) => match event.name().as_ref() {
                b"TAF" if in_taf => {
                    if !current.raw_text.is_empty() && !current.station_id.is_empty() {
                        records.push(current.clone());
                    }
                    in_taf = false;
                    mode = None;
                }
                b"raw_text" | b"issue_time" | b"latitude" | b"longitude" | b"station_id" => {
                    mode = None
                }
                _ => {}
            },
            Ok(Event::Text(event)) if in_taf => {
                let text = event
                    .xml_content()
                    .context("failed to decode TAF XML text")?
                    .into_owned();
                push_taf_text(&mut current, mode, &text);
            }
            Ok(Event::CData(event)) if in_taf => {
                let text = event
                    .xml_content()
                    .context("failed to decode TAF XML cdata")?
                    .into_owned();
                push_taf_text(&mut current, mode, &text);
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => {
                return Err(error).with_context(|| format!("failed to parse {}", path.display()));
            }
        }
        buffer.clear();
    }

    Ok(records)
}

fn push_taf_text(current: &mut ParsedTafRecord, mode: Option<TafTextMode>, text: &str) {
    match mode {
        Some(TafTextMode::RawText) => current.raw_text.push_str(text),
        Some(TafTextMode::IssueTime) => current.issue_time.push_str(text),
        Some(TafTextMode::Latitude) => current.latitude.push_str(text),
        Some(TafTextMode::Longitude) => current.longitude.push_str(text),
        Some(TafTextMode::StationId) => current.station_id.push_str(text),
        None => {}
    }
}

fn empty_to_none(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_optional_f64(value: &str) -> anyhow::Result<Option<f64>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(trimmed.parse::<f64>().with_context(|| {
        format!("failed to parse float {trimmed}")
    })?))
}

fn load_parsed_tfr_areas(
    input_dir: &Path,
) -> anyhow::Result<(Vec<TfrListEntry>, Vec<ParsedTfrArea>)> {
    let entries = load_tfr_list_entries(input_dir)?;
    let graphics_path = input_dir.join("graphics.geojson");
    if graphics_path.is_file() {
        return Ok((entries, parse_wfs_geojson_areas(&graphics_path)?));
    }
    let mut parsed_areas = Vec::new();
    for entry in &entries {
        let detail_path = input_dir
            .join("details")
            .join(format!("{}.xml", sanitize_notam_id(&entry.notam_id)));
        parsed_areas.extend(parse_detail_xml_groups(&detail_path, &entry.notam_id)?);
    }
    Ok((entries, parsed_areas))
}

fn parse_wfs_geojson_areas(path: &Path) -> anyhow::Result<Vec<ParsedTfrArea>> {
    let value: Value = serde_json::from_slice(
        &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))?;
    let features = value
        .get("features")
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow::anyhow!("{} did not contain GeoJSON features", path.display()))?;
    let mut areas = Vec::new();
    for feature in features {
        let properties = feature.get("properties").unwrap_or(&Value::Null);
        let notam_id = geojson_notam_id(feature, properties).ok_or_else(|| {
            anyhow::anyhow!("TFR GeoJSON feature missing NOTAM id in {}", path.display())
        })?;
        let mut feature_area_index = 0;
        for polygon in geojson_exterior_polygons(feature.get("geometry").unwrap_or(&Value::Null))? {
            let polygon = polygon
                .into_iter()
                .map(geojson_lon_lat_point)
                .collect::<anyhow::Result<Vec<_>>>()?
                .into_iter()
                .map(|(lon, lat)| StructuredTfrPoint { lat, lon })
                .collect::<Vec<_>>();
            if polygon.len() < 3 {
                continue;
            }
            let schedule_fragments = tfr_schedule_fragments_from_properties(properties);
            let upper_value_text = geojson_property_string(
                properties,
                &[
                    "valDistVerUpper",
                    "VAL_DIST_VER_UPPER",
                    "upper_limit",
                    "UPPER_LIMIT",
                    "upper",
                    "UPPER",
                ],
            )
            .unwrap_or_default();
            let upper_unit = geojson_property_string(
                properties,
                &[
                    "uomDistVerUpper",
                    "UOM_DIST_VER_UPPER",
                    "upper_unit",
                    "UPPER_UNIT",
                ],
            )
            .unwrap_or_default();
            let lower_value_text = geojson_property_string(
                properties,
                &[
                    "valDistVerLower",
                    "VAL_DIST_VER_LOWER",
                    "lower_limit",
                    "LOWER_LIMIT",
                    "lower",
                    "LOWER",
                ],
            )
            .unwrap_or_default();
            let lower_unit = geojson_property_string(
                properties,
                &[
                    "uomDistVerLower",
                    "UOM_DIST_VER_LOWER",
                    "lower_unit",
                    "LOWER_UNIT",
                ],
            )
            .unwrap_or_default();
            areas.push(ParsedTfrArea {
                notam_id: notam_id.clone(),
                area_index: feature_area_index,
                summary_text: build_tfr_summary_text(
                    &schedule_fragments,
                    &upper_value_text,
                    &upper_unit,
                    &lower_value_text,
                    &lower_unit,
                    &polygon,
                )?,
                schedule_fragments,
                upper_value_text,
                upper_unit,
                lower_value_text,
                lower_unit,
                polygon,
            });
            feature_area_index += 1;
        }
    }
    Ok(areas)
}

fn geojson_notam_id(feature: &Value, properties: &Value) -> Option<String> {
    geojson_property_string(
        properties,
        &[
            "notam_id",
            "NOTAM_ID",
            "notamId",
            "NOTAMID",
            "NOTAM_KEY",
            "NOTAM",
            "notam",
            "id",
            "ID",
        ],
    )
    .map(normalize_geojson_notam_id)
    .or_else(|| {
        feature
            .get("id")
            .and_then(value_to_string)
            .map(normalize_geojson_notam_id)
    })
}

fn normalize_geojson_notam_id(value: String) -> String {
    let without_feature_prefix = value
        .rsplit_once('.')
        .map(|(_, suffix)| suffix.to_string())
        .unwrap_or(value);
    without_feature_prefix
        .split_once('-')
        .map(|(notam_id, _)| notam_id.to_string())
        .unwrap_or(without_feature_prefix)
}

fn tfr_schedule_fragments_from_properties(
    properties: &Value,
) -> Vec<StructuredTfrScheduleFragment> {
    let mut fragments = Vec::new();
    if let Some(value_utc) = geojson_property_string(
        properties,
        &[
            "dateEffective",
            "DATE_EFFECTIVE",
            "effective_date",
            "EFFECTIVE_DATE",
            "effective",
            "EFFECTIVE",
        ],
    ) {
        fragments.push(StructuredTfrScheduleFragment {
            kind: "effective".to_string(),
            value_utc,
        });
    }
    if let Some(value_utc) = geojson_property_string(
        properties,
        &[
            "dateExpire",
            "DATE_EXPIRE",
            "expiration_date",
            "EXPIRATION_DATE",
            "expire",
            "EXPIRE",
        ],
    ) {
        fragments.push(StructuredTfrScheduleFragment {
            kind: "expires".to_string(),
            value_utc,
        });
    }
    fragments
}

fn geojson_property_string(properties: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| properties.get(*key).and_then(value_to_string))
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
    .filter(|value| !value.trim().is_empty())
}

fn geojson_exterior_polygons(geometry: &Value) -> anyhow::Result<Vec<Vec<Value>>> {
    let geometry_type = geometry
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let coordinates = geometry.get("coordinates").unwrap_or(&Value::Null);
    match geometry_type {
        "Polygon" => Ok(coordinates
            .as_array()
            .and_then(|rings| rings.first())
            .and_then(|ring| ring.as_array())
            .map(|ring| vec![ring.clone()])
            .unwrap_or_default()),
        "MultiPolygon" => Ok(coordinates
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|polygon| {
                polygon
                    .as_array()
                    .and_then(|rings| rings.first())
                    .and_then(|ring| ring.as_array())
                    .cloned()
            })
            .collect()),
        other => bail!("unsupported TFR GeoJSON geometry type {other}"),
    }
}

fn geojson_lon_lat_point(value: Value) -> anyhow::Result<(f64, f64)> {
    let coordinates = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("TFR GeoJSON coordinate was not an array"))?;
    let lon = coordinates
        .first()
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow::anyhow!("TFR GeoJSON coordinate missing longitude"))?;
    let lat = coordinates
        .get(1)
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow::anyhow!("TFR GeoJSON coordinate missing latitude"))?;
    if !(-180.0..=180.0).contains(&lon) || !(-90.0..=90.0).contains(&lat) {
        bail!("TFR GeoJSON coordinate is not lon/lat: [{lon}, {lat}]");
    }
    Ok((lon, lat))
}

fn build_tfr_summary_text(
    schedule_fragments: &[StructuredTfrScheduleFragment],
    upper_value_text: &str,
    upper_unit: &str,
    lower_value_text: &str,
    lower_unit: &str,
    polygon: &[StructuredTfrPoint],
) -> anyhow::Result<String> {
    let mut text = String::from("TFR:: ");
    if !upper_value_text.is_empty() {
        text.push_str("Top ");
        text.push_str(upper_value_text);
        text.push(' ');
        text.push_str(upper_unit);
        text.push(' ');
    }
    if !lower_value_text.is_empty() {
        text.push_str("Low ");
        text.push_str(lower_value_text);
        text.push(' ');
        text.push_str(lower_unit);
        text.push(' ');
    }
    for fragment in schedule_fragments {
        match fragment.kind.as_str() {
            "effective" => text.push_str("Eff "),
            "expires" => text.push_str("Exp "),
            _ => {}
        }
        text.push_str(&fragment.value_utc);
        text.push(' ');
    }
    for point in polygon {
        text.push(',');
        text.push_str(&normalize_geo_number_string(&point.lat.to_string())?);
        text.push(',');
        text.push_str(&normalize_geo_number_string(&point.lon.to_string())?);
    }
    Ok(text)
}

fn parse_detail_xml_groups(path: &Path, notam_id: &str) -> anyhow::Result<Vec<ParsedTfrArea>> {
    let xml =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
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
        summary_text: String::new(),
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
                        summary_text: "TFR:: ".to_string(),
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
                b"dateEffective" | b"dateExpire" | b"valDistVerUpper" | b"valDistVerLower"
                | b"uomDistVerUpper" | b"uomDistVerLower" | b"geoLat" | b"geoLong" => mode = None,
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
                        current_group.summary_text.push_str("Eff ");
                        current_group.summary_text.push_str(&text);
                        current_group.summary_text.push(' ');
                        current_group
                            .schedule_fragments
                            .push(StructuredTfrScheduleFragment {
                                kind: "effective".to_string(),
                                value_utc: text,
                            });
                    }
                    Some(TextMode::DateExpire) => {
                        current_group.summary_text.push_str("Exp ");
                        current_group.summary_text.push_str(&text);
                        current_group.summary_text.push(' ');
                        current_group
                            .schedule_fragments
                            .push(StructuredTfrScheduleFragment {
                                kind: "expires".to_string(),
                                value_utc: text,
                            });
                    }
                    Some(TextMode::Upper) => {
                        current_group.summary_text.push_str("Top ");
                        current_group.summary_text.push_str(&text);
                        current_group.summary_text.push(' ');
                        current_group.upper_value_text = text;
                    }
                    Some(TextMode::Lower) => {
                        current_group.summary_text.push_str("Low ");
                        current_group.summary_text.push_str(&text);
                        current_group.summary_text.push(' ');
                        current_group.lower_value_text = text;
                    }
                    Some(TextMode::UpperUnit) => {
                        current_group.summary_text.push_str(&text);
                        current_group.summary_text.push(' ');
                        current_group.upper_unit = text;
                    }
                    Some(TextMode::LowerUnit) => {
                        current_group.summary_text.push_str(&text);
                        current_group.summary_text.push(' ');
                        current_group.lower_unit = text;
                    }
                    Some(TextMode::GeoLat) if in_area => {
                        current_group.summary_text.push(',');
                        current_group
                            .summary_text
                            .push_str(&normalize_geo_number_string(&text)?);
                        pending_lat = Some(parse_geo_value(&text)?);
                    }
                    Some(TextMode::GeoLon) if in_area => {
                        let lon = parse_geo_value(&text)?;
                        current_group.summary_text.push(',');
                        current_group
                            .summary_text
                            .push_str(&normalize_geo_number_string(&text)?);
                        let lat = pending_lat.take().ok_or_else(|| {
                            anyhow::anyhow!(
                                "encountered geoLong before geoLat in {}",
                                path.display()
                            )
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
                        current_group.summary_text.push(',');
                        current_group
                            .summary_text
                            .push_str(&normalize_geo_number_string(&text)?);
                        pending_lat = Some(parse_geo_value(&text)?);
                    }
                    Some(TextMode::GeoLon) if in_area => {
                        let lon = parse_geo_value(&text)?;
                        current_group.summary_text.push(',');
                        current_group
                            .summary_text
                            .push_str(&normalize_geo_number_string(&text)?);
                        let lat = pending_lat.take().ok_or_else(|| {
                            anyhow::anyhow!(
                                "encountered geoLong before geoLat in {}",
                                path.display()
                            )
                        })?;
                        current_group.polygon.push(StructuredTfrPoint { lat, lon });
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => {
                return Err(error).with_context(|| format!("failed to parse {}", path.display()));
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
    let members = members
        .iter()
        .map(|(member_name, source_path)| ZipSource::new(*member_name, *source_path))
        .collect::<Vec<_>>();
    write_deterministic_zip(path, &members)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn geo_grid_fixture_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("preprocessor-live-feeds crate should live under product/preprocessor")
            .join("test_fixtures")
            .join("geo_grid")
            .join("geo.csv")
    }

    #[test]
    fn tfr_dataset_prefers_wfs_geojson_when_present() -> anyhow::Result<()> {
        let temp = TempDir::new().context("failed to create temp dir")?;
        let input_dir = temp.path().join("input");
        fs::create_dir_all(&input_dir)?;
        fs::write(
            input_dir.join("list.json"),
            r#"[{"notam_id":"6/7042","type":"VIP","facility":"ZAB","state":"NM","description":"fixture","creation_date":"04/30/2026"}]"#,
        )?;
        fs::write(
            input_dir.join("graphics.geojson"),
            r#"{
  "type": "FeatureCollection",
  "features": [{
    "type": "Feature",
    "id": "V_TFR_LOC.6/7042",
    "properties": {
      "dateEffective": "2026-04-30T00:00:00Z",
      "dateExpire": "2026-05-01T00:00:00Z",
      "valDistVerUpper": "5000",
      "uomDistVerUpper": "FT",
      "valDistVerLower": "SFC",
      "uomDistVerLower": ""
    },
    "geometry": {
      "type": "Polygon",
      "coordinates": [[
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 0.0]
      ]]
    }
  }]
}"#,
        )?;
        let generated_at_utc = DateTime::parse_from_rfc3339("2026-04-30T00:00:00Z")
            .expect("valid fixture timestamp")
            .with_timezone(&Utc);
        let result = build_tfr_dataset(&BuildTfrRequest {
            input_dir,
            output_dir: temp.path().join("out"),
            version_label: "fixture".to_string(),
            generated_at_utc,
            notams_by_fdc_id: BTreeMap::new(),
        })?;

        let dataset: Value = serde_json::from_slice(&fs::read(&result.structured_json_path)?)?;
        assert_eq!(1, dataset["notam_count"]);
        assert_eq!(1, dataset["area_group_count"]);
        let area = &dataset["areas"][0];
        assert_eq!("6/7042", area["notam_id"]);
        assert_eq!(1.0, area["polygon"][1]["lon"]);
        assert_eq!(1.0, area["polygon"][2]["lat"]);
        assert_eq!("5000", area["upper_limit"]["value_text"]);
        assert_eq!("FT", area["upper_limit"]["unit"]);
        Ok(())
    }

    #[test]
    fn tfr_dataset_enriches_matching_fdc_notam_metadata() -> anyhow::Result<()> {
        let temp = TempDir::new().context("failed to create temp dir")?;
        let input_dir = temp.path().join("input");
        fs::create_dir_all(&input_dir)?;
        fs::write(input_dir.join("list.json"), r#"[{"notam_id":"6/7042"}]"#)?;
        fs::write(
            input_dir.join("graphics.geojson"),
            r#"{
  "type": "FeatureCollection",
  "features": [{
    "type": "Feature",
    "id": "V_TFR_LOC.6/7042",
    "properties": {},
    "geometry": {
      "type": "Polygon",
      "coordinates": [[
        [-120.0, 47.0],
        [-119.9, 47.0],
        [-119.9, 47.1],
        [-120.0, 47.0]
      ]]
    }
  }]
}"#,
        )?;
        let generated_at_utc = DateTime::parse_from_rfc3339("2026-04-30T00:00:00Z")
            .expect("valid fixture timestamp")
            .with_timezone(&Utc);
        let result = build_tfr_dataset(&BuildTfrRequest {
            input_dir,
            output_dir: temp.path().join("out"),
            version_label: "fixture".to_string(),
            generated_at_utc,
            notams_by_fdc_id: tfr_notam_metadata_by_fdc_id(&[tfr_notam_record(
                    "F:ZSE:2026:N:7042",
                    "ZSE",
                    "7042",
                    "!FDC 6/7042 ZSE WA..AIRSPACE TEST..TEMPORARY FLIGHT RESTRICTIONS. WI AN AREA SFC-8500FT TEST FIRE.",
                )]),
        })?;

        let dataset: Value = serde_json::from_slice(&fs::read(&result.structured_json_path)?)?;
        let area = &dataset["areas"][0];
        assert_eq!("SFC", area["lower_limit"]["value_text"]);
        assert_eq!("8500", area["upper_limit"]["value_text"]);
        assert_eq!("FT", area["upper_limit"]["unit"]);
        assert_eq!("F:ZSE:2026:N:7042", area["notam"]["record_id"]);
        assert_eq!("ZSE", area["notam"]["facility"]);
        assert_eq!("PUBLISHED", area["notam"]["status"]);
        assert!(area["notam"]["text"]
            .as_str()
            .expect("notam text")
            .contains("TEMPORARY FLIGHT RESTRICTIONS"));
        Ok(())
    }

    #[test]
    fn tfr_wfs_geojson_rejects_projected_coordinates() -> anyhow::Result<()> {
        let temp = TempDir::new().context("failed to create temp dir")?;
        let input_dir = temp.path().join("input");
        fs::create_dir_all(&input_dir)?;
        fs::write(input_dir.join("list.json"), r#"[{"notam_id":"6/7042"}]"#)?;
        fs::write(
            input_dir.join("graphics.geojson"),
            r#"{
  "type": "FeatureCollection",
  "features": [{
    "type": "Feature",
    "id": "V_TFR_LOC.6/7042",
    "properties": {},
    "geometry": {
      "type": "Polygon",
      "coordinates": [[[111319.49079327357, 0.0], [1.0, 0.0], [1.0, 1.0]]]
    }
  }]
}"#,
        )?;
        let generated_at_utc = DateTime::parse_from_rfc3339("2026-04-30T00:00:00Z")
            .expect("valid fixture timestamp")
            .with_timezone(&Utc);
        let error = build_tfr_dataset(&BuildTfrRequest {
            input_dir,
            output_dir: temp.path().join("out"),
            version_label: "fixture".to_string(),
            generated_at_utc,
            notams_by_fdc_id: BTreeMap::new(),
        })
        .expect_err("projected coordinates should fail loudly");
        assert!(format!("{error:#}").contains("not lon/lat"));
        Ok(())
    }

    fn tfr_notam_record(
        id: &str,
        facility: &str,
        number: &str,
        text: &str,
    ) -> StructuredNotamRecord {
        StructuredNotamRecord {
            id: id.to_string(),
            jms_message_id: None,
            nms_id: None,
            correlation_id: None,
            source_type: Some("F".to_string()),
            notam_status: Some("PUBLISHED".to_string()),
            notam_function: Some("N".to_string()),
            notam_keyword: Some("AIRSPACE".to_string()),
            last_updated_utc: None,
            location_designator: Some(facility.to_string()),
            icao_id: None,
            airport_name: None,
            airport_position: None,
            location: Some(facility.to_string()),
            classification: None,
            account_id: None,
            xover_account_id: None,
            xover_notam_id: None,
            notam_number: Some(number.to_string()),
            notam_year: Some("2026".to_string()),
            notam_type: Some("N".to_string()),
            issued_utc: Some("2026-04-30T00:00:00Z".to_string()),
            effective_start_utc: Some("2026-04-30T00:00:00Z".to_string()),
            effective_end_utc: Some("2026-05-01T00:00:00Z".to_string()),
            text: Some(text.to_string()),
            local_text: Some(text.to_string()),
            icao_text: None,
            scenario: None,
        }
    }

    #[test]
    fn metar_dataset_is_stable_across_source_reordering() -> anyhow::Result<()> {
        let temp = TempDir::new().context("failed to create temp dir")?;
        let first = temp.path().join("first.xml");
        let second = temp.path().join("second.xml");
        let taf_first = temp.path().join("taf-first.xml");
        let taf_second = temp.path().join("taf-second.xml");
        let record_a = r#"<METAR><raw_text>METAR KAAA 160400Z 00000KT 10SM CLR 10/08 A3000</raw_text><station_id>KAAA</station_id><observation_time>2026-04-16T04:00:00.000Z</observation_time><latitude>1.0</latitude><longitude>2.0</longitude><flight_category>VFR</flight_category></METAR>"#;
        let record_b = r#"<METAR><raw_text>METAR KBBB 160355Z 18005KT 10SM SCT020 BKN025 12/09 A3001</raw_text><station_id>KBBB</station_id><observation_time>2026-04-16T03:55:00.000Z</observation_time><latitude>3.0</latitude><longitude>4.0</longitude><flight_category>MVFR</flight_category></METAR>"#;
        let taf_a = r#"<TAF><raw_text>TAF KAAA 160400Z 1604/1704 P6SM SKC</raw_text><station_id>KAAA</station_id><issue_time>2026-04-16T04:00:00.000Z</issue_time><latitude>1.0</latitude><longitude>2.0</longitude></TAF>"#;
        let taf_b = r#"<TAF><raw_text>TAF KBBB 160355Z 1604/1704 P6SM BKN025</raw_text><station_id>KBBB</station_id><issue_time>2026-04-16T03:55:00.000Z</issue_time><latitude>3.0</latitude><longitude>4.0</longitude></TAF>"#;
        fs::write(
            &first,
            format!(
                r#"<?xml version="1.0"?><response><data>{record_b}{record_a}</data></response>"#
            ),
        )?;
        fs::write(
            &second,
            format!(
                r#"<?xml version="1.0"?><response><data>{record_a}{record_b}</data></response>"#
            ),
        )?;
        fs::write(
            &taf_first,
            format!(r#"<?xml version="1.0"?><response><data>{taf_b}{taf_a}</data></response>"#),
        )?;
        fs::write(
            &taf_second,
            format!(r#"<?xml version="1.0"?><response><data>{taf_a}{taf_b}</data></response>"#),
        )?;

        let first_fingerprint = metar_content_fingerprint(&first)?;
        let second_fingerprint = metar_content_fingerprint(&second)?;
        assert_eq!(first_fingerprint, second_fingerprint);
        let version_label = first_fingerprint.chars().take(16).collect::<String>();
        let first_taf_fingerprint = taf_content_fingerprint(&taf_first)?;
        let second_taf_fingerprint = taf_content_fingerprint(&taf_second)?;
        assert_eq!(first_taf_fingerprint, second_taf_fingerprint);
        let taf_version_label = first_taf_fingerprint.chars().take(16).collect::<String>();
        let generated_at_utc = DateTime::parse_from_rfc3339("2026-04-16T04:00:00Z")
            .expect("valid fixture timestamp")
            .with_timezone(&Utc);
        let later_generated_at_utc = DateTime::parse_from_rfc3339("2026-04-16T04:05:00Z")
            .expect("valid fixture timestamp")
            .with_timezone(&Utc);
        let first_result = build_metar_dataset(&BuildMetarRequest {
            metar_xml_path: first,
            output_dir: temp.path().join("first-out"),
            version_label: version_label.clone(),
            generated_at_utc,
        })?;
        let second_result = build_metar_dataset(&BuildMetarRequest {
            metar_xml_path: second,
            output_dir: temp.path().join("second-out"),
            version_label,
            generated_at_utc: later_generated_at_utc,
        })?;
        let first_taf_result = build_taf_dataset(&BuildTafRequest {
            taf_xml_path: taf_first,
            output_dir: temp.path().join("first-taf-out"),
            version_label: taf_version_label.clone(),
            generated_at_utc,
        })?;
        let second_taf_result = build_taf_dataset(&BuildTafRequest {
            taf_xml_path: taf_second,
            output_dir: temp.path().join("second-taf-out"),
            version_label: taf_version_label,
            generated_at_utc: later_generated_at_utc,
        })?;

        assert_eq!(
            fs::read(&first_result.structured_json_path)?,
            fs::read(&second_result.structured_json_path)?,
        );
        assert_eq!(
            fs::read(&first_result.zip_path)?,
            fs::read(&second_result.zip_path)?,
        );
        let dataset: Value =
            serde_json::from_slice(&fs::read(&first_result.structured_json_path)?)?;
        let metars_by_station = dataset
            .get("metars_by_station")
            .and_then(|value| value.as_object())
            .context("METAR dataset should be keyed by station")?;
        assert!(metars_by_station.contains_key("KAAA"));
        assert!(metars_by_station.contains_key("KBBB"));
        assert!(dataset.get("metars").is_none());
        assert_eq!(
            dataset.get("generated_at_utc"),
            Some(&Value::String("2026-04-16T04:00:00+00:00".to_string())),
        );
        assert_eq!(
            dataset.get("observed_at_utc"),
            Some(&Value::String("2026-04-16T04:00:00+00:00".to_string())),
        );
        assert_eq!(
            dataset.pointer("/metars_by_station/KAAA/observed_at_utc"),
            Some(&Value::String("2026-04-16T04:00:00.000Z".to_string())),
        );
        assert_eq!(
            dataset.pointer("/metars_by_station/KBBB/flight_category"),
            Some(&Value::String("MVFR".to_string())),
        );
        assert_eq!(
            dataset.pointer("/metars_by_station/KAAA/clouds/symbol"),
            Some(&Value::String("SKC".to_string())),
        );
        assert_eq!(
            dataset.pointer("/metars_by_station/KBBB/clouds/symbol"),
            Some(&Value::String("BKN".to_string())),
        );
        assert!(dataset
            .pointer("/metars_by_station/KBBB/clouds/ceiling")
            .is_none());
        assert!(dataset.get("important_station_ids").is_none());

        let manifest: Value = serde_json::from_slice(&fs::read(&first_result.manifest_path)?)?;
        assert!(first_result
            .zip_path
            .parent()
            .unwrap()
            .join("manifest.json")
            .is_file());
        assert_eq!(
            manifest
                .pointer("/files/manifest")
                .and_then(|value| value.as_str()),
            Some("manifest.json"),
        );
        assert_eq!(
            manifest
                .pointer("/files/metars")
                .and_then(|value| value.as_str()),
            Some("metars.json"),
        );
        assert!(manifest.pointer("/files/tafs").is_none());
        assert!(manifest.pointer("/files/pireps").is_none());
        assert!(manifest.get("map_view").is_none());
        assert!(manifest.pointer("/counts/tile_count").is_none());
        assert!(manifest
            .pointer("/counts/max_wx_records_per_tile")
            .is_none());
        assert!(manifest.pointer("/counts/important_metars").is_none());
        assert_eq!(
            manifest
                .pointer("/counts/metars")
                .and_then(|value| value.as_u64()),
            Some(2),
        );

        assert_eq!(
            fs::read(&first_taf_result.structured_json_path)?,
            fs::read(&second_taf_result.structured_json_path)?,
        );
        assert_eq!(
            fs::read(&first_taf_result.zip_path)?,
            fs::read(&second_taf_result.zip_path)?,
        );
        let tafs: Value = serde_json::from_slice(&fs::read(
            first_taf_result
                .zip_path
                .parent()
                .unwrap()
                .join("tafs.json"),
        )?)?;
        assert_eq!(
            tafs.pointer("/taf_count").and_then(|value| value.as_u64()),
            Some(2)
        );
        assert_eq!(
            tafs.pointer("/tafs_by_station/KAAA/issued_at_utc")
                .and_then(|value| value.as_str()),
            Some("2026-04-16T04:00:00.000Z"),
        );
        let taf_manifest: Value =
            serde_json::from_slice(&fs::read(&first_taf_result.manifest_path)?)?;
        assert_eq!(
            taf_manifest
                .pointer("/files/tafs")
                .and_then(|value| value.as_str()),
            Some("tafs.json"),
        );
        assert_eq!(
            taf_manifest
                .pointer("/counts/tafs")
                .and_then(|value| value.as_u64()),
            Some(2),
        );

        let zip_listing = Command::new("unzip")
            .arg("-Z1")
            .arg(&first_result.zip_path)
            .output()
            .context("failed to list METAR package zip")?;
        assert!(zip_listing.status.success());
        let zip_listing = String::from_utf8(zip_listing.stdout)?;
        assert!(zip_listing.lines().any(|line| line == "manifest.json"));
        assert!(!zip_listing.lines().any(|line| line == "tafs.json"));
        assert!(!zip_listing.lines().any(|line| line == "pireps.json"));
        assert!(!zip_listing
            .lines()
            .any(|line| line.starts_with("points/wx/")));
        Ok(())
    }

    #[test]
    fn pirep_hazard_parser_computes_render_symbol_and_detail_fields() {
        for (raw_text, symbol, icing, turbulence) in [
            (
                "DUT UA /OV DUT/TM 0443/FL020/TP SB20/SK BASE020 TOP050/TB NEG/IC NEG/RM DURC ZAN",
                "generic",
                "none",
                "none",
            ),
            (
                "ABC UA /OV ABC/TM 0443/FL080/IC LGT/TB NEG",
                "light-icing",
                "light",
                "none",
            ),
            (
                "ABC UA /OV ABC/TM 0443/FL080/IC LGT/RM TB MODERATE",
                "moderate-turbulence",
                "light",
                "moderate",
            ),
            (
                "ABC UA /OV ABC/TM 0443/FL080/IC MOD/TB SEV",
                "severe-turbulence",
                "moderate",
                "severe",
            ),
            (
                "ABC UA /OV ABC/TM 0443/FL080/IC MOD/TB MOD",
                "moderate-icing",
                "moderate",
                "moderate",
            ),
            (
                "ABC UA /OV ABC/TM 0443/FL080/IC RIME/TB CHOP",
                "generic",
                "unknown",
                "unknown",
            ),
            (
                "ABC UA /OV ABC/TM 0443/FL080/IC NEG OCNL LGT/TB NONE",
                "light-icing",
                "light",
                "none",
            ),
            (
                "ARP SWR7C 4306N 05511W 2242 F390 MS54 253/130KT TB OCNL SEV CAT IC RM A333",
                "severe-turbulence",
                "unknown",
                "severe",
            ),
        ] {
            let hazards = parse_pirep_hazards(raw_text);
            assert_eq!(hazards.symbol(), symbol, "{raw_text}");
            assert_eq!(hazards.icing.as_str(), icing, "{raw_text}");
            assert_eq!(hazards.turbulence.as_str(), turbulence, "{raw_text}");
        }
    }

    #[test]
    fn metar_cloud_symbol_handles_speci_prefix() {
        let clouds = structured_metar_clouds(
            "SPECI KBOK 031917Z AUTO 19004KT 3/4SM BR OVC002 11/11 A2990 RMK AO2 RAE1857",
        );
        assert_eq!(clouds.symbol, Some("OVC".to_string()));
    }

    #[test]
    fn metar_cloud_symbol_uses_source_clear_sky_tokens() {
        for (raw_text, expected) in [
            ("METAR KAAA 160400Z 00000KT 10SM CLR 10/08 A3000", "SKC"),
            ("METAR KBBB 160400Z 00000KT 10SM SKC 10/08 A3000", "SKC"),
            ("METAR BIKF 032000Z 34012KT CAVOK 06/M03 Q1012", "SKC"),
            ("METAR KCCC 160400Z 00000KT 10SM NSC 10/08 A3000", "NSC"),
        ] {
            assert_eq!(
                structured_metar_clouds(raw_text).symbol,
                Some(expected.to_string()),
                "{raw_text}",
            );
        }
    }

    #[test]
    fn metar_cloud_symbol_selects_current_observation_ceiling() {
        let clouds = structured_metar_clouds(
            "METAR KBBB 160355Z 18005KT 10SM FEW008 SCT020 BKN025 OVC040 12/09 A3001",
        );
        assert_eq!(clouds.symbol, Some("BKN".to_string()));
    }

    #[test]
    fn metar_cloud_symbol_selects_lowest_layer_without_ceiling() {
        let clouds = structured_metar_clouds(
            "METAR AGGH 032000Z 00000KT 9999 FEW0016 SCT100 24/24 Q1008 NOSIG",
        );
        assert_eq!(clouds.symbol, Some("FEW".to_string()));
    }

    #[test]
    fn metar_cloud_symbol_ignores_trend_groups() {
        let clouds = structured_metar_clouds(
            "METAR EHAM 031955Z 21005KT 9999 FEW013 SCT017 15/13 Q1010 TEMPO BKN014",
        );
        assert_eq!(clouds.symbol, Some("FEW".to_string()));
    }

    #[test]
    fn metar_cloud_symbol_handles_vertical_visibility() {
        let clouds =
            structured_metar_clouds("METAR KAAA 160400Z 00000KT 1/4SM FG VV002 10/08 A3000");
        assert_eq!(clouds.symbol, Some("VV".to_string()));
    }

    #[test]
    fn notam_dataset_normalizes_captured_swim_message() -> anyhow::Result<()> {
        let output_dir = TempDir::new().context("failed to create temp dir")?;
        let input_jsonl = output_dir.path().join("messages.jsonl");
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<AIXMBasicMessage xmlns="http://www.aixm.aero/schema/5.1/message"
  xmlns:aixm="http://www.aixm.aero/schema/5.1"
  xmlns:event="http://www.aixm.aero/schema/5.1/event"
  xmlns:fnse="http://www.aixm.aero/schema/5.1/extensions/FAA/FNSE"
  xmlns:gml="http://www.opengis.net/gml/3.2">
  <hasMember>
    <aixm:AirportHeliport>
      <aixm:timeSlice>
        <aixm:AirportHeliportTimeSlice>
          <aixm:name>HELENA RGNL</aixm:name>
          <aixm:ARP>
            <aixm:ElevatedPoint>
              <gml:pos>46.6067222222222 -111.983277777778</gml:pos>
            </aixm:ElevatedPoint>
          </aixm:ARP>
        </aixm:AirportHeliportTimeSlice>
      </aixm:timeSlice>
    </aixm:AirportHeliport>
  </hasMember>
  <hasMember>
    <event:Event>
      <event:timeSlice>
        <event:EventTimeSlice>
          <event:scenario>95</event:scenario>
          <event:textNOTAM>
            <event:NOTAM>
              <event:number>198</event:number>
              <event:year>2026</event:year>
              <event:type>N</event:type>
              <event:issued>2026-04-25T04:04:00.000Z</event:issued>
              <event:location>HLN</event:location>
              <event:effectiveStart>202604250404</event:effectiveStart>
              <event:effectiveEnd>202604251400</event:effectiveEnd>
              <event:text>RWY 35 FICON 5/5/5 100 PCT WET OBS AT 2604250404.</event:text>
              <event:translation>
                <event:NOTAMTranslation>
                  <event:type>LOCAL_FORMAT</event:type>
                  <event:simpleText>!HLN 04/198 HLN RWY 35 FICON</event:simpleText>
                </event:NOTAMTranslation>
              </event:translation>
              <event:translation>
                <event:NOTAMTranslation>
                  <event:type>OTHER:ICAO</event:type>
                  <event:formattedText>04/198 NOTAMR Q) KZLC/QMRXX</event:formattedText>
                </event:NOTAMTranslation>
              </event:translation>
            </event:NOTAM>
          </event:textNOTAM>
          <event:extension>
            <fnse:EventExtension>
              <fnse:classification>DOM</fnse:classification>
              <fnse:accountId>HLN</fnse:accountId>
              <fnse:xoveraccountID>KHLN</fnse:xoveraccountID>
              <fnse:xovernotamID>A1833/26</fnse:xovernotamID>
              <fnse:airportname>HELENA RGNL</fnse:airportname>
            </fnse:EventExtension>
          </event:extension>
        </event:EventTimeSlice>
      </event:timeSlice>
    </event:Event>
  </hasMember>
</AIXMBasicMessage>"#;
        let line = serde_json::json!({
            "jmsMessageId": "ID:test",
            "properties": {
                "m_msg_nms_id": "5822620521126030",
                "us_gov_dot_faa_aim_fns_nds_CorrelationID": "9406860",
                "us_gov_dot_faa_aim_fns_nds_SourceType": "D",
                "us_gov_dot_faa_aim_fns_nds_NOTAMStatus": "ACTIVE",
                "us_gov_dot_faa_aim_fns_nds_NOTAMFunction": "NOTAMN",
                "us_gov_dot_faa_aim_fns_nds_NOTAMKeyword": "RWY",
                "m_msg_last_updated": "2026-04-25T04:08:57.749Z",
                "us_gov_dot_faa_aim_fns_nds_LocationDesignator": "HLN",
                "us_gov_dot_faa_aim_fns_nds_ICAOId": "KHLN"
            },
            "bodyText": xml
        });
        fs::write(&input_jsonl, format!("{line}\n"))?;

        let generated_at_utc = DateTime::parse_from_rfc3339("2026-04-25T04:16:43Z")
            .expect("valid fixture timestamp")
            .with_timezone(&Utc);
        let result = build_notam_dataset(&BuildNotamRequest {
            input_jsonl_path: input_jsonl,
            output_dir: output_dir.path().join("out"),
            version_label: "fixture".to_string(),
            generated_at_utc,
        })?;

        let dataset: StructuredNotamDataset =
            serde_json::from_slice(&fs::read(&result.structured_json_path)?)?;
        assert_eq!(1, dataset.notam_count);
        let record = &dataset.notams[0];
        assert_eq!("D:HLN:2026:N:198", record.id);
        assert_eq!(Some(record), dataset.notams_by_id.get("D:HLN:2026:N:198"));
        assert_eq!(Some("KHLN".to_string()), record.icao_id.clone());
        assert_eq!(Some("RWY".to_string()), record.notam_keyword.clone());
        assert_eq!(Some("HELENA RGNL".to_string()), record.airport_name.clone());
        assert_eq!(Some("A1833/26".to_string()), record.xover_notam_id.clone());
        assert_eq!(
            Some("2026-04-25T04:04:00+00:00".to_string()),
            record.effective_start_utc.clone()
        );
        assert!(result.zip_path.exists());
        Ok(())
    }

    #[test]
    fn geoid_grid_interpolates_geo_fixture() -> anyhow::Result<()> {
        let grid = GeoidGrid::from_geo_csv(&geo_grid_fixture_path())?;
        assert_eq!(grid.geoid_height_feet_bilinear(-90.0, -180.0), -30.0);
        assert_eq!(grid.geoid_height_feet_bilinear(89.0, 179.0), 10.0);

        let west = grid.geoid_height_feet_bilinear(40.0, -122.0);
        let east = grid.geoid_height_feet_bilinear(40.0, -121.0);
        let midpoint = grid.geoid_height_feet_bilinear(40.0, -121.5);
        assert!((midpoint - ((west + east) / 2.0)).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn terrain_transform_adds_geoid_height_after_meter_to_feet_conversion() -> anyhow::Result<()> {
        let grid = GeoidGrid::from_geo_csv(&geo_grid_fixture_path())?;
        let transformed =
            terrain_ellipsoid_height_feet_from_navd88_meters(100.0, -90.0, -180.0, &grid);
        assert!((transformed - 298.0839895).abs() < 0.0001);
        Ok(())
    }
}
