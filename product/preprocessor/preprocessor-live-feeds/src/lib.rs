// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use chrono::{DateTime, NaiveDateTime, Utc};
use notam_state::NotamRecord;
use preprocessor_data::faa_procedure_id_candidate_groups;
use preprocessor_zip::{write_deterministic_zip, ZipSource};
use product_contracts::{
    AirportNotamEffect, ProcedurePublishedName, ProcedureRendezvousKey, ProcedureRendezvousKind,
};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub mod engine;
pub mod nms_initial_load;
pub mod notam_store;
pub mod products;
pub mod simulation;
pub mod tfr_detail_backfill;
mod winds_aloft;

const METAR_PRODUCT_CONTRACT_VERSION: u32 = 9;
const TAF_PRODUCT_CONTRACT_VERSION: u32 = 1;
const PIREP_PRODUCT_CONTRACT_VERSION: u32 = 1;
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
pub struct BuildPirepRequest {
    pub pirep_xml_path: PathBuf,
    pub output_dir: PathBuf,
    pub version_label: String,
    pub generated_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct BuildPirepResult {
    pub manifest_path: PathBuf,
    pub structured_json_path: PathBuf,
    pub zip_path: PathBuf,
    pub pirep_count: usize,
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
struct PirepManifest {
    schema_version: u32,
    version_label: String,
    generated_at_utc: String,
    files: PirepManifestFiles,
    counts: PirepManifestCounts,
}

#[derive(Debug, Clone, Serialize)]
struct PirepManifestFiles {
    manifest: String,
    structured_json: String,
    pireps: String,
    zip: String,
}

#[derive(Debug, Clone, Serialize)]
struct PirepManifestCounts {
    pireps: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StructuredNotamRecord {
    pub id: String,
    pub nms_id: Option<String>,
    pub source_type: Option<String>,
    pub notam_status: Option<String>,
    pub notam_function: Option<String>,
    pub notam_keyword: Option<String>,
    pub last_updated_utc: Option<String>,
    pub location_designator: Option<String>,
    pub icao_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub airport_id: Option<String>,
    #[serde(default)]
    pub airport_effects: BTreeSet<AirportNotamEffect>,
    #[serde(default)]
    pub procedure_rendezvous_keys: BTreeSet<ProcedureRendezvousKey>,
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

pub fn published_notam_record(record: &StructuredNotamRecord) -> Option<NotamRecord> {
    let published = NotamRecord {
        id: record.id.clone(),
        airport_id: record.airport_id.clone(),
        airport_effects: record.airport_effects.clone(),
        procedure_rendezvous_keys: record.procedure_rendezvous_keys.clone(),
        notam_keyword: record.notam_keyword.clone(),
        effective_start_utc: record.effective_start_utc.clone(),
        effective_end_utc: record.effective_end_utc.clone(),
        text: record.text.clone(),
        local_text: record.local_text.clone(),
        icao_text: record.icao_text.clone(),
    };
    published.is_displayable().then_some(published)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CanonicalNotamIdentity {
    source_type: String,
    location: Option<String>,
    year: String,
    notam_type: String,
    number: String,
}

impl CanonicalNotamIdentity {
    fn new(
        source_type: Option<&str>,
        location: Option<&str>,
        year: Option<&str>,
        notam_type: Option<&str>,
        number: Option<&str>,
    ) -> anyhow::Result<Self> {
        let source_type = required_uppercase_notam_field(source_type, "source type")?;
        if !matches!(source_type.as_str(), "D" | "F") {
            bail!("unsupported NOTAM source type {source_type}");
        }
        let location = location
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_uppercase);
        let year = required_uppercase_notam_field(year, "year")?;
        if year.len() != 4 || !year.chars().all(|ch| ch.is_ascii_digit()) {
            bail!("invalid NOTAM year {year}");
        }
        let source_notam_type = notam_type
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("N")
            .to_ascii_uppercase();
        let notam_type = match source_notam_type.as_str() {
            "N" | "R" | "C" => "N".to_string(),
            _ => bail!("unsupported NOTAM type {source_notam_type}"),
        };
        let raw_number = number
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .context("NOTAM number is missing")?;
        let number = match raw_number.split_once('/') {
            Some((series, number)) => {
                if !series.chars().all(|ch| ch.is_ascii_digit())
                    || !number.chars().all(|ch| ch.is_ascii_digit())
                {
                    bail!("invalid NOTAM number {raw_number}");
                }
                let series = series
                    .parse::<u64>()
                    .with_context(|| format!("invalid NOTAM series {series}"))?;
                let number = number
                    .parse::<u64>()
                    .with_context(|| format!("invalid NOTAM number {number}"))?;
                if source_type == "D" {
                    if !(1..=12).contains(&series) {
                        bail!("invalid domestic NOTAM month {series}");
                    }
                    format!("{series:02}/{number:03}")
                } else {
                    format!("{series}/{number:04}")
                }
            }
            None => {
                if !raw_number.chars().all(|ch| ch.is_ascii_digit()) {
                    bail!("invalid NOTAM number {raw_number}");
                }
                raw_number
                    .parse::<u64>()
                    .with_context(|| format!("invalid NOTAM number {raw_number}"))?
                    .to_string()
            }
        };
        Ok(Self {
            source_type,
            location,
            year,
            notam_type,
            number,
        })
    }

    fn id(&self) -> anyhow::Result<String> {
        let location = self
            .location
            .as_deref()
            .context("NOTAM location is missing")?;
        Ok(format!(
            "{}:{}:{}:{}:{}",
            self.source_type, location, self.year, self.notam_type, self.number
        ))
    }
}

#[derive(Debug, Default)]
struct NotamNormalizationHints {
    nms_id: Option<String>,
    source_type: Option<String>,
    notam_status: Option<String>,
    notam_function: Option<String>,
    notam_type: Option<String>,
    notam_keyword: Option<String>,
    last_updated_utc: Option<String>,
    location_designator: Option<String>,
    icao_id: Option<String>,
    notam_number: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CapturedNotamMessage {
    #[serde(default)]
    properties: serde_json::Map<String, Value>,
    #[serde(default, rename = "bodyText")]
    body_text: Option<String>,
    #[serde(default, rename = "bodyUtf8")]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PirepTextMode {
    RawText,
    ObservationTime,
    Latitude,
    Longitude,
    ReportType,
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
    generated_at_utc: String,
    observed_at_utc: String,
    pirep_count: usize,
    pireps_by_id: BTreeMap<String, StructuredPirepRecord>,
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

#[derive(Debug, Clone, Serialize)]
struct PirepProductModel {
    pireps_by_id: BTreeMap<String, StructuredPirepRecord>,
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

#[derive(Debug, Clone)]
struct ParsedPirepRecord {
    raw_text: String,
    observation_time: String,
    report_type: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    let text_rank = metadata.text.as_deref().is_some_and(text_contains_tfr) as u8;
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
    let tokens = text.split_whitespace().collect::<Vec<_>>();
    for (index, token) in tokens.iter().enumerate() {
        if let Some(limits) = parse_tfr_altitude_pair_token(token) {
            return Some(limits);
        }
        let Some(next) = tokens.get(index + 1) else {
            continue;
        };
        let next = next
            .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-')
            .to_ascii_uppercase();
        let Some(upper) = next
            .strip_prefix("MSL-")
            .or_else(|| next.strip_prefix("AGL-"))
        else {
            continue;
        };
        if let Some(limits) = parse_tfr_altitude_pair_token(&format!("{token}-{upper}")) {
            return Some(limits);
        }
    }
    None
}

fn parse_tfr_altitude_pair_token(token: &str) -> Option<(StructuredTfrLimit, StructuredTfrLimit)> {
    let token = token
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-')
        .to_ascii_uppercase();
    let (lower, upper) = token.split_once('-')?;
    if lower.is_empty() || upper.is_empty() {
        return None;
    }
    // Bare numeric ranges also describe compact NOTAM effective dates. At
    // least one side must carry syntax that identifies this as an altitude.
    if !tfr_altitude_limit_has_marker(lower) && !tfr_altitude_limit_has_marker(upper) {
        return None;
    }
    Some((
        parse_tfr_altitude_limit(lower)?,
        parse_tfr_altitude_limit(upper)?,
    ))
}

fn tfr_altitude_limit_has_marker(value: &str) -> bool {
    let value = value
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric())
        .to_ascii_uppercase();
    value == "SFC" || value.starts_with("FL") || value.ends_with("FT")
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
    if let Some(number) = value.strip_prefix("FL") {
        if !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit()) {
            return Some(StructuredTfrLimit {
                value_text: number.to_string(),
                unit: "FL".to_string(),
            });
        }
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

pub fn build_pirep_dataset(request: &BuildPirepRequest) -> anyhow::Result<BuildPirepResult> {
    if request.output_dir.exists() {
        fs::remove_dir_all(&request.output_dir)
            .with_context(|| format!("failed to clear {}", request.output_dir.display()))?;
    }
    fs::create_dir_all(&request.output_dir)
        .with_context(|| format!("failed to create {}", request.output_dir.display()))?;

    let model = pirep_product_model(&request.pirep_xml_path)?;
    let content_timestamp = pirep_product_content_timestamp(&model, request.generated_at_utc)?;
    let content_timestamp_text = content_timestamp.to_rfc3339();
    let pirep_count = model.pireps_by_id.len();
    let structured_json_path = request.output_dir.join("pireps.json");
    let canonical_manifest_path = request.output_dir.join("manifest.json");
    let manifest_path = request
        .output_dir
        .join(format!("pireps_{}.manifest.json", request.version_label));
    let zip_path = request
        .output_dir
        .join(format!("pireps_{}.zip", request.version_label));

    write_json_pretty(
        &structured_json_path,
        &StructuredPirepDataset {
            schema_version: 1,
            version_label: request.version_label.clone(),
            generated_at_utc: content_timestamp_text.clone(),
            observed_at_utc: content_timestamp_text.clone(),
            pirep_count,
            pireps_by_id: model.pireps_by_id.clone(),
        },
    )?;
    let manifest = PirepManifest {
        schema_version: PIREP_PRODUCT_CONTRACT_VERSION,
        version_label: request.version_label.clone(),
        generated_at_utc: content_timestamp_text,
        files: PirepManifestFiles {
            manifest: "manifest.json".to_string(),
            structured_json: "pireps.json".to_string(),
            pireps: "pireps.json".to_string(),
            zip: zip_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string(),
        },
        counts: PirepManifestCounts {
            pireps: pirep_count,
        },
    };
    write_json_pretty(&canonical_manifest_path, &manifest)?;
    write_json_pretty(&manifest_path, &manifest)?;
    write_zip(
        &zip_path,
        &[
            ("pireps.json", &structured_json_path),
            ("manifest.json", &canonical_manifest_path),
        ],
    )?;

    Ok(BuildPirepResult {
        manifest_path,
        structured_json_path,
        zip_path,
        pirep_count,
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

pub fn pirep_content_fingerprint(pirep_xml_path: &Path) -> anyhow::Result<String> {
    let model = pirep_product_model(pirep_xml_path)?;
    let bytes = serde_json::to_vec(&(PIREP_PRODUCT_CONTRACT_VERSION, model))
        .context("failed to encode canonical PIREP model")?;
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

fn pirep_product_model(pirep_xml_path: &Path) -> anyhow::Result<PirepProductModel> {
    Ok(PirepProductModel {
        pireps_by_id: structured_pirep_records(pirep_xml_path)?
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect(),
    })
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

fn pirep_product_content_timestamp(
    model: &PirepProductModel,
    fallback: DateTime<Utc>,
) -> anyhow::Result<DateTime<Utc>> {
    let mut latest = None;
    for value in model
        .pireps_by_id
        .values()
        .map(|record| record.observed_at_utc.as_str())
    {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let parsed = DateTime::parse_from_rfc3339(value)
            .with_context(|| format!("failed to parse PIREP source timestamp {value:?}"))?
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

fn structured_pirep_records(input_xml_path: &Path) -> anyhow::Result<Vec<StructuredPirepRecord>> {
    let mut records = parse_pirep_records(input_xml_path)?;
    records.sort_by(|left, right| {
        (
            &left.observation_time,
            &left.raw_text,
            &left.report_type,
            &left.longitude,
            &left.latitude,
        )
            .cmp(&(
                &right.observation_time,
                &right.raw_text,
                &right.report_type,
                &right.longitude,
                &right.latitude,
            ))
    });
    records
        .into_iter()
        .scan(
            BTreeMap::<String, usize>::new(),
            |occurrence_counts, record| -> Option<anyhow::Result<StructuredPirepRecord>> {
                let result = (|| -> anyhow::Result<StructuredPirepRecord> {
                    let hazards = parse_pirep_hazards(&record.raw_text);
                    let identity = serde_json::to_vec(&(
                        &record.observation_time,
                        &record.raw_text,
                        &record.report_type,
                        &record.longitude,
                        &record.latitude,
                    ))
                    .context("failed to encode PIREP identity")?;
                    let digest = format!("{:x}", Sha256::digest(identity));
                    let base_id = format!("pirep:{}", &digest[..16]);
                    let count = occurrence_counts.entry(base_id.clone()).or_insert(0);
                    let id = if *count == 0 {
                        base_id
                    } else {
                        format!("{base_id}:{}", *count)
                    };
                    *count += 1;
                    Ok(StructuredPirepRecord {
                        id,
                        raw_text: record.raw_text,
                        observed_at_utc: record.observation_time,
                        report_type: empty_to_none(record.report_type),
                        longitude: parse_optional_f64(&record.longitude)?,
                        latitude: parse_optional_f64(&record.latitude)?,
                        symbol: hazards.symbol().to_string(),
                        icing: hazards.icing.as_str().to_string(),
                        turbulence: hazards.turbulence.as_str().to_string(),
                    })
                })();
                Some(result)
            },
        )
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PirepHazardSeverity {
    None,
    Unknown,
    Light,
    Moderate,
    Severe,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PirepHazards {
    icing: PirepHazardSeverity,
    turbulence: PirepHazardSeverity,
}

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

fn pirep_hazard_tokens(section: &str) -> Vec<String> {
    section
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PirepHazardKind {
    Icing,
    Turbulence,
}

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

fn pirep_hazard_kind(token: &str) -> Option<PirepHazardKind> {
    match token {
        "IC" | "ICE" | "ICING" | "RIME" | "CLRICE" | "MXD" | "MXDICE" => {
            Some(PirepHazardKind::Icing)
        }
        "TB" | "TURB" | "TURBC" | "TURBULENCE" | "CHOP" => Some(PirepHazardKind::Turbulence),
        _ => None,
    }
}

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

pub fn structured_notam_record_from_json(
    message_json: &str,
) -> anyhow::Result<Option<StructuredNotamRecord>> {
    let message = serde_json::from_str::<CapturedNotamMessage>(message_json)
        .context("failed to parse captured NOTAM message JSON")?;
    let body = message
        .body_text
        .or(message.body_utf8)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let Some(body) = body else {
        return Ok(None);
    };
    let property = |name: &str| {
        message
            .properties
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    normalize_notam_xml(
        &body,
        NotamNormalizationHints {
            nms_id: property("m_msg_nms_id"),
            source_type: property("us_gov_dot_faa_aim_fns_nds_SourceType"),
            notam_status: property("us_gov_dot_faa_aim_fns_nds_NOTAMStatus"),
            notam_function: property("us_gov_dot_faa_aim_fns_nds_NOTAMFunction"),
            notam_type: None,
            notam_keyword: property("us_gov_dot_faa_aim_fns_nds_NOTAMKeyword"),
            last_updated_utc: property("m_msg_last_updated"),
            location_designator: property("us_gov_dot_faa_aim_fns_nds_LocationDesignator"),
            icao_id: property("us_gov_dot_faa_aim_fns_nds_ICAOId"),
            notam_number: property("us_gov_dot_faa_aim_fns_nds_NOTAMNumber"),
        },
    )
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

fn normalize_notam_xml(
    body: &str,
    hints: NotamNormalizationHints,
) -> anyhow::Result<Option<StructuredNotamRecord>> {
    let xml = roxmltree::Document::parse(body).context("failed to parse NOTAM XML body")?;
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

    let text = child_text(&notam_node, "text");
    let local_text = find_translation_text(&notam_node, "LOCAL_FORMAT");
    let icao_text = find_translation_text(&notam_node, "OTHER:ICAO");
    let location = child_text(&notam_node, "location");
    let location_designator =
        normalize_optional_notam_field(hints.location_designator.or_else(|| location.clone()));
    let icao_id = normalize_optional_notam_field(
        hints
            .icao_id
            .or_else(|| find_first_text(&xml, "icaoLocation")),
    );
    let source_type = hints.source_type.or_else(|| {
        source_type_for_classification(find_first_text(&xml, "classification").as_deref())
    });
    let notam_number = local_format_notam_number(local_text.as_deref())
        .or(hints.notam_number)
        .or_else(|| child_text(&notam_node, "number"));
    let notam_year = child_text(&notam_node, "year");
    let notam_type = child_text(&notam_node, "type").or(hints.notam_type);
    let identity = CanonicalNotamIdentity::new(
        source_type.as_deref(),
        location
            .as_deref()
            .or(location_designator.as_deref())
            .or(icao_id.as_deref()),
        notam_year.as_deref(),
        notam_type.as_deref(),
        notam_number.as_deref(),
    )?;
    let nms_id = normalize_nms_id(hints.nms_id.or_else(|| nms_id_from_xml(&xml)))?;
    let record_id = canonical_notam_record_id(nms_id.as_deref(), &identity)?;
    let notam_status = canonical_notam_status(hints.notam_status)?;
    let notam_function =
        canonical_notam_function(hints.notam_function.or_else(|| notam_type.clone()))?;
    let notam_keyword =
        canonical_notam_keyword(&identity.source_type, hints.notam_keyword, text.as_deref());
    let airport_id = airport_id_for_notam(
        notam_keyword.as_deref(),
        icao_id.as_deref(),
        location_designator.as_deref(),
        identity.location.as_deref(),
    );
    let procedure_rendezvous_keys = procedure_rendezvous_keys_for_notam(
        &xml,
        notam_keyword.as_deref(),
        airport_id.as_deref(),
        text.as_deref(),
    )?;
    let scenario = event_time_slice.and_then(|node| child_text(&node, "scenario"));
    let airport_effects = airport_id
        .as_ref()
        .map(|_| {
            airport_effects_for_notam(
                notam_keyword.as_deref(),
                scenario.as_deref(),
                text.as_deref(),
            )
        })
        .unwrap_or_default();
    let account_id = find_first_text(&xml, "accountId");
    let xover_notam_id = find_first_text(&xml, "xovernotamID");
    let airport_position = find_airport_position(&xml)?;

    Ok(Some(StructuredNotamRecord {
        id: record_id,
        nms_id,
        source_type: Some(identity.source_type.clone()),
        notam_status: Some(notam_status),
        notam_function,
        notam_keyword,
        last_updated_utc: hints
            .last_updated_utc
            .or_else(|| find_first_text(&xml, "lastUpdated")),
        location_designator,
        icao_id,
        airport_id,
        airport_effects,
        procedure_rendezvous_keys,
        airport_name: find_first_text(&xml, "airportname").or(find_first_text(&xml, "name")),
        airport_position,
        location: identity.location.clone(),
        classification: find_first_text(&xml, "classification"),
        account_id,
        xover_account_id: find_first_text(&xml, "xoveraccountID"),
        xover_notam_id,
        notam_number: Some(identity.number.clone()),
        notam_year: Some(identity.year.clone()),
        notam_type: Some(identity.notam_type.clone()),
        issued_utc: child_text(&notam_node, "issued"),
        effective_start_utc: child_text(&notam_node, "effectiveStart")
            .and_then(|value| parse_compact_notam_timestamp(&value).ok()),
        effective_end_utc: child_text(&notam_node, "effectiveEnd")
            .and_then(|value| parse_compact_notam_timestamp(&value).ok()),
        text,
        local_text,
        icao_text,
        scenario,
    }))
}

pub(crate) fn canonicalize_structured_notam_record(
    mut record: StructuredNotamRecord,
) -> anyhow::Result<StructuredNotamRecord> {
    record.location_designator = normalize_optional_notam_field(record.location_designator);
    record.icao_id = normalize_optional_notam_field(record.icao_id);
    record.notam_number = local_format_notam_number(record.local_text.as_deref())
        .or(record.notam_number)
        .map(|value| value.trim().to_ascii_uppercase());
    let identity = CanonicalNotamIdentity::new(
        record.source_type.as_deref(),
        record
            .location
            .as_deref()
            .or(record.location_designator.as_deref())
            .or(record.icao_id.as_deref()),
        record.notam_year.as_deref(),
        record.notam_type.as_deref(),
        record.notam_number.as_deref(),
    )?;
    record.nms_id = normalize_nms_id(record.nms_id)?;
    record.id = canonical_notam_record_id(record.nms_id.as_deref(), &identity)?;
    record.source_type = Some(identity.source_type.clone());
    record.location = identity.location;
    record.notam_year = Some(identity.year);
    record.notam_type = Some(identity.notam_type);
    record.notam_number = Some(identity.number);
    record.notam_status = Some(canonical_notam_status(record.notam_status)?);
    record.notam_function = canonical_notam_function(record.notam_function)?;
    record.notam_keyword = canonical_notam_keyword(
        record.source_type.as_deref().unwrap(),
        record.notam_keyword,
        record.text.as_deref(),
    );
    record.airport_id = airport_id_for_notam(
        record.notam_keyword.as_deref(),
        record.icao_id.as_deref(),
        record.location_designator.as_deref(),
        record.location.as_deref(),
    );
    record.airport_effects = record
        .airport_id
        .as_ref()
        .map(|_| {
            airport_effects_for_notam(
                record.notam_keyword.as_deref(),
                record.scenario.as_deref(),
                record.text.as_deref(),
            )
        })
        .unwrap_or_default();
    for key in &record.procedure_rendezvous_keys {
        key.validate()
            .map_err(anyhow::Error::msg)
            .with_context(|| {
                format!(
                    "NOTAM {} has an invalid procedure rendezvous key",
                    record.id
                )
            })?;
    }
    Ok(record)
}

pub(crate) fn validate_canonical_structured_notam_record(
    record: &StructuredNotamRecord,
) -> anyhow::Result<()> {
    let canonical = canonicalize_structured_notam_record(record.clone())?;
    if canonical != *record {
        bail!("NOTAM {} is not canonical", record.id);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotamProjectionAction {
    Upsert,
    Remove,
}

pub(crate) fn notam_projection_action(
    record: &StructuredNotamRecord,
) -> anyhow::Result<NotamProjectionAction> {
    match record.notam_status.as_deref() {
        Some("ACTIVE") => Ok(NotamProjectionAction::Upsert),
        Some("CANCELLED") => Ok(NotamProjectionAction::Remove),
        status => bail!(
            "canonical NOTAM {} has unsupported projected status {status:?}",
            record.id
        ),
    }
}

fn required_uppercase_notam_field(value: Option<&str>, label: &str) -> anyhow::Result<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_uppercase)
        .with_context(|| format!("NOTAM {label} is missing"))
}

fn normalize_optional_notam_field(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_ascii_uppercase())
        .filter(|value| !value.is_empty())
}

fn local_format_notam_number(local_text: Option<&str>) -> Option<String> {
    local_text?
        .split_whitespace()
        .take(4)
        .map(|token| {
            token.trim_matches(|character: char| {
                !character.is_ascii_alphanumeric() && character != '/'
            })
        })
        .find_map(|token| {
            let (series, number) = token.split_once('/')?;
            if (1..=2).contains(&series.len())
                && (3..=4).contains(&number.len())
                && series.chars().all(|character| character.is_ascii_digit())
                && number.chars().all(|character| character.is_ascii_digit())
            {
                Some(format!("{series}/{number}"))
            } else {
                None
            }
        })
}

fn source_type_for_classification(classification: Option<&str>) -> Option<String> {
    match classification?.trim().to_ascii_uppercase().as_str() {
        "DOM" | "DOMESTIC" => Some("D".to_string()),
        "FDC" => Some("F".to_string()),
        _ => None,
    }
}

fn normalize_nms_id(value: Option<String>) -> anyhow::Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().strip_prefix("NMS_ID_").unwrap_or(value.trim());
    if value.is_empty() || !value.chars().all(|character| character.is_ascii_digit()) {
        bail!("invalid NMS ID {value}");
    }
    Ok(Some(value.to_string()))
}

fn canonical_notam_record_id(
    nms_id: Option<&str>,
    fallback_identity: &CanonicalNotamIdentity,
) -> anyhow::Result<String> {
    match nms_id {
        Some(nms_id) => Ok(format!("NMS:{nms_id}")),
        None => fallback_identity.id(),
    }
}

fn nms_id_from_xml(document: &roxmltree::Document<'_>) -> Option<String> {
    document
        .descendants()
        .find(|node| node.tag_name().name() == "AIXMBasicMessage")
        .and_then(|node| node.attributes().find(|attribute| attribute.name() == "id"))
        .map(|attribute| attribute.value().trim())
        .and_then(|value| value.strip_prefix("NMS_ID_").or(Some(value)))
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn canonical_notam_status(value: Option<String>) -> anyhow::Result<String> {
    let value = required_uppercase_notam_field(value.as_deref(), "status")?;
    match value.as_str() {
        "ACTIVE" | "PUBLISHED" => Ok("ACTIVE".to_string()),
        "CANCELED" | "CANCELLED" => Ok("CANCELLED".to_string()),
        _ => bail!("unsupported NOTAM status {value}"),
    }
}

fn canonical_notam_function(value: Option<String>) -> anyhow::Result<Option<String>> {
    let Some(value) = normalize_optional_notam_field(value) else {
        return Ok(None);
    };
    match value.as_str() {
        "N" | "NOTAMN" => Ok(Some("NOTAMN".to_string())),
        "R" | "NOTAMR" => Ok(Some("NOTAMR".to_string())),
        "C" | "NOTAMC" => Ok(Some("NOTAMC".to_string())),
        _ => bail!("unsupported NOTAM function {value}"),
    }
}

fn canonical_notam_keyword(
    source_type: &str,
    property_keyword: Option<String>,
    text: Option<&str>,
) -> Option<String> {
    let property_keyword = normalize_optional_notam_field(property_keyword);
    let text_keyword = text.and_then(notam_keyword_from_text);
    if source_type == "D" {
        text_keyword.or(property_keyword)
    } else {
        property_keyword.or(text_keyword)
    }
}

fn notam_keyword_from_text(text: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "AD", "AIRSPACE", "APRON", "COM", "IAP", "NAV", "OBST", "ODP", "RWY", "SID", "SPECIAL",
        "STAR", "SVC", "TWY",
    ];
    let candidate = text
        .split_whitespace()
        .next()?
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric())
        .to_ascii_uppercase();
    KEYWORDS.contains(&candidate.as_str()).then_some(candidate)
}

fn textual_iap_candidate_groups(text: &str) -> Vec<BTreeSet<String>> {
    let mut groups = BTreeSet::new();
    for segment in text.split("...") {
        let words = segment.split_whitespace().collect::<Vec<_>>();
        let Some(revision_index) = words.iter().position(|word| {
            let word = word.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-');
            word == "AMDT" || word == "AMT" || word.starts_with("ORIG")
        }) else {
            continue;
        };

        let title_with_heading = words[..revision_index].join(" ");
        let title = title_with_heading
            .rsplit_once(". ")
            .map_or(title_with_heading.as_str(), |(_, title)| title)
            .trim_end_matches([',', '.']);
        groups.extend(faa_procedure_id_candidate_groups(title));
    }
    groups.into_iter().collect()
}

fn procedure_rendezvous_keys_for_notam(
    document: &roxmltree::Document<'_>,
    keyword: Option<&str>,
    airport_id: Option<&str>,
    text: Option<&str>,
) -> anyhow::Result<BTreeSet<ProcedureRendezvousKey>> {
    let mut keys = BTreeSet::new();
    match keyword {
        Some("IAP") => {
            let Some(airport_id) = airport_id else {
                return Ok(keys);
            };
            for time_slice in document.descendants().filter(|node| {
                node.is_element()
                    && node.tag_name().name() == "InstrumentApproachProcedureTimeSlice"
            }) {
                let Some(name) = child_text(&time_slice, "name") else {
                    continue;
                };
                for procedure_id in faa_procedure_id_candidate_groups(&name)
                    .into_iter()
                    .flatten()
                {
                    keys.insert(
                        ProcedureRendezvousKey::airport_scoped(
                            ProcedureRendezvousKind::Approach,
                            airport_id,
                            &procedure_id,
                        )
                        .map_err(anyhow::Error::msg)?,
                    );
                }
            }
            for procedure_id in text
                .into_iter()
                .flat_map(textual_iap_candidate_groups)
                .flatten()
            {
                keys.insert(
                    ProcedureRendezvousKey::airport_scoped(
                        ProcedureRendezvousKind::Approach,
                        airport_id,
                        &procedure_id,
                    )
                    .map_err(anyhow::Error::msg)?,
                );
            }
        }
        Some("SID") | Some("ODP") => {
            let Some(airport_id) = airport_id else {
                return Ok(keys);
            };
            if keyword == Some("ODP") && text.is_some_and(explicitly_names_takeoff_minimums) {
                keys.insert(
                    ProcedureRendezvousKey::airport_scoped_takeoff_minimums(airport_id)
                        .map_err(anyhow::Error::msg)?,
                );
            }
            for time_slice in document.descendants().filter(|node| {
                node.is_element()
                    && node.tag_name().name() == "StandardInstrumentDepartureTimeSlice"
            }) {
                if let Some(procedure_id) = descendant_text(&time_slice, "legacyControlNumber")
                    .and_then(|control| terminal_control_procedure_id(&control, false))
                {
                    keys.insert(
                        ProcedureRendezvousKey::airport_scoped(
                            ProcedureRendezvousKind::Departure,
                            airport_id,
                            &procedure_id,
                        )
                        .map_err(anyhow::Error::msg)?,
                    );
                }
                if let Some(published_name) = structured_published_procedure_name(&time_slice) {
                    keys.insert(
                        ProcedureRendezvousKey::airport_scoped_published_name(
                            ProcedureRendezvousKind::Departure,
                            airport_id,
                            &published_name,
                        )
                        .map_err(anyhow::Error::msg)?,
                    );
                }
            }
            for published_name in
                textual_published_procedure_names(text.unwrap_or_default(), &["DEPARTURE"])
            {
                keys.insert(
                    ProcedureRendezvousKey::airport_scoped_published_name(
                        ProcedureRendezvousKind::Departure,
                        airport_id,
                        &published_name,
                    )
                    .map_err(anyhow::Error::msg)?,
                );
            }
        }
        Some("STAR") => {
            for time_slice in document.descendants().filter(|node| {
                node.is_element() && node.tag_name().name() == "StandardInstrumentArrivalTimeSlice"
            }) {
                if let Some(procedure_id) = descendant_text(&time_slice, "legacyControlNumber")
                    .and_then(|control| terminal_control_procedure_id(&control, true))
                {
                    keys.insert(
                        ProcedureRendezvousKey::shared_arrival(&procedure_id)
                            .map_err(anyhow::Error::msg)?,
                    );
                }
                if let Some(published_name) = structured_published_procedure_name(&time_slice) {
                    keys.insert(
                        ProcedureRendezvousKey::shared_arrival_published_name(&published_name)
                            .map_err(anyhow::Error::msg)?,
                    );
                }
            }
            for procedure_id in textual_star_cifp_ids(text.unwrap_or_default()) {
                keys.insert(
                    ProcedureRendezvousKey::shared_arrival(&procedure_id)
                        .map_err(anyhow::Error::msg)?,
                );
            }
            for published_name in
                textual_published_procedure_names(text.unwrap_or_default(), &["ARR", "ARRIVAL"])
            {
                keys.insert(
                    ProcedureRendezvousKey::shared_arrival_published_name(&published_name)
                        .map_err(anyhow::Error::msg)?,
                );
            }
        }
        _ => {}
    }
    Ok(keys)
}

fn explicitly_names_takeoff_minimums(text: &str) -> bool {
    text.split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_uppercase)
        .collect::<Vec<_>>()
        .windows(2)
        .any(|words| words == ["TAKEOFF", "MINIMUMS"])
}

fn descendant_text(node: &roxmltree::Node<'_, '_>, local_name: &str) -> Option<String> {
    node.descendants()
        .find(|child| child.is_element() && child.tag_name().name() == local_name)
        .and_then(text_content)
}

fn terminal_control_procedure_id(control: &str, arrival: bool) -> Option<String> {
    let components = control
        .trim()
        .split('.')
        .map(str::trim)
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    let candidate = if arrival {
        components.last()
    } else {
        components.first()
    }?;
    candidate
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        .then(|| candidate.to_ascii_uppercase())
}

fn structured_published_procedure_name(time_slice: &roxmltree::Node<'_, '_>) -> Option<String> {
    let name = child_text(time_slice, "name")?;
    if ProcedurePublishedName::parse(&name).is_ok() {
        return Some(name);
    }
    let revision = descendant_text(time_slice, "printableVersionNumber")?;
    let combined = format!("{name} {revision}");
    ProcedurePublishedName::parse(&combined)
        .is_ok()
        .then_some(combined)
}

fn textual_star_cifp_ids(text: &str) -> BTreeSet<String> {
    let uppercase = text.to_ascii_uppercase();
    let mut ids = BTreeSet::new();

    for parenthesized in uppercase
        .split('(')
        .skip(1)
        .filter_map(|tail| tail.split_once(')'))
    {
        let candidate = parenthesized
            .0
            .rsplit_once('.')
            .map_or(parenthesized.0, |(_, candidate)| candidate)
            .trim();
        let Some((revision, stem)) = candidate.chars().next_back().map(|revision| {
            let stem = &candidate[..candidate.len() - revision.len_utf8()];
            (revision, stem)
        }) else {
            continue;
        };
        if matches!(revision, '1'..='9')
            && (2..=5).contains(&stem.len())
            && stem.chars().all(|ch| ch.is_ascii_uppercase())
        {
            ids.insert(candidate.to_string());
        }
    }

    ids
}

fn textual_published_procedure_names(text: &str, markers: &[&str]) -> BTreeSet<String> {
    let uppercase = text.to_ascii_uppercase();
    let mut names = BTreeSet::new();
    for clause in uppercase.split('.') {
        let words = clause
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .filter(|word| !word.is_empty())
            .collect::<Vec<_>>();
        let starts_with_heading = words
            .first()
            .is_some_and(|word| matches!(*word, "SID" | "STAR" | "ODP" | "IAP" | "SPECIAL"));
        for marker_index in words
            .iter()
            .enumerate()
            .filter_map(|(index, word)| markers.contains(word).then_some(index))
        {
            let mut end = marker_index;
            if end > 0 && words[end - 1] == "RNAV" {
                end -= 1;
            }
            let candidate = if starts_with_heading {
                // Some NMS rows omit the separator between the airport heading
                // and the procedure. The shortest valid suffix before ARRIVAL
                // or DEPARTURE isolates the published name without absorbing
                // the airport name into the identity.
                (1..end).rev().find_map(|start| {
                    let candidate = words[start..end].join(" ");
                    ProcedurePublishedName::parse(&candidate)
                        .is_ok()
                        .then_some(candidate)
                })
            } else {
                let candidate = words[..end].join(" ");
                ProcedurePublishedName::parse(&candidate)
                    .is_ok()
                    .then_some(candidate)
            };
            if let Some(candidate) = candidate {
                names.insert(candidate);
            }
        }
    }
    names
}

fn airport_id_for_notam(
    keyword: Option<&str>,
    icao_id: Option<&str>,
    location_designator: Option<&str>,
    location: Option<&str>,
) -> Option<String> {
    const AIRPORT_KEYWORDS: &[&str] = &["AD", "APRON", "IAP", "ODP", "RWY", "SID", "TWY"];
    AIRPORT_KEYWORDS
        .contains(&keyword?)
        .then(|| icao_id.or(location_designator).or(location))
        .flatten()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_uppercase)
}

fn airport_effects_for_notam(
    keyword: Option<&str>,
    scenario: Option<&str>,
    text: Option<&str>,
) -> BTreeSet<AirportNotamEffect> {
    let Some(keyword) = keyword.map(str::trim).filter(|value| !value.is_empty()) else {
        return BTreeSet::from([AirportNotamEffect::Other]);
    };
    let keyword = keyword.to_ascii_uppercase();
    let scenario = scenario
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_uppercase);
    let (known_scenario, scenario_effect) =
        airport_effect_for_scenario(&keyword, scenario.as_deref());
    let mut effects = BTreeSet::new();
    effects.extend(scenario_effect);
    effects.extend(airport_effects_from_text(
        &keyword,
        text.unwrap_or_default(),
    ));

    // Preserve an explicit signal for new FAA scenarios even when their text happens
    // to match one of the narrow recognizers below.
    if scenario.is_some() && !known_scenario {
        effects.insert(AirportNotamEffect::Other);
    }
    if effects.is_empty() {
        effects.insert(AirportNotamEffect::Other);
    }
    effects
}

fn airport_effect_for_scenario(
    keyword: &str,
    scenario: Option<&str>,
) -> (bool, Option<AirportNotamEffect>) {
    use AirportNotamEffect as Effect;

    let Some(scenario) = scenario else {
        return (false, None);
    };
    let effect = match (keyword, scenario) {
        ("AD", "25") => Some(Effect::AirportClosed),
        ("AD", "302") => Some(Effect::WorkInProgress),
        ("AD", "26" | "100" | "22") => Some(Effect::RoutineAdvisory),
        ("AD", "27" | "34") => Some(Effect::MovementAreaEquipmentUnavailable),
        ("AD", "576") => Some(Effect::RunwayEquipmentUnavailable),
        ("AD", "78") => Some(Effect::SurfaceCondition),
        ("APRON", "28") => Some(Effect::ApronClosed),
        ("APRON", "304") => Some(Effect::WorkInProgress),
        ("APRON", "43" | "111") => Some(Effect::SurfaceCondition),
        ("IAP", "802" | "FDC001") => Some(Effect::ProcedureRestricted),
        ("ODP", "811") | ("SID", "812") => Some(Effect::ProcedureRestricted),
        ("RWY", "50" | "82" | "86") => Some(Effect::RunwayClosed),
        ("RWY", "18" | "29" | "49" | "100" | "461" | "522" | "526") => {
            Some(Effect::RunwayEquipmentUnavailable)
        }
        ("RWY", "301") => Some(Effect::WorkInProgress),
        ("RWY", "95") => Some(Effect::SurfaceCondition),
        ("RWY", "87") => Some(Effect::RoutineAdvisory),
        ("TWY", "30" | "115") => Some(Effect::TaxiwayClosed),
        ("TWY", "303") => Some(Effect::WorkInProgress),
        ("TWY", "36" | "39" | "305") => Some(Effect::MovementAreaEquipmentUnavailable),
        ("TWY", "42" | "110") => Some(Effect::SurfaceCondition),
        // FF001 is FAA's generic/free-form scenario. Its standardized NOTAM text is
        // the semantic source, so this tuple is known even though it adds no effect.
        ("AD" | "APRON" | "RWY" | "TWY", "FF001") => None,
        _ => return (false, None),
    };
    (true, effect)
}

fn airport_effects_from_text(keyword: &str, text: &str) -> BTreeSet<AirportNotamEffect> {
    use AirportNotamEffect as Effect;

    let text = text.to_ascii_uppercase();
    let mut effects = BTreeSet::new();
    if has_notam_token(&text, "CLSD") {
        match keyword {
            "AD" => {
                effects.insert(Effect::AirportClosed);
            }
            "RWY" => {
                effects.insert(Effect::RunwayClosed);
                if text.contains("CLSD TO") || text.contains("CLSD EXC") {
                    effects.insert(Effect::RunwayRestricted);
                }
            }
            "TWY" => {
                effects.insert(Effect::TaxiwayClosed);
            }
            "APRON" => {
                effects.insert(Effect::ApronClosed);
            }
            "IAP" | "ODP" | "SID" => {
                effects.insert(Effect::ProcedureUnavailable);
            }
            _ => {}
        }
    }
    if matches!(keyword, "IAP" | "ODP" | "SID") && has_notam_token(&text, "NA") {
        effects.insert(Effect::ProcedureUnavailable);
    }
    if has_notam_token(&text, "U/S")
        || text.contains("OUT OF SERVICE")
        || has_notam_token(&text, "UNMONITORED")
        || text.contains("NOT STD")
    {
        match keyword {
            "RWY" => {
                effects.insert(Effect::RunwayEquipmentUnavailable);
            }
            "IAP" | "ODP" | "SID" => {
                effects.insert(Effect::ProcedureRestricted);
            }
            "AD" | "APRON" | "TWY" => {
                effects.insert(Effect::MovementAreaEquipmentUnavailable);
            }
            _ => {}
        }
    }
    if has_notam_token(&text, "FICON") || text.contains("SFC COND") {
        effects.insert(Effect::SurfaceCondition);
    }
    if has_notam_token(&text, "WIP")
        || has_notam_token(&text, "MOWING")
        || text.contains("GRASS CUTTING")
        || has_notam_token(&text, "CONSTRUCTION")
        || has_notam_token(&text, "MAINT")
    {
        effects.insert(Effect::WorkInProgress);
    }
    effects
}

fn has_notam_token(text: &str, expected: &str) -> bool {
    text.split_whitespace().any(|token| {
        token.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '/') == expected
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
    let value = value
        .strip_suffix("EST")
        .map(str::trim_end)
        .unwrap_or(value);
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

fn parse_pirep_records(path: &Path) -> anyhow::Result<Vec<ParsedPirepRecord>> {
    let xml =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut reader = Reader::from_str(&xml);
    let mut buffer = Vec::new();
    let mut records = Vec::new();
    let mut in_report = false;
    let mut mode = None;
    let mut current = ParsedPirepRecord {
        raw_text: String::new(),
        observation_time: String::new(),
        report_type: String::new(),
        longitude: String::new(),
        latitude: String::new(),
    };

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => match event.name().as_ref() {
                b"AircraftReport" => {
                    in_report = true;
                    current = ParsedPirepRecord {
                        raw_text: String::new(),
                        observation_time: String::new(),
                        report_type: String::new(),
                        longitude: String::new(),
                        latitude: String::new(),
                    };
                    mode = None;
                }
                b"raw_text" if in_report => mode = Some(PirepTextMode::RawText),
                b"observation_time" if in_report => mode = Some(PirepTextMode::ObservationTime),
                b"latitude" if in_report => mode = Some(PirepTextMode::Latitude),
                b"longitude" if in_report => mode = Some(PirepTextMode::Longitude),
                b"report_type" if in_report => mode = Some(PirepTextMode::ReportType),
                _ => {}
            },
            Ok(Event::End(event)) => match event.name().as_ref() {
                b"AircraftReport" if in_report => {
                    if !current.raw_text.is_empty() {
                        records.push(current.clone());
                    }
                    in_report = false;
                    mode = None;
                }
                b"raw_text" | b"observation_time" | b"latitude" | b"longitude" | b"report_type" => {
                    mode = None
                }
                _ => {}
            },
            Ok(Event::Text(event)) if in_report => {
                let text = event
                    .xml_content()
                    .context("failed to decode aircraft report XML text")?
                    .into_owned();
                push_pirep_text(&mut current, mode, &text);
            }
            Ok(Event::CData(event)) if in_report => {
                let text = event
                    .xml_content()
                    .context("failed to decode aircraft report XML cdata")?
                    .into_owned();
                push_pirep_text(&mut current, mode, &text);
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

fn push_pirep_text(current: &mut ParsedPirepRecord, mode: Option<PirepTextMode>, text: &str) {
    match mode {
        Some(PirepTextMode::RawText) => current.raw_text.push_str(text),
        Some(PirepTextMode::ObservationTime) => current.observation_time.push_str(text),
        Some(PirepTextMode::Latitude) => current.latitude.push_str(text),
        Some(PirepTextMode::Longitude) => current.longitude.push_str(text),
        Some(PirepTextMode::ReportType) => current.report_type.push_str(text),
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
    use std::io::{BufWriter, Write};
    use std::process::Command;
    use tempfile::{NamedTempFile, TempDir};

    #[test]
    fn compact_notam_timestamp_accepts_estimated_suffix() -> anyhow::Result<()> {
        assert_eq!(
            parse_compact_notam_timestamp("202607241405EST")?,
            "2026-07-24T14:05:00+00:00"
        );
        Ok(())
    }

    #[test]
    fn airport_notam_effects_combine_structured_and_text_signals() {
        assert_eq!(
            airport_effects_for_notam(
                Some("AD"),
                Some("302"),
                Some("AD AP CLSD EXC AIR AMBULANCE; WIP GRASS CUTTING"),
            ),
            BTreeSet::from([
                AirportNotamEffect::AirportClosed,
                AirportNotamEffect::WorkInProgress,
            ])
        );
    }

    #[test]
    fn airport_notam_effects_keep_unknown_scenarios_visible() {
        assert_eq!(
            airport_effects_for_notam(
                Some("RWY"),
                Some("NEW999"),
                Some("RWY 18 CLSD TO LDG EXC MIL OPS"),
            ),
            BTreeSet::from([
                AirportNotamEffect::RunwayClosed,
                AirportNotamEffect::RunwayRestricted,
                AirportNotamEffect::Other,
            ])
        );
        assert_eq!(
            airport_effects_for_notam(Some("AD"), Some("NEW999"), Some("AD AP INFO")),
            BTreeSet::from([AirportNotamEffect::Other])
        );
    }

    #[test]
    fn airport_notam_effects_distinguish_closures_from_routine_work() {
        assert_eq!(
            airport_effects_for_notam(Some("RWY"), Some("82"), Some("RWY 16/34 CLSD")),
            BTreeSet::from([AirportNotamEffect::RunwayClosed])
        );
        assert_eq!(
            airport_effects_for_notam(Some("AD"), Some("FF001"), Some("AD AP ALL SFC WIP MOWING"),),
            BTreeSet::from([AirportNotamEffect::WorkInProgress])
        );
    }

    #[test]
    fn nms_structured_procedures_emit_exact_rendezvous_candidates() -> anyhow::Result<()> {
        let xml = roxmltree::Document::parse(
            r#"<root>
                <InstrumentApproachProcedureTimeSlice>
                  <name>ILS OR LOC RWY 34C</name>
                </InstrumentApproachProcedureTimeSlice>
                <StandardInstrumentDepartureTimeSlice>
                  <name>KUSSO</name>
                  <extension><legacyControlNumber>KUSSO3.KUSSO</legacyControlNumber></extension>
                </StandardInstrumentDepartureTimeSlice>
              </root>"#,
        )?;

        assert_eq!(
            procedure_rendezvous_keys_for_notam(&xml, Some("IAP"), Some("KSEA"), None)?,
            BTreeSet::from([
                ProcedureRendezvousKey::airport_scoped(
                    ProcedureRendezvousKind::Approach,
                    "KSEA",
                    "I34C",
                )
                .unwrap(),
                ProcedureRendezvousKey::airport_scoped(
                    ProcedureRendezvousKind::Approach,
                    "KSEA",
                    "L34C",
                )
                .unwrap(),
            ])
        );
        assert_eq!(
            procedure_rendezvous_keys_for_notam(&xml, Some("SID"), Some("KTKI"), None)?,
            BTreeSet::from([ProcedureRendezvousKey::airport_scoped(
                ProcedureRendezvousKind::Departure,
                "KTKI",
                "KUSSO3",
            )
            .unwrap(),])
        );
        Ok(())
    }

    #[test]
    fn nms_one_digit_approach_runway_uses_cycle_canonical_key() -> anyhow::Result<()> {
        let xml = roxmltree::Document::parse(
            r#"<root>
                <InstrumentApproachProcedureTimeSlice>
                  <name>RNAV (GPS) RWY 6</name>
                </InstrumentApproachProcedureTimeSlice>
              </root>"#,
        )?;

        assert_eq!(
            procedure_rendezvous_keys_for_notam(&xml, Some("IAP"), Some("04W"), None)?,
            BTreeSet::from([ProcedureRendezvousKey::airport_scoped(
                ProcedureRendezvousKind::Approach,
                "04W",
                "R06",
            )
            .unwrap()]),
        );
        Ok(())
    }

    #[test]
    fn nms_text_only_iap_notam_emits_every_revision_qualified_title() -> anyhow::Result<()> {
        let xml = roxmltree::Document::parse("<root />")?;

        assert_eq!(
            procedure_rendezvous_keys_for_notam(
                &xml,
                Some("IAP"),
                Some("KALB"),
                Some(
                    "IAP ALBANY INTL, ALBANY, NY. ILS OR LOC RWY 1, AMDT 11D... \
                     ILS RWY 1 (SA CAT II), AMDT 11D... PROCEDURE NA.",
                ),
            )?,
            BTreeSet::from([
                ProcedureRendezvousKey::airport_scoped(
                    ProcedureRendezvousKind::Approach,
                    "KALB",
                    "I01",
                )
                .unwrap(),
                ProcedureRendezvousKey::airport_scoped(
                    ProcedureRendezvousKind::Approach,
                    "KALB",
                    "L01",
                )
                .unwrap(),
            ])
        );
        assert_eq!(
            procedure_rendezvous_keys_for_notam(
                &xml,
                Some("IAP"),
                Some("KAPF"),
                Some("IAP NAPLES, NAPLES, FL. RNAV (GPS) Y RWY 23\nORIG-A... LPV NA."),
            )?,
            BTreeSet::from([ProcedureRendezvousKey::airport_scoped(
                ProcedureRendezvousKind::Approach,
                "KAPF",
                "R23-Y",
            )
            .unwrap()])
        );
        assert_eq!(
            procedure_rendezvous_keys_for_notam(
                &xml,
                Some("IAP"),
                Some("KOMA"),
                Some("IAP EPPLEY AIRFIELD, OMAHA, NE. ILS RWY 14R (CAT II-III), AMT 6"),
            )?,
            BTreeSet::from([ProcedureRendezvousKey::airport_scoped(
                ProcedureRendezvousKind::Approach,
                "KOMA",
                "I14R",
            )
            .unwrap()])
        );
        assert!(procedure_rendezvous_keys_for_notam(
            &xml,
            Some("IAP"),
            Some("KCNP"),
            Some("IAP BILLY G RAY FLD, CHAPPELL, NE. NDB OR GPS RWY 30, AMDT 2C..."),
        )?
        .is_empty());
        Ok(())
    }

    #[test]
    fn nms_multi_sid_notam_emits_every_departure_rendezvous_key() -> anyhow::Result<()> {
        let xml = roxmltree::Document::parse(
            r#"<root>
                <StandardInstrumentDepartureTimeSlice>
                  <name>FARMINGTON SEVEN</name>
                  <extension><legacyControlNumber>FARM7.FARM</legacyControlNumber></extension>
                </StandardInstrumentDepartureTimeSlice>
                <StandardInstrumentDepartureTimeSlice>
                  <name>SCAPO SEVEN</name>
                  <extension><legacyControlNumber>SCAPO7.SCAPO</legacyControlNumber></extension>
                </StandardInstrumentDepartureTimeSlice>
              </root>"#,
        )?;

        assert_eq!(
            procedure_rendezvous_keys_for_notam(
                &xml,
                Some("SID"),
                Some("KHIO"),
                Some(
                    "SID PORTLAND-HILLSBORO, PORTLAND, OR.\n\
                     FARMINGTON SEVEN DEPARTURE...\n\
                     SCAPO SEVEN DEPARTURE...",
                ),
            )?,
            BTreeSet::from([
                ProcedureRendezvousKey::airport_scoped(
                    ProcedureRendezvousKind::Departure,
                    "KHIO",
                    "FARM7",
                )
                .unwrap(),
                ProcedureRendezvousKey::airport_scoped(
                    ProcedureRendezvousKind::Departure,
                    "KHIO",
                    "SCAPO7",
                )
                .unwrap(),
                ProcedureRendezvousKey::airport_scoped_published_name(
                    ProcedureRendezvousKind::Departure,
                    "KHIO",
                    "FARMINGTON SEVEN",
                )
                .unwrap(),
                ProcedureRendezvousKey::airport_scoped_published_name(
                    ProcedureRendezvousKind::Departure,
                    "KHIO",
                    "SCAPO SEVEN",
                )
                .unwrap(),
            ]),
        );
        Ok(())
    }

    #[test]
    fn nms_text_only_sid_uses_published_name_without_inventing_cifp_ids() -> anyhow::Result<()> {
        let xml = roxmltree::Document::parse("<root/>")?;
        assert_eq!(
            procedure_rendezvous_keys_for_notam(
                &xml,
                Some("SID"),
                Some("KAFW"),
                Some(
                    "SID PEROT FLD/FORT WORTH ALLIANCE, FORT WORTH, TX.\n\
                     WORTH ONE DEPARTURE ...\n\
                     JOE POOL EIGHT DEPARTURE ...",
                ),
            )?,
            BTreeSet::from([
                ProcedureRendezvousKey::airport_scoped_published_name(
                    ProcedureRendezvousKind::Departure,
                    "KAFW",
                    "WORTH ONE",
                )
                .unwrap(),
                ProcedureRendezvousKey::airport_scoped_published_name(
                    ProcedureRendezvousKind::Departure,
                    "KAFW",
                    "JOE POOL EIGHT",
                )
                .unwrap(),
            ]),
        );
        Ok(())
    }

    #[test]
    fn nms_odp_takeoff_minimums_emits_typed_airport_key() -> anyhow::Result<()> {
        let xml = roxmltree::Document::parse("<root/>")?;
        assert_eq!(
            procedure_rendezvous_keys_for_notam(
                &xml,
                Some("ODP"),
                Some("KDAN"),
                Some(
                    "ODP DANVILLE RGNL, DANVILLE, VA.\n\
                     TAKEOFF MINIMUMS AND (OBSTACLE) DEPARTURE PROCEDURES AMDT 2A...",
                ),
            )?,
            BTreeSet::from([
                ProcedureRendezvousKey::airport_scoped_takeoff_minimums("KDAN").unwrap(),
            ]),
        );
        assert!(procedure_rendezvous_keys_for_notam(
            &xml,
            Some("ODP"),
            Some("KDAN"),
            Some("ODP DANVILLE ONE DEPARTURE PROCEDURE NA"),
        )?
        .iter()
        .all(|key| {
            key.identity != product_contracts::ProcedureRendezvousIdentity::TakeoffMinimums
        }));
        Ok(())
    }

    #[test]
    fn nms_star_text_emits_one_shared_key_for_every_served_airport() -> anyhow::Result<()> {
        let xml = roxmltree::Document::parse("<root/>")?;
        let keys = procedure_rendezvous_keys_for_notam(
            &xml,
            Some("STAR"),
            None,
            Some(
                "STAR SEA CHART CHINS FIVE ARRIVAL (CHINS.CHINS5) PROCEDURE NA; \
                 WINDMILL AT 873 OBS (25-077159)",
            ),
        )?;
        assert_eq!(
            keys,
            BTreeSet::from([
                ProcedureRendezvousKey::shared_arrival("CHINS5").unwrap(),
                ProcedureRendezvousKey::shared_arrival_published_name("CHINS FIVE").unwrap(),
            ])
        );
        assert_eq!(
            notam_keyword_from_text("STAR CHINS FIVE ARRIVAL PROCEDURE NA").as_deref(),
            Some("STAR")
        );
        Ok(())
    }

    #[test]
    fn nms_star_text_accepts_direct_parenthesized_cifp_id() -> anyhow::Result<()> {
        let xml = roxmltree::Document::parse("<root/>")?;
        let keys = procedure_rendezvous_keys_for_notam(
            &xml,
            Some("STAR"),
            None,
            Some(
                "STAR HNL DANIEL K. INOUYE INTL AIRPORT, HONOLULU HI STAR KAENA FIVE \
                 (KAENA5) STAR NOT AVAILABLE. TRANSIT CORRIDOR THROUGH WARNING AREAS \
                 W-189A, W-189B, AND W-190 UNAVAILABLE DUE TO PARTICIPATING MILITARY \
                 OPERATIONS.",
            ),
        )?;

        assert!(keys.contains(&ProcedureRendezvousKey::shared_arrival("KAENA5").unwrap()));
        Ok(())
    }

    fn synthetic_geoid_height_feet(latitude: i32, longitude: i32) -> i32 {
        latitude * 2 + longitude
    }

    fn synthetic_geo_grid_fixture() -> anyhow::Result<NamedTempFile> {
        let mut fixture = NamedTempFile::new().context("failed to create synthetic geo grid")?;
        {
            let mut writer = BufWriter::new(fixture.as_file_mut());
            for latitude in GeoidGrid::MIN_LAT..GeoidGrid::MAX_LAT_EXCLUSIVE {
                for longitude in GeoidGrid::MIN_LON..GeoidGrid::MAX_LON_EXCLUSIVE {
                    writeln!(
                        writer,
                        "{latitude},{longitude},{},0",
                        synthetic_geoid_height_feet(latitude, longitude)
                    )?;
                }
            }
            writer.flush()?;
        }
        Ok(fixture)
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
    fn nms_star_text_accepts_faa_arrival_spelling_and_layout_variants() {
        for (text, expected) in [
            (
                "STAR BOZEMAN YELLOWSTONE INTERNATIONAL. BGMAN ONE ARRIVAL...NOT AVBL",
                "BGMAN ONE",
            ),
            (
                "STAR HOUSTON. DOOBI THREE RNAV ARR... ADD NOTE: DO NOT FILE.",
                "DOOBI THREE",
            ),
            (
                "STAR NASHVILLE. RYYMN THREE (RNAV) ARRIVAL...TURNT TRANSITION NOT AVBL",
                "RYYMN THREE",
            ),
            (
                "STAR ORLANDO. GTOUT ONE RNAV\nARRIVAL..CROSS NICCK AT 12000.",
                "GTOUT ONE",
            ),
            (
                "STAR PROVO. TAYTR THREE ARRIVAL..CHANGE ARRIVAL ROUTE DESCRIPTION",
                "TAYTR THREE",
            ),
            (
                "STAR GROSSE ILE MUNICIPAL AIRPORT, DETROIT/GROSSE ILE, MI\n PETTE TWO ARRIVAL...HOSSA TRANSITION",
                "PETTE TWO",
            ),
            (
                "STAR SARDI ONE ARRIVAL. DISREGARD NOTE: STAR LIMITED TO JET AIRCRAFT.",
                "SARDI ONE",
            ),
        ] {
            assert_eq!(
                textual_published_procedure_names(text, &["ARR", "ARRIVAL"]),
                BTreeSet::from([expected.to_string()]),
                "failed to parse {text:?}",
            );
        }
        assert!(textual_published_procedure_names(
            "STAR SPECIAL IAP, USCG SAN DIEGO, COPTER RNAV (GPS) 007, ORIG",
            &["ARR", "ARRIVAL"],
        )
        .is_empty());
        assert!(textual_star_cifp_ids(
            "STAR SPECIAL IAP, USCG SAN DIEGO, COPTER RNAV (GPS) 007, ORIG"
        )
        .is_empty());
    }

    #[test]
    fn tfr_altitude_parser_ignores_effective_dates_before_msl_to_flight_level_limits() {
        let text = "2603080900-2611010959 END PART 1 OF 2 \
                    390803N1212615W 4100FT MSL-FL180 \
                    EFFECTIVE 2603080900 UTC UNTIL 2611010959 UTC";

        let (lower, upper) = tfr_altitude_limits_from_text(text).expect("TFR altitude limits");

        assert_eq!(lower.value_text, "4100");
        assert_eq!(lower.unit, "FT");
        assert_eq!(upper.value_text, "180");
        assert_eq!(upper.unit, "FL");
        assert!(
            tfr_altitude_limits_from_text("EFFECTIVE 2603080900 UTC UNTIL 2611010959 UTC")
                .is_none()
        );
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
            nms_id: None,
            source_type: Some("F".to_string()),
            notam_status: Some("PUBLISHED".to_string()),
            notam_function: Some("N".to_string()),
            notam_keyword: Some("AIRSPACE".to_string()),
            last_updated_utc: None,
            location_designator: Some(facility.to_string()),
            icao_id: None,
            airport_id: None,
            airport_effects: BTreeSet::new(),
            procedure_rendezvous_keys: BTreeSet::new(),
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
    fn pirep_dataset_is_keyed_stable_and_independent_from_metars() -> anyhow::Result<()> {
        let temp = TempDir::new().context("failed to create temp dir")?;
        let first = temp.path().join("first-pireps.xml");
        let second = temp.path().join("second-pireps.xml");
        let report_a = r#"<AircraftReport><observation_time>2026-08-14T16:00:00.000Z</observation_time><latitude>47.5</latitude><longitude>-122.3</longitude><report_type>PIREP</report_type><raw_text>SEA UA /OV SEA/TM 1600/FL080/IC LGT/TB NEG</raw_text></AircraftReport>"#;
        let report_b = r#"<AircraftReport><observation_time>2026-08-14T16:05:00.000Z</observation_time><latitude>46.9</latitude><longitude>-121.8</longitude><report_type>AIREP</report_type><raw_text>SEA UUA /OV SEA/TM 1605/FL120/IC NEG/TB MOD</raw_text></AircraftReport>"#;
        fs::write(
            &first,
            format!(
                r#"<?xml version="1.0"?><response><data>{report_b}{report_a}</data></response>"#
            ),
        )?;
        fs::write(
            &second,
            format!(
                r#"<?xml version="1.0"?><response><data>{report_a}{report_b}</data></response>"#
            ),
        )?;

        let first_fingerprint = pirep_content_fingerprint(&first)?;
        let second_fingerprint = pirep_content_fingerprint(&second)?;
        assert_eq!(first_fingerprint, second_fingerprint);
        let version_label = first_fingerprint.chars().take(16).collect::<String>();
        let generated_at_utc =
            DateTime::parse_from_rfc3339("2026-08-14T16:10:00Z")?.with_timezone(&Utc);
        let first_result = build_pirep_dataset(&BuildPirepRequest {
            pirep_xml_path: first,
            output_dir: temp.path().join("first-out"),
            version_label: version_label.clone(),
            generated_at_utc,
        })?;
        let second_result = build_pirep_dataset(&BuildPirepRequest {
            pirep_xml_path: second,
            output_dir: temp.path().join("second-out"),
            version_label,
            generated_at_utc,
        })?;

        assert_eq!(
            fs::read(&first_result.structured_json_path)?,
            fs::read(&second_result.structured_json_path)?,
        );
        let dataset: Value =
            serde_json::from_slice(&fs::read(&first_result.structured_json_path)?)?;
        let records = dataset
            .get("pireps_by_id")
            .and_then(Value::as_object)
            .context("PIREP dataset should be keyed by stable report id")?;
        assert_eq!(records.len(), 2);
        assert_eq!(dataset["pirep_count"], 2);
        assert!(dataset.get("metars_by_station").is_none());
        assert!(records
            .values()
            .any(|record| record["symbol"] == "light-icing"));
        assert!(records
            .values()
            .any(|record| record["symbol"] == "moderate-turbulence"));
        let manifest: Value = serde_json::from_slice(&fs::read(&first_result.manifest_path)?)?;
        assert_eq!(manifest["files"]["pireps"], "pireps.json");
        assert_eq!(manifest["counts"]["pireps"], 2);
        Ok(())
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
    fn domestic_notam_number_includes_local_series() {
        assert_eq!(
            local_format_notam_number(Some("!STL 08/430 8WC RWY 20 RWY END ID LGT U/S")),
            Some("08/430".to_string())
        );
        assert_eq!(
            local_format_notam_number(Some("!FDC 6/6721 MVY IAP MARTHAS VINEYARD")),
            Some("6/6721".to_string())
        );
    }

    #[test]
    fn geoid_grid_interpolates_geo_fixture() -> anyhow::Result<()> {
        let fixture = synthetic_geo_grid_fixture()?;
        let grid = GeoidGrid::from_geo_csv(fixture.path())?;
        assert_eq!(
            grid.geoid_height_feet_bilinear(-90.0, -180.0),
            f64::from(synthetic_geoid_height_feet(-90, -180))
        );
        assert_eq!(
            grid.geoid_height_feet_bilinear(89.0, 179.0),
            f64::from(synthetic_geoid_height_feet(89, 179))
        );

        let west = grid.geoid_height_feet_bilinear(40.0, -122.0);
        let east = grid.geoid_height_feet_bilinear(40.0, -121.0);
        let midpoint = grid.geoid_height_feet_bilinear(40.0, -121.5);
        assert!((midpoint - ((west + east) / 2.0)).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn terrain_transform_adds_geoid_height_after_meter_to_feet_conversion() -> anyhow::Result<()> {
        let fixture = synthetic_geo_grid_fixture()?;
        let grid = GeoidGrid::from_geo_csv(fixture.path())?;
        let transformed =
            terrain_ellipsoid_height_feet_from_navd88_meters(100.0, -90.0, -180.0, &grid);
        let expected = 100.0 * 3.280_839_895 + f64::from(synthetic_geoid_height_feet(-90, -180));
        assert!((transformed - expected).abs() < 0.0001);
        Ok(())
    }
}
