use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context};
use chrono::{DateTime, NaiveDateTime, Utc};
use image::ImageReader;
use metar::{CloudDensity, Data, Metar, VerticalVisibility};
use preprocessor_zip::{write_deterministic_zip, ZipSource};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

const METAR_TILE_ZOOM: u8 = 7;
const METAR_TILE_PATH_TEMPLATE: &str = "points/metars/{z}/{x}/{y}.json";
const METAR_TILE_SIZE: u16 = 256;

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

#[derive(Debug, Clone)]
pub struct BuildMetarParityRequest {
    pub input_xml_path: PathBuf,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct BuildMetarParityResult {
    pub output_path: PathBuf,
    pub metar_count: usize,
}

#[derive(Debug, Clone)]
pub struct BuildNexradParityRequest {
    pub input_dir: PathBuf,
    pub output_dir: PathBuf,
    pub generated_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct BuildNexradParityResult {
    pub manifest_path: PathBuf,
    pub latest_txt_path: PathBuf,
    pub png_paths: Vec<PathBuf>,
    pub zip_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct BuildMetarRequest {
    pub input_xml_path: PathBuf,
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
pub struct BuildNexradRequest {
    pub input_dir: PathBuf,
    pub output_dir: PathBuf,
    pub version_label: String,
    pub generated_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct BuildNexradResult {
    pub manifest_path: PathBuf,
    pub structured_json_path: PathBuf,
    pub zip_path: PathBuf,
    pub frame_count: usize,
}

#[derive(Debug, Clone)]
pub struct BuildGeoRequest {
    pub source_csv_path: PathBuf,
    pub output_dir: PathBuf,
    pub version_label: String,
    pub generated_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct BuildGeoResult {
    pub manifest_path: PathBuf,
    pub csv_path: PathBuf,
    pub zip_path: PathBuf,
    pub point_count: usize,
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
    map_view: MetarManifestMapView,
    counts: MetarManifestCounts,
}

#[derive(Debug, Clone, Serialize)]
struct MetarManifestFiles {
    structured_json: String,
    zip: String,
}

#[derive(Debug, Clone, Serialize)]
struct MetarManifestMapView {
    layer: String,
    storage_kind: String,
    tile_path_template: String,
    tile_size: u16,
    min_zoom: u8,
    max_zoom: u8,
    levels: Vec<MetarManifestTileLevel>,
}

#[derive(Debug, Clone, Serialize)]
struct MetarManifestTileLevel {
    zoom: u8,
    tile_count: usize,
    max_records_per_tile: usize,
}

#[derive(Debug, Clone, Serialize)]
struct MetarManifestCounts {
    metars: usize,
    tile_count: usize,
    max_metars_per_tile: usize,
}

#[derive(Debug, Clone, Serialize)]
struct NexradManifest {
    schema_version: u32,
    version_label: String,
    generated_at_utc: String,
    files: NexradManifestFiles,
    counts: NexradManifestCounts,
}

#[derive(Debug, Clone, Serialize)]
struct NexradManifestFiles {
    structured_json: String,
    zip: String,
}

#[derive(Debug, Clone, Serialize)]
struct NexradManifestCounts {
    frames: usize,
}

#[derive(Debug, Clone, Serialize)]
struct GeoManifest {
    schema_version: u32,
    version_label: String,
    generated_at_utc: String,
    grid: GeoManifestGrid,
    files: GeoManifestFiles,
    counts: GeoManifestCounts,
}

#[derive(Debug, Clone, Serialize)]
struct GeoManifestGrid {
    latitude_step_degrees: i32,
    longitude_step_degrees: i32,
    value_units: GeoManifestValueUnits,
}

#[derive(Debug, Clone, Serialize)]
struct GeoManifestValueUnits {
    geoid_height: String,
    magnetic_declination: String,
}

#[derive(Debug, Clone, Serialize)]
struct GeoManifestFiles {
    csv: String,
    zip: String,
}

#[derive(Debug, Clone, Serialize)]
struct GeoManifestCounts {
    points: usize,
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

#[derive(Debug, Clone)]
struct RadarListingEntry {
    file_name: String,
    observed_at_utc: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredMetarDataset {
    schema_version: u32,
    version_label: String,
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
    ceiling: Option<StructuredMetarCeiling>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredMetarCeiling {
    amount: String,
    height_ft_agl: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
struct MetarTileFile {
    schema_version: u32,
    layer: String,
    z: u8,
    x: u32,
    y: u32,
    records: Vec<MetarTileReference>,
}

#[derive(Debug, Clone, Serialize)]
struct MetarTileReference {
    station_id: String,
}

#[derive(Debug, Clone, Serialize)]
struct MetarTileRecord {
    z: u8,
    x: u32,
    y: u32,
    records: Vec<MetarTileReference>,
}

#[derive(Debug, Clone, Serialize)]
struct MetarProductModel {
    metars_by_station: BTreeMap<String, StructuredMetarRecord>,
    tiles: Vec<MetarTileRecord>,
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

#[derive(Debug, Clone, Serialize)]
pub struct StructuredNexradDataset {
    schema_version: u32,
    version_label: String,
    frame_count: usize,
    projection: String,
    frames: Vec<StructuredNexradFrame>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredNexradFrame {
    filename: String,
    observed_at_utc: String,
    width: u32,
    height: u32,
    bounds: StructuredNexradBounds,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredNexradBounds {
    west: f64,
    south: f64,
    east: f64,
    north: f64,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct GeoGridPoint {
    latitude: i32,
    longitude: i32,
    geoid_height_feet: i32,
    magnetic_declination_degrees: i32,
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

    pub fn from_avare_geo_csv(path: &Path) -> anyhow::Result<Self> {
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
        .map(|area| StructuredTfrArea {
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

pub fn build_metar_avare_parity_artifacts(
    request: &BuildMetarParityRequest,
) -> anyhow::Result<BuildMetarParityResult> {
    if request.output_dir.exists() {
        fs::remove_dir_all(&request.output_dir)
            .with_context(|| format!("failed to clear {}", request.output_dir.display()))?;
    }
    fs::create_dir_all(&request.output_dir)
        .with_context(|| format!("failed to create {}", request.output_dir.display()))?;

    let xml = fs::read_to_string(&request.input_xml_path)
        .with_context(|| format!("failed to read {}", request.input_xml_path.display()))?;
    let mut reader = Reader::from_str(&xml);
    let mut buffer = Vec::new();
    let mut output = String::new();
    let mut metar_count = 0usize;
    let mut in_metar = false;
    let mut mode = None;
    let mut raw_text = String::new();
    let mut observation_time = String::new();
    let mut latitude = String::new();
    let mut longitude = String::new();
    let mut station_id = String::new();
    let mut flight_category = String::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => match event.name().as_ref() {
                b"METAR" => {
                    in_metar = true;
                    raw_text.clear();
                    observation_time.clear();
                    latitude.clear();
                    longitude.clear();
                    station_id.clear();
                    flight_category.clear();
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
                    if !raw_text.is_empty() && !flight_category.is_empty() {
                        output.push_str(&flight_category);
                        output.push(',');
                        output.push_str(&raw_text.replace('\n', ""));
                        output.push('\n');
                        metar_count += 1;
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
                match mode {
                    Some(MetarTextMode::RawText) => raw_text.push_str(&text),
                    Some(MetarTextMode::ObservationTime) => observation_time = text,
                    Some(MetarTextMode::Latitude) => latitude = text,
                    Some(MetarTextMode::Longitude) => longitude = text,
                    Some(MetarTextMode::StationId) => station_id = text,
                    Some(MetarTextMode::FlightCategory) => flight_category = text,
                    None => {}
                }
            }
            Ok(Event::CData(event)) if in_metar => {
                let text = event
                    .xml_content()
                    .context("failed to decode METAR XML cdata")?
                    .into_owned();
                match mode {
                    Some(MetarTextMode::RawText) => raw_text.push_str(&text),
                    Some(MetarTextMode::ObservationTime) => observation_time = text,
                    Some(MetarTextMode::Latitude) => latitude = text,
                    Some(MetarTextMode::Longitude) => longitude = text,
                    Some(MetarTextMode::StationId) => station_id = text,
                    Some(MetarTextMode::FlightCategory) => flight_category = text,
                    None => {}
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to parse {}", request.input_xml_path.display())
                });
            }
        }
        buffer.clear();
    }

    let output_path = request.output_dir.join("metars.txt");
    fs::write(&output_path, output)
        .with_context(|| format!("failed to write {}", output_path.display()))?;
    Ok(BuildMetarParityResult {
        output_path,
        metar_count,
    })
}

pub fn build_metar_dataset(request: &BuildMetarRequest) -> anyhow::Result<BuildMetarResult> {
    if request.output_dir.exists() {
        fs::remove_dir_all(&request.output_dir)
            .with_context(|| format!("failed to clear {}", request.output_dir.display()))?;
    }
    fs::create_dir_all(&request.output_dir)
        .with_context(|| format!("failed to create {}", request.output_dir.display()))?;

    let model = metar_product_model(&request.input_xml_path)?;
    let metar_count = model.metars_by_station.len();
    let tile_count = model.tiles.len();
    let max_metars_per_tile = model
        .tiles
        .iter()
        .map(|tile| tile.records.len())
        .max()
        .unwrap_or(0);

    let structured_json_path = request.output_dir.join("metars.json");
    let manifest_path = request
        .output_dir
        .join(format!("metars_{}.manifest.json", request.version_label));
    let zip_path = request
        .output_dir
        .join(format!("metars_{}.zip", request.version_label));

    write_json_pretty(
        &structured_json_path,
        &StructuredMetarDataset {
            schema_version: 2,
            version_label: request.version_label.clone(),
            metar_count,
            metars_by_station: model.metars_by_station.clone(),
        },
    )?;
    let mut zip_members = vec![("metars.json".to_string(), structured_json_path.clone())];
    for tile in &model.tiles {
        let relative_path = metar_tile_relative_path(tile.z, tile.x, tile.y);
        let tile_path = request.output_dir.join(&relative_path);
        if let Some(parent) = tile_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        write_json_pretty(
            &tile_path,
            &MetarTileFile {
                schema_version: 1,
                layer: "metars".to_string(),
                z: tile.z,
                x: tile.x,
                y: tile.y,
                records: tile.records.clone(),
            },
        )?;
        zip_members.push((relative_path, tile_path));
    }
    write_json_pretty(
        &manifest_path,
        &MetarManifest {
            schema_version: 2,
            version_label: request.version_label.clone(),
            generated_at_utc: request.generated_at_utc.to_rfc3339(),
            files: MetarManifestFiles {
                structured_json: "metars.json".to_string(),
                zip: zip_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string(),
            },
            map_view: MetarManifestMapView {
                layer: "metars".to_string(),
                storage_kind: "fast_product".to_string(),
                tile_path_template: METAR_TILE_PATH_TEMPLATE.to_string(),
                tile_size: METAR_TILE_SIZE,
                min_zoom: METAR_TILE_ZOOM,
                max_zoom: METAR_TILE_ZOOM,
                levels: vec![MetarManifestTileLevel {
                    zoom: METAR_TILE_ZOOM,
                    tile_count,
                    max_records_per_tile: max_metars_per_tile,
                }],
            },
            counts: MetarManifestCounts {
                metars: metar_count,
                tile_count,
                max_metars_per_tile,
            },
        },
    )?;
    let zip_member_refs = zip_members
        .iter()
        .map(|(relative_path, path)| (relative_path.as_str(), path.as_path()))
        .collect::<Vec<_>>();
    write_zip(&zip_path, &zip_member_refs)?;

    Ok(BuildMetarResult {
        manifest_path,
        structured_json_path,
        zip_path,
        metar_count,
    })
}

pub fn metar_content_fingerprint(input_xml_path: &Path) -> anyhow::Result<String> {
    let model = metar_product_model(input_xml_path)?;
    let bytes = serde_json::to_vec(&model).context("failed to encode canonical METAR model")?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

fn metar_product_model(input_xml_path: &Path) -> anyhow::Result<MetarProductModel> {
    let mut metars_by_station = BTreeMap::new();
    for mut record in structured_metar_records(input_xml_path)? {
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
    let tiles = build_metar_tiles(&metars_by_station);
    Ok(MetarProductModel {
        metars_by_station,
        tiles,
    })
}

fn build_metar_tiles(
    metars_by_station: &BTreeMap<String, StructuredMetarRecord>,
) -> Vec<MetarTileRecord> {
    let mut tiles: BTreeMap<(u8, u32, u32), Vec<MetarTileReference>> = BTreeMap::new();
    for (station_id, record) in metars_by_station {
        let (Some(latitude), Some(longitude)) = (record.latitude, record.longitude) else {
            continue;
        };
        if !metar_has_valid_position(latitude, longitude) {
            continue;
        }
        let (x, y) = metar_slippy_tile(latitude, longitude, METAR_TILE_ZOOM);
        tiles
            .entry((METAR_TILE_ZOOM, x, y))
            .or_default()
            .push(MetarTileReference {
                station_id: station_id.clone(),
            });
    }
    tiles
        .into_iter()
        .map(|((z, x, y), records)| MetarTileRecord { z, x, y, records })
        .collect()
}

fn metar_tile_relative_path(z: u8, x: u32, y: u32) -> String {
    format!("points/metars/{z}/{x}/{y}.json")
}

fn metar_has_valid_position(latitude: f64, longitude: f64) -> bool {
    latitude.is_finite()
        && longitude.is_finite()
        && (-85.051_128_78..=85.051_128_78).contains(&latitude)
        && (-180.0..=180.0).contains(&longitude)
}

fn metar_slippy_tile(latitude: f64, longitude: f64, zoom: u8) -> (u32, u32) {
    let lat_rad = latitude.to_radians();
    let tile_count = 2_f64.powi(i32::from(zoom));
    let x = ((longitude + 180.0) / 360.0) * tile_count;
    let y = (1.0 - ((lat_rad.tan() + (1.0 / lat_rad.cos())).ln() / PI)) / 2.0 * tile_count;
    (metar_clamp_tile(x, zoom), metar_clamp_tile(y, zoom))
}

fn metar_clamp_tile(value: f64, zoom: u8) -> u32 {
    let max_tile = (1_u32 << zoom) - 1;
    value.floor().clamp(0.0, f64::from(max_tile)) as u32
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

fn structured_metar_clouds(raw_text: &str) -> StructuredMetarClouds {
    let ceiling = Metar::parse(raw_text)
        .ok()
        .and_then(|metar| metar_ceiling(&metar));
    StructuredMetarClouds { ceiling }
}

fn metar_ceiling(metar: &Metar) -> Option<StructuredMetarCeiling> {
    if let Some(vertical_visibility) = metar.vert_visibility {
        return Some(match vertical_visibility {
            VerticalVisibility::Distance(height) => StructuredMetarCeiling {
                amount: "VV".to_string(),
                height_ft_agl: Some(height * 100),
            },
            VerticalVisibility::ReducedByUnknownAmount => StructuredMetarCeiling {
                amount: "VV".to_string(),
                height_ft_agl: None,
            },
        });
    }

    metar.cloud_layers.iter().find_map(|layer| {
        let amount = match layer.density {
            Data::Known(CloudDensity::Broken) => "BKN",
            Data::Known(CloudDensity::Overcast) => "OVC",
            _ => return None,
        };
        Some(StructuredMetarCeiling {
            amount: amount.to_string(),
            height_ft_agl: match layer.height {
                Data::Known(height) => Some(height * 100),
                Data::Unknown => None,
            },
        })
    })
}

pub fn build_nexrad_avare_parity_artifacts(
    request: &BuildNexradParityRequest,
) -> anyhow::Result<BuildNexradParityResult> {
    if request.output_dir.exists() {
        fs::remove_dir_all(&request.output_dir)
            .with_context(|| format!("failed to clear {}", request.output_dir.display()))?;
    }
    fs::create_dir_all(&request.output_dir)
        .with_context(|| format!("failed to create {}", request.output_dir.display()))?;

    let listings = parse_radar_listing(&request.input_dir.join("index.html"))?;
    if listings.len() < 11 {
        bail!(
            "expected at least 11 radar listings, found {}",
            listings.len()
        );
    }

    let selected = [0usize, 5usize, 10usize]
        .into_iter()
        .map(|index| listings[index].clone())
        .collect::<Vec<_>>();

    let manifest_path = request.output_dir.join("conus");
    let latest_txt_path = request.output_dir.join("latest.txt");
    let zip_path = request.output_dir.join("conus.zip");
    let image_bases = ["latest_radaronly", "latest_radaronly1", "latest_radaronly2"];
    let mut manifest = format!(
        "{}\nlatest.txt\n",
        request.generated_at_utc.format("%m_%d_%Y_%H:%M_UTC")
    );
    let mut latest_txt = String::new();
    let mut png_paths = Vec::new();
    let corner_labels = ["Upper Left", "Lower Left", "Upper Right", "Lower Right"];

    for (entry, image_base) in selected.iter().zip(image_bases) {
        let copied_gz_path = request.output_dir.join(&entry.file_name);
        let source_gz_path = request.input_dir.join(&entry.file_name);
        fs::copy(&source_gz_path, &copied_gz_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                source_gz_path.display(),
                copied_gz_path.display()
            )
        })?;
        run_command("gzip", &["-d", copied_gz_path.to_str().unwrap()])?;

        let source_tif_name = entry.file_name.trim_end_matches(".gz");
        let source_tif_path = request.output_dir.join(source_tif_name);
        let warped_tif_path = request.output_dir.join(format!("{image_base}.tif"));
        let png_path = request.output_dir.join(format!("{image_base}.png"));

        run_command(
            "gdalwarp",
            &[
                "-r",
                "near",
                "-s_srs",
                "EPSG:4326",
                "-t_srs",
                "EPSG:3857",
                "-of",
                "gtiff",
                source_tif_path.to_str().unwrap(),
                warped_tif_path.to_str().unwrap(),
            ],
        )?;
        run_command(
            "convert",
            &[
                warped_tif_path.to_str().unwrap(),
                "-transparent",
                "black",
                "-resize",
                "25%",
                png_path.to_str().unwrap(),
            ],
        )?;

        let gdalinfo_output =
            run_command_capture("gdalinfo", &[warped_tif_path.to_str().unwrap(), "-noct"])?;
        for label in corner_labels {
            let line = gdalinfo_output
                .lines()
                .find(|line| line.trim_start().starts_with(label))
                .ok_or_else(|| anyhow::anyhow!("missing {label} in gdalinfo output"))?;
            let start = line
                .rfind('(')
                .ok_or_else(|| anyhow::anyhow!("missing '(' in gdalinfo output line: {line}"))?;
            let end = line[start + 1..]
                .find(')')
                .ok_or_else(|| anyhow::anyhow!("missing ')' in gdalinfo output line: {line}"))?;
            latest_txt.push_str(&line[start + 1..start + 1 + end]);
            latest_txt.push('\n');
        }
        latest_txt.push_str(&entry.observed_at_utc.format("%Y%m%d_%H%M").to_string());
        latest_txt.push('\n');

        manifest.push_str(&format!("{image_base}.png\n"));
        png_paths.push(png_path);
    }

    fs::write(&manifest_path, manifest)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    fs::write(&latest_txt_path, latest_txt)
        .with_context(|| format!("failed to write {}", latest_txt_path.display()))?;

    let mut members = vec![
        ("conus", manifest_path.as_path()),
        ("latest.txt", latest_txt_path.as_path()),
    ];
    for png_path in &png_paths {
        let name = png_path.file_name().and_then(|name| name.to_str()).unwrap();
        members.push((name, png_path.as_path()));
    }
    write_zip(&zip_path, &members)?;

    Ok(BuildNexradParityResult {
        manifest_path,
        latest_txt_path,
        png_paths,
        zip_path,
    })
}

pub fn build_nexrad_dataset(request: &BuildNexradRequest) -> anyhow::Result<BuildNexradResult> {
    if request.output_dir.exists() {
        fs::remove_dir_all(&request.output_dir)
            .with_context(|| format!("failed to clear {}", request.output_dir.display()))?;
    }
    fs::create_dir_all(&request.output_dir)
        .with_context(|| format!("failed to create {}", request.output_dir.display()))?;

    let listings = parse_radar_listing(&request.input_dir.join("index.html"))?;
    if listings.len() < 11 {
        bail!(
            "expected at least 11 radar listings, found {}",
            listings.len()
        );
    }

    let selected = [0usize, 5usize, 10usize]
        .into_iter()
        .map(|index| listings[index].clone())
        .collect::<Vec<_>>();

    let structured_json_path = request.output_dir.join("nexrad.json");
    let manifest_path = request
        .output_dir
        .join(format!("nexrad_{}.manifest.json", request.version_label));
    let zip_path = request
        .output_dir
        .join(format!("nexrad_{}.zip", request.version_label));

    let frame_names = ["frame_0.png", "frame_1.png", "frame_2.png"];
    let mut frames = Vec::new();
    let mut zip_members: Vec<(String, PathBuf)> =
        vec![("nexrad.json".to_string(), structured_json_path.clone())];

    for (entry, frame_name) in selected.iter().zip(frame_names) {
        let copied_gz_path = request.output_dir.join(&entry.file_name);
        let source_gz_path = request.input_dir.join(&entry.file_name);
        fs::copy(&source_gz_path, &copied_gz_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                source_gz_path.display(),
                copied_gz_path.display()
            )
        })?;
        run_command("gzip", &["-d", copied_gz_path.to_str().unwrap()])?;

        let source_tif_name = entry.file_name.trim_end_matches(".gz");
        let source_tif_path = request.output_dir.join(source_tif_name);
        let warped_tif_path = request
            .output_dir
            .join(frame_name.trim_end_matches(".png").to_string() + ".tif");
        let png_path = request.output_dir.join(frame_name);

        run_command(
            "gdalwarp",
            &[
                "-r",
                "near",
                "-s_srs",
                "EPSG:4326",
                "-t_srs",
                "EPSG:3857",
                "-of",
                "gtiff",
                source_tif_path.to_str().unwrap(),
                warped_tif_path.to_str().unwrap(),
            ],
        )?;
        run_command(
            "convert",
            &[
                warped_tif_path.to_str().unwrap(),
                "-transparent",
                "black",
                "-resize",
                "25%",
                png_path.to_str().unwrap(),
            ],
        )?;

        let (west, south, east, north) = warped_bounds(&warped_tif_path)?;
        let image = ImageReader::open(&png_path)
            .with_context(|| format!("failed to open {}", png_path.display()))?
            .with_guessed_format()
            .context("failed to guess png format")?
            .decode()
            .with_context(|| format!("failed to decode {}", png_path.display()))?;
        frames.push(StructuredNexradFrame {
            filename: frame_name.to_string(),
            observed_at_utc: entry.observed_at_utc.and_utc().to_rfc3339(),
            width: image.width(),
            height: image.height(),
            bounds: StructuredNexradBounds {
                west,
                south,
                east,
                north,
            },
        });
        zip_members.push((frame_name.to_string(), png_path));
    }

    write_json_pretty(
        &structured_json_path,
        &StructuredNexradDataset {
            schema_version: 1,
            version_label: request.version_label.clone(),
            frame_count: frames.len(),
            projection: "EPSG:3857".to_string(),
            frames,
        },
    )?;
    write_json_pretty(
        &manifest_path,
        &NexradManifest {
            schema_version: 1,
            version_label: request.version_label.clone(),
            generated_at_utc: request.generated_at_utc.to_rfc3339(),
            files: NexradManifestFiles {
                structured_json: "nexrad.json".to_string(),
                zip: zip_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string(),
            },
            counts: NexradManifestCounts {
                frames: zip_members.len() - 1,
            },
        },
    )?;
    let member_refs = zip_members
        .iter()
        .map(|(name, path)| (name.as_str(), path.as_path()))
        .collect::<Vec<_>>();
    write_zip(&zip_path, &member_refs)?;

    Ok(BuildNexradResult {
        manifest_path,
        structured_json_path,
        zip_path,
        frame_count: zip_members.len() - 1,
    })
}

pub fn build_geo_dataset(request: &BuildGeoRequest) -> anyhow::Result<BuildGeoResult> {
    if request.output_dir.exists() {
        fs::remove_dir_all(&request.output_dir)
            .with_context(|| format!("failed to clear {}", request.output_dir.display()))?;
    }
    fs::create_dir_all(&request.output_dir)
        .with_context(|| format!("failed to create {}", request.output_dir.display()))?;

    let points = parse_geo_csv(&request.source_csv_path)?;
    let csv_path = request.output_dir.join("geo.csv");
    let manifest_path = request
        .output_dir
        .join(format!("geo_{}.manifest.json", request.version_label));
    let zip_path = request
        .output_dir
        .join(format!("geo_{}.zip", request.version_label));

    write_geo_csv(&csv_path, &points)?;
    write_json_pretty(
        &manifest_path,
        &GeoManifest {
            schema_version: 1,
            version_label: request.version_label.clone(),
            generated_at_utc: request.generated_at_utc.to_rfc3339(),
            grid: GeoManifestGrid {
                latitude_step_degrees: 1,
                longitude_step_degrees: 1,
                value_units: GeoManifestValueUnits {
                    geoid_height: "feet_msl_offset_rounded".to_string(),
                    magnetic_declination: "degrees_east_rounded".to_string(),
                },
            },
            files: GeoManifestFiles {
                csv: "geo.csv".to_string(),
                zip: zip_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string(),
            },
            counts: GeoManifestCounts {
                points: points.len(),
            },
        },
    )?;
    write_zip(&zip_path, &[("geo.csv", &csv_path)])?;

    Ok(BuildGeoResult {
        manifest_path,
        csv_path,
        zip_path,
        point_count: points.len(),
    })
}

pub fn build_notam_dataset(request: &BuildNotamRequest) -> anyhow::Result<BuildNotamResult> {
    if request.output_dir.exists() {
        fs::remove_dir_all(&request.output_dir)
            .with_context(|| format!("failed to clear {}", request.output_dir.display()))?;
    }
    fs::create_dir_all(&request.output_dir)
        .with_context(|| format!("failed to create {}", request.output_dir.display()))?;

    let notams = structured_notam_records(&request.input_jsonl_path)?;
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
                magnetic_declination_degrees: parse_geo_i32(
                    path,
                    index,
                    "magnetic declination",
                    columns[3],
                )?,
            })
        })
        .collect()
}

fn structured_notam_records(input_jsonl_path: &Path) -> anyhow::Result<Vec<StructuredNotamRecord>> {
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

fn write_geo_csv(path: &Path, points: &[GeoGridPoint]) -> anyhow::Result<()> {
    let mut output = String::new();
    for point in points {
        output.push_str(&format!(
            "{},{},{},{}\n",
            point.latitude,
            point.longitude,
            point.geoid_height_feet,
            point.magnetic_declination_degrees
        ));
    }
    fs::write(path, output).with_context(|| format!("failed to write {}", path.display()))
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

fn parse_radar_listing(path: &Path) -> anyhow::Result<Vec<RadarListingEntry>> {
    let html =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut entries = html
        .lines()
        .filter_map(|line| {
            let start = line.find("CONUS_L2_CREF_QCD_")?;
            let tail = &line[start..];
            let end = tail.find(".tif.gz")?;
            Some(&tail[..end + ".tif.gz".len()])
        })
        .filter_map(|name| {
            let suffix = name.strip_prefix("CONUS_L2_CREF_QCD_")?;
            let (date, time_with_ext) = suffix.split_once('_')?;
            let time = time_with_ext.strip_suffix(".tif.gz")?;
            let observed_at_utc =
                NaiveDateTime::parse_from_str(&(date.to_string() + time), "%Y%m%d%H%M%S").ok()?;
            Some(RadarListingEntry {
                file_name: name.to_string(),
                observed_at_utc,
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| right.observed_at_utc.cmp(&left.observed_at_utc));
    Ok(entries)
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

fn warped_bounds(path: &Path) -> anyhow::Result<(f64, f64, f64, f64)> {
    let output = run_command_capture("gdalinfo", &[path.to_str().unwrap(), "-noct"])?;
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for label in ["Upper Left", "Lower Left", "Upper Right", "Lower Right"] {
        let line = output
            .lines()
            .find(|line| line.trim_start().starts_with(label))
            .ok_or_else(|| anyhow::anyhow!("missing {label} in gdalinfo output"))?;
        let start = line
            .find('(')
            .ok_or_else(|| anyhow::anyhow!("missing '(' in gdalinfo output line: {line}"))?;
        let end = line[start + 1..]
            .find(')')
            .ok_or_else(|| anyhow::anyhow!("missing ')' in gdalinfo output line: {line}"))?;
        let coords = &line[start + 1..start + 1 + end];
        let (x, y) = coords
            .split_once(',')
            .ok_or_else(|| anyhow::anyhow!("missing comma in gdalinfo coords: {coords}"))?;
        xs.push(
            x.trim()
                .parse::<f64>()
                .with_context(|| format!("failed to parse x {x}"))?,
        );
        ys.push(
            y.trim()
                .parse::<f64>()
                .with_context(|| format!("failed to parse y {y}"))?,
        );
    }
    Ok((
        xs.iter().copied().fold(f64::INFINITY, f64::min),
        ys.iter().copied().fold(f64::INFINITY, f64::min),
        xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    ))
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
                avare_text: build_tfr_avare_text(
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

fn build_tfr_avare_text(
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
                        current_group.avare_text.push_str("Eff ");
                        current_group.avare_text.push_str(&text);
                        current_group.avare_text.push(' ');
                        current_group
                            .schedule_fragments
                            .push(StructuredTfrScheduleFragment {
                                kind: "effective".to_string(),
                                value_utc: text,
                            });
                    }
                    Some(TextMode::DateExpire) => {
                        current_group.avare_text.push_str("Exp ");
                        current_group.avare_text.push_str(&text);
                        current_group.avare_text.push(' ');
                        current_group
                            .schedule_fragments
                            .push(StructuredTfrScheduleFragment {
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

fn run_command(program: &str, args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("failed to execute {program}"))?;
    if !status.success() {
        bail!("{program} exited with status {status}");
    }
    Ok(())
}

fn run_command_capture(program: &str, args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to execute {program}"))?;
    if !output.status.success() {
        bail!("{program} exited with status {}", output.status);
    }
    String::from_utf8(output.stdout).context("command output was not valid utf-8")
}

#[cfg(test)]
fn run_command_capture_metric(program: &str, args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to execute {program}"))?;
    if !output.status.success() {
        bail!("{program} exited with status {}", output.status);
    }
    let stderr = String::from_utf8(output.stderr).context("command stderr was not valid utf-8")?;
    Ok(stderr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn avare_parity_fixture_root(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("preprocessor-fast crate should live under product/preprocessor")
            .join("test_fixtures")
            .join("avare_parity")
            .join(name)
    }

    #[test]
    fn avare_fixture_parity() -> anyhow::Result<()> {
        let fixture_root = avare_parity_fixture_root("tfr_parity");
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
        })
        .expect_err("projected coordinates should fail loudly");
        assert!(format!("{error:#}").contains("not lon/lat"));
        Ok(())
    }

    #[test]
    fn metar_fixture_parity() -> anyhow::Result<()> {
        let fixture_root = avare_parity_fixture_root("metar_parity");
        let output_dir = TempDir::new().context("failed to create temp dir")?;
        let result = build_metar_avare_parity_artifacts(&BuildMetarParityRequest {
            input_xml_path: fixture_root.join("input").join("metars.cache.xml"),
            output_dir: output_dir.path().join("out"),
        })?;

        assert_eq!(
            fs::read_to_string(fixture_root.join("expected").join("avare_handler.txt"))?,
            fs::read_to_string(&result.output_path)?,
        );
        assert!(result.metar_count > 0);
        Ok(())
    }

    #[test]
    fn metar_dataset_is_stable_across_source_reordering() -> anyhow::Result<()> {
        let temp = TempDir::new().context("failed to create temp dir")?;
        let first = temp.path().join("first.xml");
        let second = temp.path().join("second.xml");
        let record_a = r#"<METAR><raw_text>METAR KAAA 160400Z 00000KT 10SM CLR 10/08 A3000</raw_text><station_id>KAAA</station_id><observation_time>2026-04-16T04:00:00.000Z</observation_time><latitude>1.0</latitude><longitude>2.0</longitude><flight_category>VFR</flight_category></METAR>"#;
        let record_b = r#"<METAR><raw_text>METAR KBBB 160355Z 18005KT 10SM SCT020 BKN025 12/09 A3001</raw_text><station_id>KBBB</station_id><observation_time>2026-04-16T03:55:00.000Z</observation_time><latitude>3.0</latitude><longitude>4.0</longitude><flight_category>MVFR</flight_category></METAR>"#;
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

        let first_fingerprint = metar_content_fingerprint(&first)?;
        let second_fingerprint = metar_content_fingerprint(&second)?;
        assert_eq!(first_fingerprint, second_fingerprint);
        let version_label = first_fingerprint.chars().take(16).collect::<String>();
        let generated_at_utc = DateTime::parse_from_rfc3339("2026-04-16T04:00:00Z")
            .expect("valid fixture timestamp")
            .with_timezone(&Utc);
        let first_result = build_metar_dataset(&BuildMetarRequest {
            input_xml_path: first,
            output_dir: temp.path().join("first-out"),
            version_label: version_label.clone(),
            generated_at_utc,
        })?;
        let second_result = build_metar_dataset(&BuildMetarRequest {
            input_xml_path: second,
            output_dir: temp.path().join("second-out"),
            version_label,
            generated_at_utc,
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
            dataset.pointer("/metars_by_station/KAAA/observed_at_utc"),
            Some(&Value::String("2026-04-16T04:00:00.000Z".to_string())),
        );
        assert_eq!(
            dataset.pointer("/metars_by_station/KBBB/flight_category"),
            Some(&Value::String("MVFR".to_string())),
        );
        assert_eq!(
            dataset.pointer("/metars_by_station/KBBB/clouds/ceiling/amount"),
            Some(&Value::String("BKN".to_string())),
        );
        assert_eq!(
            dataset
                .pointer("/metars_by_station/KBBB/clouds/ceiling/height_ft_agl")
                .and_then(|value| value.as_u64()),
            Some(2500),
        );

        let manifest: Value = serde_json::from_slice(&fs::read(&first_result.manifest_path)?)?;
        assert_eq!(
            manifest
                .pointer("/map_view/tile_path_template")
                .and_then(|value| value.as_str()),
            Some(METAR_TILE_PATH_TEMPLATE),
        );
        assert_eq!(
            manifest
                .pointer("/map_view/storage_kind")
                .and_then(|value| value.as_str()),
            Some("fast_product"),
        );
        assert_eq!(
            manifest
                .pointer("/map_view/min_zoom")
                .and_then(|value| value.as_u64()),
            Some(u64::from(METAR_TILE_ZOOM)),
        );
        assert_eq!(
            manifest
                .pointer("/map_view/max_zoom")
                .and_then(|value| value.as_u64()),
            Some(u64::from(METAR_TILE_ZOOM)),
        );
        assert_eq!(
            manifest
                .pointer("/map_view/levels/0/zoom")
                .and_then(|value| value.as_u64()),
            Some(u64::from(METAR_TILE_ZOOM)),
        );
        assert_eq!(
            manifest
                .pointer("/map_view/levels/0/max_records_per_tile")
                .and_then(|value| value.as_u64()),
            manifest
                .pointer("/counts/max_metars_per_tile")
                .and_then(|value| value.as_u64()),
        );
        assert!(
            manifest
                .pointer("/counts/tile_count")
                .and_then(|value| value.as_u64())
                .unwrap_or(0)
                > 0
        );
        assert!(
            manifest
                .pointer("/counts/max_metars_per_tile")
                .and_then(|value| value.as_u64())
                .unwrap_or(0)
                > 0
        );

        let (x, y) = metar_slippy_tile(1.0, 2.0, METAR_TILE_ZOOM);
        let tile_path = first_result
            .zip_path
            .parent()
            .unwrap()
            .join(metar_tile_relative_path(METAR_TILE_ZOOM, x, y));
        let tile: Value = serde_json::from_slice(&fs::read(tile_path)?)?;
        assert_eq!(
            tile.pointer("/records/0/station_id")
                .and_then(|value| value.as_str()),
            Some("KAAA")
        );
        Ok(())
    }

    #[test]
    fn nexrad_fixture_parity() -> anyhow::Result<()> {
        let fixture_root = avare_parity_fixture_root("nexrad_parity");
        let output_dir = TempDir::new().context("failed to create temp dir")?;
        let generated_at_utc = DateTime::parse_from_rfc3339("2026-04-16T01:29:00Z")
            .expect("valid fixture timestamp")
            .with_timezone(&Utc);
        let result = build_nexrad_avare_parity_artifacts(&BuildNexradParityRequest {
            input_dir: fixture_root.join("input"),
            output_dir: output_dir.path().join("out"),
            generated_at_utc,
        })?;

        assert_eq!(
            fs::read_to_string(fixture_root.join("expected").join("conus"))?,
            fs::read_to_string(&result.manifest_path)?,
        );
        assert_eq!(
            fs::read_to_string(fixture_root.join("expected").join("latest.txt"))?,
            fs::read_to_string(&result.latest_txt_path)?,
        );
        for png_name in [
            "latest_radaronly.png",
            "latest_radaronly1.png",
            "latest_radaronly2.png",
        ] {
            let expected_png = fixture_root.join("expected").join(png_name);
            let actual_png = output_dir.path().join("out").join(png_name);
            assert_eq!(
                "0",
                run_command_capture_metric(
                    "compare",
                    &[
                        "-metric",
                        "AE",
                        actual_png.to_str().unwrap(),
                        expected_png.to_str().unwrap(),
                        "null:",
                    ],
                )?
                .trim(),
            );
        }
        assert!(result.zip_path.exists());
        Ok(())
    }

    #[test]
    fn geo_fixture_parity() -> anyhow::Result<()> {
        let fixture_root = avare_parity_fixture_root("geo_parity");
        let output_dir = TempDir::new().context("failed to create temp dir")?;
        let generated_at_utc = DateTime::parse_from_rfc3339("2026-04-16T00:00:00Z")
            .expect("valid fixture timestamp")
            .with_timezone(&Utc);
        let result = build_geo_dataset(&BuildGeoRequest {
            source_csv_path: fixture_root.join("input").join("geo.csv"),
            output_dir: output_dir.path().join("out"),
            version_label: "fixture".to_string(),
            generated_at_utc,
        })?;

        assert_eq!(
            fs::read(fixture_root.join("expected").join("geo.csv"))?,
            fs::read(&result.csv_path)?,
        );
        assert_eq!(64_800, result.point_count);
        Ok(())
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
    fn geoid_grid_interpolates_avare_geo_fixture() -> anyhow::Result<()> {
        let fixture_root = avare_parity_fixture_root("geo_parity");
        let grid = GeoidGrid::from_avare_geo_csv(&fixture_root.join("input").join("geo.csv"))?;
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
        let fixture_root = avare_parity_fixture_root("geo_parity");
        let grid = GeoidGrid::from_avare_geo_csv(&fixture_root.join("input").join("geo.csv"))?;
        let transformed =
            terrain_ellipsoid_height_feet_from_navd88_meters(100.0, -90.0, -180.0, &grid);
        assert!((transformed - 298.0839895).abs() < 0.0001);
        Ok(())
    }
}
