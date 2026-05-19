use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{bail, Context};
use chrono::{DateTime, SecondsFormat, Timelike, Utc};
use preprocessor_fetch::{
    hash_file, prefetch_archives_with_provenance, prefetch_requests_with_provenance,
    FetchCacheConfig, FetchCacheMode, PrefetchRequest,
};
use preprocessor_vectors::{build_obstacle_dataset, BuildObstacleDatasetRequest};
use preprocessor_zip::{write_deterministic_zip, ZipSource};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    build_metar_dataset, build_tfr_dataset,
    engine::{
        read_json_value, sha256_hex, write_json_pretty_file, BuiltLiveFeedState, DeltaPolicy,
        LiveFeedStatePayload, ProductBuilder, UpstreamEvent,
    },
    metar_content_fingerprint, BuildMetarRequest, BuildTfrRequest,
};

const METAR_XML_URL: &str = "https://aviationweather.gov/data/cache/metars.cache.xml.gz";
const TAF_XML_URL: &str = "https://aviationweather.gov/data/cache/tafs.cache.xml.gz";
const PIREP_XML_URL: &str = "https://aviationweather.gov/data/cache/aircraftreports.cache.xml.gz";
const TFR_LIST_URL: &str = "https://tfr.faa.gov/tfrapi/exportTfrList";
const TFR_GRAPHICS_URL: &str = concat!(
    "https://tfr.faa.gov/geoserver/TFR/ows?",
    "service=WFS&version=1.1.0&request=GetFeature&typeName=TFR:V_TFR_LOC&",
    "maxFeatures=300&outputFormat=application/json&srsname=EPSG:4326"
);
const OBSTACLE_DOF_URL: &str = "https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP";
const NEXRAD_INDEX_URL: &str = "https://mrms.ncep.noaa.gov/data/RIDGEII/L2/CONUS/CREF_QCD/";

pub const WINDS_ALOFT_FORECAST_HOURS: &[u32] = &[0, 3, 6, 9, 12];
pub const WINDS_ALOFT_PRESSURE_LEVELS_MB: &[u32] = &[1000, 925, 850, 700, 600, 500, 400, 300];

#[derive(Debug, Clone)]
pub struct LiveFeedFetchConfig {
    pub fetch_jobs: usize,
    pub fetch_cache: Option<FetchCacheConfig>,
}

impl LiveFeedFetchConfig {
    pub fn new(fetch_jobs: usize, fetch_cache: Option<FetchCacheConfig>) -> Self {
        Self {
            fetch_jobs: fetch_jobs.max(1),
            fetch_cache,
        }
    }

    pub fn cache_first(fetch_jobs: usize, root: PathBuf) -> Self {
        Self::new(
            fetch_jobs,
            Some(FetchCacheConfig {
                root,
                mode: FetchCacheMode::CacheFirst,
            }),
        )
    }
}

pub fn json_live_feed_state(
    product: &str,
    version: String,
    state_source_path: PathBuf,
    state_value: Value,
    delta_policy: DeltaPolicy,
    changed_count_if_no_delta: usize,
) -> BuiltLiveFeedState {
    BuiltLiveFeedState {
        product: product.to_string(),
        version,
        payload: LiveFeedStatePayload::JsonFile {
            path: state_source_path,
            value: state_value,
        },
        delta_policy,
        changed_count_if_no_delta,
    }
}

pub fn directory_live_feed_state(
    product: &str,
    version: String,
    state_source_dir: PathBuf,
    manifest_source_path: PathBuf,
    manifest_value: Value,
    changed_count_if_no_delta: usize,
) -> BuiltLiveFeedState {
    BuiltLiveFeedState {
        product: product.to_string(),
        version,
        payload: LiveFeedStatePayload::Directory {
            root: state_source_dir,
            manifest_path: manifest_source_path,
            manifest_value,
        },
        delta_policy: DeltaPolicy::None,
        changed_count_if_no_delta,
    }
}

pub fn nexrad_live_feed_state(
    version: String,
    state_source_dir: PathBuf,
    manifest_source_path: PathBuf,
    manifest_value: Value,
) -> anyhow::Result<BuiltLiveFeedState> {
    let tile_count = live_nexrad_tile_count(&manifest_value)?;
    Ok(directory_live_feed_state(
        "nexrad",
        version,
        state_source_dir,
        manifest_source_path,
        manifest_value,
        tile_count,
    ))
}

pub fn live_nexrad_tile_count(manifest: &Value) -> anyhow::Result<usize> {
    let levels = manifest
        .get("levels")
        .and_then(Value::as_array)
        .context("NEXRAD source-grid manifest missing levels")?;
    Ok(levels
        .iter()
        .map(|level| {
            let cols = level.get("tile_cols").and_then(Value::as_u64).unwrap_or(0);
            let rows = level.get("tile_rows").and_then(Value::as_u64).unwrap_or(0);
            (cols * rows) as usize
        })
        .sum())
}

pub fn stage_live_feed_input_file(
    source_path: &Path,
    input_dir: &Path,
    env_name: &str,
) -> anyhow::Result<PathBuf> {
    let file_name = source_path
        .file_name()
        .with_context(|| format!("{env_name} must name a file"))?;
    let source_path = source_path.canonicalize().with_context(|| {
        format!(
            "failed to resolve {env_name} path {}",
            source_path.display()
        )
    })?;
    fs::create_dir_all(input_dir)
        .with_context(|| format!("failed to create {}", input_dir.display()))?;
    let staged_path = input_dir.join(file_name);
    if staged_path
        .canonicalize()
        .ok()
        .is_some_and(|path| path == source_path)
    {
        return Ok(staged_path);
    }
    if staged_path.exists() {
        fs::remove_file(&staged_path)
            .with_context(|| format!("failed to remove stale {}", staged_path.display()))?;
    }
    if let Err(link_error) = fs::hard_link(&source_path, &staged_path) {
        fs::copy(&source_path, &staged_path).with_context(|| {
            format!(
                "failed to stage {} at {} after hard-link error: {link_error}",
                source_path.display(),
                staged_path.display()
            )
        })?;
    }
    Ok(staged_path)
}

pub fn parse_nexrad_observed_at_utc(file_name: &str) -> anyhow::Result<chrono::NaiveDateTime> {
    let suffix = file_name
        .strip_prefix("CONUS_L2_CREF_QCD_")
        .with_context(|| format!("unexpected NEXRAD source filename: {file_name}"))?;
    let (date, time_with_ext) = suffix
        .split_once('_')
        .with_context(|| format!("unexpected NEXRAD source filename: {file_name}"))?;
    let time = time_with_ext
        .strip_suffix(".tif.gz")
        .with_context(|| format!("unexpected NEXRAD source filename: {file_name}"))?;
    chrono::NaiveDateTime::parse_from_str(&(date.to_string() + time), "%Y%m%d%H%M%S")
        .with_context(|| format!("failed to parse NEXRAD observed time from {file_name}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GfsWindsAloftCycle {
    pub date: String,
    pub cycle: String,
    pub cycle_time_utc: DateTime<Utc>,
}

pub fn selected_gfs_winds_aloft_cycle(now: DateTime<Utc>) -> GfsWindsAloftCycle {
    let candidate = now - chrono::Duration::hours(9);
    let cycle_hour = (candidate.hour() / 6) * 6;
    let cycle_time_utc = candidate
        .date_naive()
        .and_hms_opt(cycle_hour, 0, 0)
        .expect("rounded GFS cycle time should be valid")
        .and_utc();
    GfsWindsAloftCycle {
        date: cycle_time_utc.format("%Y%m%d").to_string(),
        cycle: format!("{cycle_hour:02}"),
        cycle_time_utc,
    }
}

pub fn gfs_winds_aloft_filter_url(cycle: &GfsWindsAloftCycle, forecast_hour: u32) -> String {
    let mut url = format!(
        "https://nomads.ncep.noaa.gov/cgi-bin/filter_gfs_0p25.pl?dir=%2Fgfs.{}%2F{}%2Fatmos&file=gfs.t{}z.pgrb2.0p25.f{forecast_hour:03}",
        cycle.date, cycle.cycle, cycle.cycle
    );
    for variable in ["UGRD", "VGRD", "HGT"] {
        url.push_str("&var_");
        url.push_str(variable);
        url.push_str("=on");
    }
    for level in WINDS_ALOFT_PRESSURE_LEVELS_MB {
        url.push_str("&lev_");
        url.push_str(&level.to_string());
        url.push_str("_mb=on");
    }
    url.push_str("&subregion=&toplat=55&leftlon=225&rightlon=310&bottomlat=15");
    url
}

#[derive(Debug, Clone)]
pub struct MetarLiveFeedBuilder {
    fetch: LiveFeedFetchConfig,
}

impl MetarLiveFeedBuilder {
    pub fn new(fetch: LiveFeedFetchConfig) -> Self {
        Self { fetch }
    }
}

impl ProductBuilder for MetarLiveFeedBuilder {
    fn product_id(&self) -> &str {
        "metars"
    }

    fn build_state(
        &self,
        event: &UpstreamEvent,
        scratch_dir: &Path,
    ) -> anyhow::Result<BuiltLiveFeedState> {
        let generated_at_utc = normalized_event_time(event.observed_at_utc);
        let input_dir = fresh_dir(&scratch_dir.join("input"))?;
        let output_dir = fresh_dir(&scratch_dir.join("output"))?;
        let provenance_dir = scratch_dir.join("meta").join("provenance").join("metars");
        let requests = vec![
            PrefetchRequest::new(METAR_XML_URL).with_logical_file_name("metars.cache.xml.gz"),
            PrefetchRequest::new(TAF_XML_URL).with_logical_file_name("tafs.cache.xml.gz"),
            PrefetchRequest::new(PIREP_XML_URL)
                .with_logical_file_name("aircraftreports.cache.xml.gz"),
        ];
        prefetch_archives_with_provenance(
            &requests,
            &input_dir,
            self.fetch.fetch_jobs,
            self.fetch.fetch_cache.as_ref(),
            &provenance_dir,
            "metars",
        )?;
        for file_name in [
            "metars.cache.xml.gz",
            "tafs.cache.xml.gz",
            "aircraftreports.cache.xml.gz",
        ] {
            run_gzip_decompress(&input_dir.join(file_name))?;
        }
        let metar_xml_path = input_dir.join("metars.cache.xml");
        let taf_xml_path = input_dir.join("tafs.cache.xml");
        let pirep_xml_path = input_dir.join("aircraftreports.cache.xml");
        let fingerprint =
            metar_content_fingerprint(&metar_xml_path, &taf_xml_path, &pirep_xml_path)?;
        let version = content_version_label(&fingerprint);
        let result = build_metar_dataset(&BuildMetarRequest {
            metar_xml_path,
            taf_xml_path,
            pirep_xml_path,
            output_dir,
            version_label: version.clone(),
            generated_at_utc,
        })?;
        let state_value = read_json_value(&result.structured_json_path)?;
        Ok(json_live_feed_state(
            "metars",
            version,
            result.structured_json_path,
            state_value,
            DeltaPolicy::KeyedRecords {
                records_key: "metars_by_station".to_string(),
                count_key: Some("metar_count".to_string()),
            },
            result.metar_count,
        ))
    }
}

#[derive(Debug, Clone)]
pub struct TfrLiveFeedBuilder {
    fetch: LiveFeedFetchConfig,
}

impl TfrLiveFeedBuilder {
    pub fn new(fetch: LiveFeedFetchConfig) -> Self {
        Self { fetch }
    }
}

impl ProductBuilder for TfrLiveFeedBuilder {
    fn product_id(&self) -> &str {
        "tfrs"
    }

    fn build_state(
        &self,
        event: &UpstreamEvent,
        scratch_dir: &Path,
    ) -> anyhow::Result<BuiltLiveFeedState> {
        let generated_at_utc = normalized_event_time(event.observed_at_utc);
        let input_dir = fresh_dir(&scratch_dir.join("input"))?;
        let output_dir = fresh_dir(&scratch_dir.join("output"))?;
        let provenance_dir = scratch_dir.join("meta").join("provenance").join("tfrs");
        let requests = vec![
            PrefetchRequest::new(TFR_LIST_URL).with_logical_file_name("list.json"),
            PrefetchRequest::new(TFR_GRAPHICS_URL).with_logical_file_name("graphics.geojson"),
        ];
        prefetch_requests_with_provenance(
            &requests,
            &input_dir,
            self.fetch.fetch_jobs,
            self.fetch.fetch_cache.as_ref(),
            &provenance_dir,
            "tfrs",
        )?;
        let fingerprint = hash_tree(&input_dir)?;
        let version = content_version_label(&fingerprint);
        let result = build_tfr_dataset(&BuildTfrRequest {
            input_dir,
            output_dir,
            version_label: version.clone(),
            generated_at_utc,
        })?;
        let state_value = read_json_value(&result.structured_json_path)?;
        Ok(json_live_feed_state(
            "tfrs",
            version,
            result.structured_json_path,
            state_value,
            DeltaPolicy::None,
            result.area_group_count,
        ))
    }
}

#[derive(Debug, Clone)]
pub struct WindsAloftLiveFeedBuilder {
    fetch: LiveFeedFetchConfig,
}

impl WindsAloftLiveFeedBuilder {
    pub fn new(fetch: LiveFeedFetchConfig) -> Self {
        Self { fetch }
    }
}

impl ProductBuilder for WindsAloftLiveFeedBuilder {
    fn product_id(&self) -> &str {
        "winds-aloft"
    }

    fn build_state(
        &self,
        event: &UpstreamEvent,
        scratch_dir: &Path,
    ) -> anyhow::Result<BuiltLiveFeedState> {
        let cycle = selected_gfs_winds_aloft_cycle(event.observed_at_utc);
        let input_dir = fresh_dir(&scratch_dir.join("input"))?;
        let output_dir = fresh_dir(&scratch_dir.join("output"))?;
        let provenance_dir = scratch_dir
            .join("meta")
            .join("provenance")
            .join("winds-aloft");
        let requests = WINDS_ALOFT_FORECAST_HOURS
            .iter()
            .map(|forecast_hour| {
                PrefetchRequest::new(gfs_winds_aloft_filter_url(&cycle, *forecast_hour))
                    .with_logical_file_name(format!(
                        "gfs_{}_{}_f{forecast_hour:03}.grib2",
                        cycle.date, cycle.cycle
                    ))
            })
            .collect::<Vec<_>>();
        prefetch_requests_with_provenance(
            &requests,
            &input_dir,
            self.fetch.fetch_jobs,
            self.fetch.fetch_cache.as_ref(),
            &provenance_dir,
            "winds-aloft",
        )?;
        let fingerprint = hash_tree(&input_dir)?;
        let version = content_version_label(&fingerprint);
        let state_path =
            build_winds_aloft_state_from_inputs(&input_dir, &output_dir, &cycle, &version)?;
        let state_value = read_json_value(&state_path)?;
        let file_count = state_value
            .get("files")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        Ok(json_live_feed_state(
            "winds-aloft",
            version,
            state_path,
            state_value,
            DeltaPolicy::None,
            file_count,
        ))
    }
}

#[derive(Debug, Clone)]
pub struct ObstaclesLiveFeedBuilder {
    fetch: LiveFeedFetchConfig,
}

impl ObstaclesLiveFeedBuilder {
    pub fn new(fetch: LiveFeedFetchConfig) -> Self {
        Self { fetch }
    }
}

impl ProductBuilder for ObstaclesLiveFeedBuilder {
    fn product_id(&self) -> &str {
        "obstacles"
    }

    fn build_state(
        &self,
        _event: &UpstreamEvent,
        scratch_dir: &Path,
    ) -> anyhow::Result<BuiltLiveFeedState> {
        let input_dir = fresh_dir(&scratch_dir.join("input"))?;
        let output_dir = fresh_dir(&scratch_dir.join("output"))?;
        let provenance_dir = scratch_dir
            .join("meta")
            .join("provenance")
            .join("obstacles");
        let requests = vec![
            PrefetchRequest::new(OBSTACLE_DOF_URL).with_logical_file_name("DAILY_DOF_DAT.ZIP")
        ];
        prefetch_archives_with_provenance(
            &requests,
            &input_dir,
            self.fetch.fetch_jobs,
            self.fetch.fetch_cache.as_ref(),
            &provenance_dir,
            "obstacles",
        )?;
        let fingerprint = hash_tree(&input_dir)?;
        let version = content_version_label(&fingerprint);
        let result = build_obstacle_dataset(&BuildObstacleDatasetRequest {
            input_dir,
            output_dir,
            version_label: version.clone(),
        })?;
        let state_value = read_json_value(&result.structured_json_path)?;
        let obstacle_count = state_value
            .get("obstacles_by_id")
            .and_then(Value::as_object)
            .map_or(0, serde_json::Map::len);
        Ok(json_live_feed_state(
            "obstacles",
            version,
            result.structured_json_path,
            state_value,
            DeltaPolicy::KeyedRecords {
                records_key: "obstacles_by_id".to_string(),
                count_key: Some("obstacle_count".to_string()),
            },
            obstacle_count,
        ))
    }
}

#[derive(Debug, Clone)]
pub struct NexradSourceGridLiveFeedBuilder {
    fetch: LiveFeedFetchConfig,
    debug_lat_lon_grid: bool,
}

impl NexradSourceGridLiveFeedBuilder {
    pub fn new(fetch: LiveFeedFetchConfig, debug_lat_lon_grid: bool) -> Self {
        Self {
            fetch,
            debug_lat_lon_grid,
        }
    }
}

impl ProductBuilder for NexradSourceGridLiveFeedBuilder {
    fn product_id(&self) -> &str {
        "nexrad"
    }

    fn build_state(
        &self,
        event: &UpstreamEvent,
        scratch_dir: &Path,
    ) -> anyhow::Result<BuiltLiveFeedState> {
        let input_dir = fresh_dir(&scratch_dir.join("input"))?;
        let output_dir = fresh_dir(&scratch_dir.join("output"))?;
        let source_path = if let Some(payload_path) = &event.payload_path {
            stage_live_feed_input_file(payload_path, &input_dir, "NEXRAD payload")?
        } else {
            fetch_latest_nexrad_source(&input_dir, scratch_dir, &self.fetch)?
        };
        let source_file = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .context("NEXRAD source path has no utf-8 file name")?
            .to_string();
        let observed_at = parse_nexrad_observed_at_utc(&source_file)?.and_utc();
        let source_sha256 = hash_file(&source_path)?;
        let palette_hash = hash_text(NEXRAD_FIXED_OPAQUE_PALETTE_JSON);
        let version = format!(
            "{}_{}_png8{}",
            observed_at.format("%Y%m%dT%H%M%SZ"),
            &source_sha256[..16],
            &palette_hash[..8]
        );
        build_nexrad_source_grid_tiles(
            &source_path,
            &output_dir,
            &version,
            &observed_at.to_rfc3339(),
            &source_file,
            &source_sha256,
            self.debug_lat_lon_grid,
        )?;
        let manifest_path = output_dir.join("manifest.json");
        let manifest_value = read_json_value(&manifest_path)?;
        nexrad_live_feed_state(version, output_dir, manifest_path, manifest_value)
    }
}

pub const NEXRAD_FIXED_OPAQUE_PALETTE_JSON: &str =
    include_str!("../../../../docs/nexrad/analysis/whole-day-greedy-255-palette.json");

const NEXRAD_SOURCE_GRID_TILE_SCRIPT: &str = include_str!("nexrad_source_grid_tiles.py");

pub fn build_nexrad_source_grid_tiles(
    source_gz_path: &Path,
    output_dir: &Path,
    version: &str,
    observed_at_utc: &str,
    source_file: &str,
    source_sha256: &str,
    debug_lat_lon_grid: bool,
) -> anyhow::Result<()> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let script_path = output_dir.join("build_nexrad_source_grid_tiles.py");
    let palette_path = output_dir.join("nexrad_fixed_palette.json");
    fs::write(&script_path, NEXRAD_SOURCE_GRID_TILE_SCRIPT)
        .with_context(|| format!("failed to write {}", script_path.display()))?;
    fs::write(&palette_path, NEXRAD_FIXED_OPAQUE_PALETTE_JSON)
        .with_context(|| format!("failed to write {}", palette_path.display()))?;
    let output = Command::new("python3")
        .arg(&script_path)
        .arg("--palette")
        .arg(&palette_path)
        .arg("--source-gz")
        .arg(source_gz_path)
        .arg("--output-dir")
        .arg(output_dir)
        .arg("--state-id")
        .arg(version)
        .arg("--observed-at-utc")
        .arg(observed_at_utc)
        .arg("--source-file")
        .arg(source_file)
        .arg("--source-sha256")
        .arg(source_sha256)
        .arg("--tile-size")
        .arg("512")
        .arg("--res-level")
        .arg("0")
        .arg("--res-level")
        .arg("1")
        .arg("--res-level")
        .arg("2")
        .arg("--res-level")
        .arg("3")
        .args(debug_lat_lon_grid.then_some("--debug-lat-lon-grid"))
        .output()
        .with_context(|| format!("failed to run {}", script_path.display()))?;
    if !output.status.success() {
        bail!(
            "NEXRAD source-grid tiler failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::remove_file(&script_path)
        .with_context(|| format!("failed to remove {}", script_path.display()))?;
    fs::remove_file(&palette_path)
        .with_context(|| format!("failed to remove {}", palette_path.display()))?;
    Ok(())
}

fn fetch_latest_nexrad_source(
    input_dir: &Path,
    scratch_dir: &Path,
    fetch: &LiveFeedFetchConfig,
) -> anyhow::Result<PathBuf> {
    let provenance_dir = scratch_dir.join("meta").join("provenance").join("nexrad");
    let index_request = PrefetchRequest::new(NEXRAD_INDEX_URL)
        .with_logical_file_name("index.html")
        .allow_html();
    prefetch_requests_with_provenance(
        std::slice::from_ref(&index_request),
        input_dir,
        fetch.fetch_jobs,
        fetch.fetch_cache.as_ref(),
        &provenance_dir,
        "nexrad-index",
    )?;
    let latest = parse_latest_nexrad_listing(&input_dir.join("index.html"))?;
    let request = PrefetchRequest::new(format!("{NEXRAD_INDEX_URL}{latest}"))
        .with_logical_file_name(latest.clone());
    prefetch_archives_with_provenance(
        &[request],
        input_dir,
        fetch.fetch_jobs,
        fetch.fetch_cache.as_ref(),
        &provenance_dir,
        "nexrad-frame",
    )?;
    Ok(input_dir.join(latest))
}

pub fn parse_latest_nexrad_listing(index_path: &Path) -> anyhow::Result<String> {
    let text = fs::read_to_string(index_path)
        .with_context(|| format!("failed to read {}", index_path.display()))?;
    let mut listings = Vec::new();
    for token in text
        .split(|ch: char| ch == '"' || ch == '\'' || ch == '<' || ch == '>' || ch.is_whitespace())
    {
        if token.starts_with("CONUS_L2_CREF_QCD_") && token.ends_with(".tif.gz") {
            listings.push(token.to_string());
        }
    }
    listings.sort();
    listings.dedup();
    listings.pop().with_context(|| {
        format!(
            "{} contained no NEXRAD CREF_QCD listings",
            index_path.display()
        )
    })
}

fn build_winds_aloft_state_from_inputs(
    input_dir: &Path,
    output_dir: &Path,
    cycle: &GfsWindsAloftCycle,
    version: &str,
) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let structured_json_path = output_dir.join("winds-aloft.json");
    let manifest_path = output_dir.join(format!("winds-aloft_{version}.manifest.json"));
    let zip_path = output_dir.join(format!("winds-aloft_{version}.zip"));
    let grib_output_dir = output_dir.join("grib2");
    fs::create_dir_all(&grib_output_dir)
        .with_context(|| format!("failed to create {}", grib_output_dir.display()))?;
    let mut members = Vec::new();
    let grib_files = WINDS_ALOFT_FORECAST_HOURS
        .iter()
        .map(|forecast_hour| {
            let file_name = format!(
                "gfs_{}_{}_f{forecast_hour:03}.grib2",
                cycle.date, cycle.cycle
            );
            let source_path = input_dir.join(&file_name);
            let size_bytes = fs::metadata(&source_path)
                .with_context(|| format!("failed to stat {}", source_path.display()))?
                .len();
            let staged_path = grib_output_dir.join(&file_name);
            copy_or_link(&source_path, &staged_path)?;
            members.push(ZipSource::new(format!("grib2/{file_name}"), &staged_path));
            Ok(serde_json::json!({
                "forecast_hour": forecast_hour,
                "path": format!("grib2/{file_name}"),
                "size_bytes": size_bytes,
            }))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let generated_at_utc = cycle
        .cycle_time_utc
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    let manifest = serde_json::json!({
        "schema_version": 1,
        "product_id": "winds-aloft",
        "source": "NOAA/NCEP GFS 0.25 degree via NOMADS filtered GRIB2",
        "version_label": version,
        "generated_at_utc": generated_at_utc,
        "model": {
            "id": "gfs",
            "grid": "0.25-degree",
            "cycle_date": cycle.date,
            "cycle": cycle.cycle,
            "cycle_time_utc": generated_at_utc,
        },
        "domain": {
            "lat_min": 15.0,
            "lat_max": 55.0,
            "lon_min": -135.0,
            "lon_max": -50.0,
        },
        "forecast_hours": WINDS_ALOFT_FORECAST_HOURS,
        "pressure_levels_mb": WINDS_ALOFT_PRESSURE_LEVELS_MB,
        "variables": ["UGRD", "VGRD", "HGT"],
        "files": grib_files,
        "notes": [
            "Raw measuring state; not yet a client rendering wire format.",
            "UGRD/VGRD are wind vector components. HGT is included to map pressure levels to geometric altitude."
        ],
    });
    write_json_pretty_file(&structured_json_path, &manifest)?;
    write_json_pretty_file(&manifest_path, &manifest)?;
    members.push(ZipSource::new("manifest.json", &structured_json_path));
    write_deterministic_zip(&zip_path, &members)?;
    Ok(structured_json_path)
}

fn normalized_event_time(time: DateTime<Utc>) -> DateTime<Utc> {
    time.with_second(0)
        .expect("zero seconds should be valid")
        .with_nanosecond(0)
        .expect("zero nanos should be valid")
}

fn fresh_dir(path: &Path) -> anyhow::Result<PathBuf> {
    if path.exists() {
        fs::remove_dir_all(path).with_context(|| format!("failed to clear {}", path.display()))?;
    }
    fs::create_dir_all(path).with_context(|| format!("failed to create {}", path.display()))?;
    Ok(path.to_path_buf())
}

fn run_gzip_decompress(path: &Path) -> anyhow::Result<()> {
    let status = Command::new("gzip")
        .arg("-d")
        .arg(path)
        .status()
        .with_context(|| format!("failed to run gzip on {}", path.display()))?;
    if !status.success() {
        bail!("gzip failed for {}", path.display());
    }
    Ok(())
}

fn copy_or_link(source: &Path, target: &Path) -> anyhow::Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    if target.exists() {
        fs::remove_file(target)
            .with_context(|| format!("failed to remove {}", target.display()))?;
    }
    if let Err(link_error) = fs::hard_link(source, target) {
        fs::copy(source, target).with_context(|| {
            format!(
                "failed to copy {} to {} after hard-link error: {link_error}",
                source.display(),
                target.display()
            )
        })?;
    }
    Ok(())
}

pub fn hash_tree(root: &Path) -> anyhow::Result<String> {
    let mut entries = Vec::new();
    collect_hash_tree_entries(root, root, &mut entries)?;
    entries.sort();
    let mut hasher = Sha256::new();
    for (relative, hash) in entries {
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update(hash.as_bytes());
        hasher.update([0xff]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_hash_tree_entries(
    root: &Path,
    path: &Path,
    entries: &mut Vec<(String, String)>,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_hash_tree_entries(root, &path, entries)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .with_context(|| format!("{} is not under {}", path.display(), root.display()))?
                .to_string_lossy()
                .replace('\\', "/");
            entries.push((relative, hash_file(&path)?));
        }
    }
    Ok(())
}

pub fn hash_text(value: &str) -> String {
    sha256_hex(value.as_bytes())
}

pub fn content_version_label(source_fingerprint: &str) -> String {
    source_fingerprint.chars().take(16).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use tempfile::tempdir;

    #[test]
    fn winds_aloft_cycle_selection_uses_conservative_gfs_lag() {
        let now = DateTime::parse_from_rfc3339("2026-05-09T15:10:00Z")
            .expect("valid test timestamp")
            .with_timezone(&Utc);

        let cycle = selected_gfs_winds_aloft_cycle(now);

        assert_eq!(cycle.date, "20260509");
        assert_eq!(cycle.cycle, "06");
        assert_eq!(
            cycle
                .cycle_time_utc
                .to_rfc3339_opts(SecondsFormat::Secs, true),
            "2026-05-09T06:00:00Z"
        );
    }

    #[test]
    fn winds_aloft_filter_url_selects_bounded_gfs_slice() {
        let cycle = GfsWindsAloftCycle {
            date: "20260509".to_string(),
            cycle: "06".to_string(),
            cycle_time_utc: Utc.with_ymd_and_hms(2026, 5, 9, 6, 0, 0).unwrap(),
        };

        let url = gfs_winds_aloft_filter_url(&cycle, 3);

        assert!(url.contains("filter_gfs_0p25.pl"));
        assert!(url.contains("dir=%2Fgfs.20260509%2F06%2Fatmos"));
        assert!(url.contains("file=gfs.t06z.pgrb2.0p25.f003"));
        for fragment in [
            "var_UGRD=on",
            "var_VGRD=on",
            "var_HGT=on",
            "lev_1000_mb=on",
            "lev_300_mb=on",
            "toplat=55",
            "bottomlat=15",
            "leftlon=225",
            "rightlon=310",
        ] {
            assert!(url.contains(fragment), "{url} missing {fragment}");
        }
    }

    #[test]
    fn source_override_staging_does_not_create_fixture_sidecars() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let fixture_dir = temp.path().join("fixture");
        let input_dir = temp.path().join("input");
        fs::create_dir_all(&fixture_dir)?;
        let source_path = fixture_dir.join("CONUS_L2_CREF_QCD_20260511_211842.tif.gz");
        fs::write(&source_path, b"fixture bytes")?;

        let staged_path = stage_live_feed_input_file(&source_path, &input_dir, "test fixture")?;

        assert_eq!(
            staged_path,
            input_dir.join("CONUS_L2_CREF_QCD_20260511_211842.tif.gz")
        );
        assert_eq!(fs::read(&staged_path)?, b"fixture bytes");
        assert!(
            !fixture_dir
                .join("CONUS_L2_CREF_QCD_20260511_211842.tif.gz.properties")
                .exists(),
            "staging must not create live-feed sidecars beside source fixtures"
        );
        Ok(())
    }

    #[test]
    fn latest_nexrad_listing_uses_newest_file() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let path = temp.path().join("index.html");
        fs::write(
            &path,
            r#"
            <a href="CONUS_L2_CREF_QCD_20260511_211842.tif.gz">old</a>
            <a href="CONUS_L2_CREF_QCD_20260511_212042.tif.gz">new</a>
            "#,
        )?;

        assert_eq!(
            parse_latest_nexrad_listing(&path)?,
            "CONUS_L2_CREF_QCD_20260511_212042.tif.gz"
        );
        Ok(())
    }
}
