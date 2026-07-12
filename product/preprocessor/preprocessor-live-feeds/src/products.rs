use std::{
    collections::{BTreeMap, BTreeSet},
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
    build_metar_dataset, build_taf_dataset, build_tfr_dataset,
    engine::{
        read_json_value, sha256_hex, write_json_pretty_file, BuiltLiveFeedState, DeltaPolicy,
        LiveFeedStatePayload, LiveFeedStatusTimestamps, ProductBuilder, UpstreamEvent,
    },
    load_tfr_notam_ids, metar_content_fingerprint,
    notam_store::NotamPersistentStore,
    sanitize_notam_id, taf_content_fingerprint,
    tfr_detail_backfill::{TfrDetailBackfillStore, TfrDetailFetchTarget},
    tfr_notam_metadata_by_fdc_id, BuildMetarRequest, BuildTafRequest, BuildTfrRequest,
    StructuredTfrNotamMetadata,
};

const METAR_XML_URL: &str = "https://aviationweather.gov/data/cache/metars.cache.xml.gz";
const TAF_XML_URL: &str = "https://aviationweather.gov/data/cache/tafs.cache.xml.gz";
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
        state_sha256: None,
        state_payload_kind: None,
        status_timestamps: Default::default(),
        delta_policy,
        precomputed_delta: None,
        changed_count_if_no_delta,
    }
}

pub fn keyed_record_json_live_feed_state(
    product: &str,
    version: String,
    state_source_path: PathBuf,
    state_value: Value,
    records_key: &str,
    count_key: Option<&str>,
    changed_count_if_no_delta: usize,
) -> BuiltLiveFeedState {
    json_live_feed_state(
        product,
        version,
        state_source_path,
        state_value,
        DeltaPolicy::KeyedRecords {
            records_key: records_key.to_string(),
            count_key: count_key.map(str::to_string),
        },
        changed_count_if_no_delta,
    )
}

#[derive(Debug, Clone)]
pub struct NotamLiveFeedBuilder {
    state_root: PathBuf,
}

impl NotamLiveFeedBuilder {
    pub fn new(state_root: impl Into<PathBuf>) -> Self {
        Self {
            state_root: state_root.into(),
        }
    }
}

impl ProductBuilder for NotamLiveFeedBuilder {
    fn product_id(&self) -> &str {
        "notams"
    }

    fn build_state(
        &self,
        event: &UpstreamEvent,
        scratch_dir: &Path,
    ) -> anyhow::Result<BuiltLiveFeedState> {
        let generated_at_utc = normalized_event_time(event.observed_at_utc);
        let output_dir = fresh_dir(&scratch_dir.join("output"))?;
        let store = NotamPersistentStore::new(&self.state_root);
        let records = store.current_records()?;
        let notams_by_id = records
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect::<BTreeMap<_, _>>();
        let notam_count = notams_by_id.len();
        let content_for_version = serde_json::json!({
            "schema_version": 1,
            "notam_count": notam_count,
            "notams_by_id": notams_by_id,
        });
        let fingerprint = sha256_hex(
            &serde_json::to_vec(&content_for_version)
                .context("failed to encode NOTAM live-feed version content")?,
        );
        let version = content_version_label(&fingerprint);
        let structured_json_path = output_dir.join("notams.json");
        let state_value = serde_json::json!({
            "schema_version": 1,
            "version_label": version.clone(),
            "notam_count": notam_count,
            "notams_by_id": content_for_version["notams_by_id"].clone(),
        });
        write_json_pretty_file(&structured_json_path, &state_value)?;
        Ok(with_collected_at(
            keyed_record_json_live_feed_state(
                "notams",
                version,
                structured_json_path.clone(),
                state_value,
                "notams_by_id",
                Some("notam_count"),
                notam_count,
            ),
            generated_at_utc,
        ))
    }
}

fn with_collected_at(
    mut state: BuiltLiveFeedState,
    collected_at_utc: DateTime<Utc>,
) -> BuiltLiveFeedState {
    state.status_timestamps.collected_at_utc = Some(collected_at_utc);
    state
}

fn with_status_timestamps(
    mut state: BuiltLiveFeedState,
    status_timestamps: LiveFeedStatusTimestamps,
) -> BuiltLiveFeedState {
    state.status_timestamps = status_timestamps;
    state
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
        state_sha256: None,
        state_payload_kind: None,
        status_timestamps: Default::default(),
        delta_policy: DeltaPolicy::None,
        precomputed_delta: None,
        changed_count_if_no_delta,
    }
}

pub fn nav_kv_live_feed_state(
    product: &str,
    version: String,
    state_source_dir: PathBuf,
    manifest_source_path: PathBuf,
    manifest_value: Value,
    state_sha256: String,
    pairs: Vec<had_nav_kv::NavKvPair>,
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
        state_sha256: Some(state_sha256),
        state_payload_kind: Some("nav_kv".to_string()),
        status_timestamps: Default::default(),
        delta_policy: DeltaPolicy::NavKv { pairs },
        precomputed_delta: None,
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
        let requests =
            vec![PrefetchRequest::new(METAR_XML_URL).with_logical_file_name("metars.cache.xml.gz")];
        prefetch_archives_with_provenance(
            &requests,
            &input_dir,
            self.fetch.fetch_jobs,
            self.fetch.fetch_cache.as_ref(),
            &provenance_dir,
            "metars",
        )?;
        run_gzip_decompress(&input_dir.join("metars.cache.xml.gz"))?;
        let metar_xml_path = input_dir.join("metars.cache.xml");
        let fingerprint = metar_content_fingerprint(&metar_xml_path)?;
        let version = content_version_label(&fingerprint);
        let result = build_metar_dataset(&BuildMetarRequest {
            metar_xml_path,
            output_dir,
            version_label: version.clone(),
            generated_at_utc,
        })?;
        let state_value = read_json_value(&result.structured_json_path)?;
        Ok(with_collected_at(
            keyed_record_json_live_feed_state(
                "metars",
                version,
                result.structured_json_path,
                state_value,
                "metars_by_station",
                Some("metar_count"),
                result.metar_count,
            ),
            generated_at_utc,
        ))
    }
}

#[derive(Debug, Clone)]
pub struct TafLiveFeedBuilder {
    fetch: LiveFeedFetchConfig,
}

impl TafLiveFeedBuilder {
    pub fn new(fetch: LiveFeedFetchConfig) -> Self {
        Self { fetch }
    }
}

impl ProductBuilder for TafLiveFeedBuilder {
    fn product_id(&self) -> &str {
        "tafs"
    }

    fn build_state(
        &self,
        event: &UpstreamEvent,
        scratch_dir: &Path,
    ) -> anyhow::Result<BuiltLiveFeedState> {
        let generated_at_utc = normalized_event_time(event.observed_at_utc);
        let input_dir = fresh_dir(&scratch_dir.join("input"))?;
        let output_dir = fresh_dir(&scratch_dir.join("output"))?;
        let provenance_dir = scratch_dir.join("meta").join("provenance").join("tafs");
        let requests =
            vec![PrefetchRequest::new(TAF_XML_URL).with_logical_file_name("tafs.cache.xml.gz")];
        prefetch_archives_with_provenance(
            &requests,
            &input_dir,
            self.fetch.fetch_jobs,
            self.fetch.fetch_cache.as_ref(),
            &provenance_dir,
            "tafs",
        )?;
        run_gzip_decompress(&input_dir.join("tafs.cache.xml.gz"))?;
        let taf_xml_path = input_dir.join("tafs.cache.xml");
        let fingerprint = taf_content_fingerprint(&taf_xml_path)?;
        let version = content_version_label(&fingerprint);
        let result = build_taf_dataset(&BuildTafRequest {
            taf_xml_path,
            output_dir,
            version_label: version.clone(),
            generated_at_utc,
        })?;
        let state_value = read_json_value(&result.structured_json_path)?;
        Ok(with_collected_at(
            keyed_record_json_live_feed_state(
                "tafs",
                version,
                result.structured_json_path,
                state_value,
                "tafs_by_station",
                Some("taf_count"),
                result.taf_count,
            ),
            generated_at_utc,
        ))
    }
}

#[derive(Debug, Clone)]
pub struct TfrLiveFeedBuilder {
    fetch: LiveFeedFetchConfig,
    notam_state_root: Option<PathBuf>,
    tfr_detail_backfill_state_root: Option<PathBuf>,
}

impl TfrLiveFeedBuilder {
    pub fn new(fetch: LiveFeedFetchConfig) -> Self {
        Self {
            fetch,
            notam_state_root: None,
            tfr_detail_backfill_state_root: None,
        }
    }

    pub fn with_notam_state_root(mut self, state_root: impl Into<PathBuf>) -> Self {
        self.notam_state_root = Some(state_root.into());
        self
    }

    pub fn with_tfr_detail_backfill_state_root(mut self, state_root: impl Into<PathBuf>) -> Self {
        self.tfr_detail_backfill_state_root = Some(state_root.into());
        self
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
        let tfr_notam_ids = load_tfr_notam_ids(&input_dir)?
            .into_iter()
            .collect::<BTreeSet<_>>();
        let mut notams_by_fdc_id = self.current_tfr_notam_metadata_by_fdc_id(&tfr_notam_ids);
        let missing_from_swim = tfr_notam_ids
            .iter()
            .filter(|tfr_id| !notams_by_fdc_id.contains_key(*tfr_id))
            .cloned()
            .collect::<BTreeSet<_>>();
        if let Some(backfill_metadata) =
            self.current_tfr_detail_backfill_metadata(&missing_from_swim)
        {
            for (fdc_id, metadata) in backfill_metadata {
                notams_by_fdc_id.entry(fdc_id).or_insert(metadata);
            }
        }
        write_json_pretty_file(
            &input_dir.join("notam-enrichment-by-fdc-id.json"),
            &notams_by_fdc_id,
        )?;
        let fingerprint = hash_tree(&input_dir)?;
        let version = content_version_label(&fingerprint);
        let result = build_tfr_dataset(&BuildTfrRequest {
            input_dir,
            output_dir,
            version_label: version.clone(),
            generated_at_utc,
            notams_by_fdc_id,
        })?;
        let state_value = read_json_value(&result.structured_json_path)?;
        Ok(with_collected_at(
            json_live_feed_state(
                "tfrs",
                version,
                result.structured_json_path,
                state_value,
                DeltaPolicy::None,
                result.area_group_count,
            ),
            generated_at_utc,
        ))
    }
}

impl TfrLiveFeedBuilder {
    fn current_tfr_notam_metadata_by_fdc_id(
        &self,
        tfr_notam_ids: &BTreeSet<String>,
    ) -> BTreeMap<String, StructuredTfrNotamMetadata> {
        let Some(state_root) = &self.notam_state_root else {
            return BTreeMap::new();
        };
        match NotamPersistentStore::new(state_root).current_records() {
            Ok(records) => {
                let mut metadata = tfr_notam_metadata_by_fdc_id(&records);
                metadata.retain(|fdc_id, _| tfr_notam_ids.contains(fdc_id));
                metadata
            }
            Err(error) => {
                eprintln!(
                    "TFR NOTAM enrichment unavailable from {}: {error:#}",
                    state_root.display()
                );
                BTreeMap::new()
            }
        }
    }

    fn current_tfr_detail_backfill_metadata(
        &self,
        missing_tfr_ids: &BTreeSet<String>,
    ) -> Option<Vec<(String, StructuredTfrNotamMetadata)>> {
        let Some(state_root) = &self.tfr_detail_backfill_state_root else {
            return None;
        };
        let store = TfrDetailBackfillStore::new(state_root);
        if let Err(error) = store.record_desired_tfrs(missing_tfr_ids) {
            eprintln!(
                "TFR detail backfill queue unavailable from {}: {error:#}",
                state_root.display()
            );
        }
        match store.current_metadata_by_fdc_id(missing_tfr_ids) {
            Ok(metadata) => Some(metadata),
            Err(error) => {
                eprintln!(
                    "TFR detail backfill metadata unavailable from {}: {error:#}",
                    state_root.display()
                );
                None
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TfrDetailBackfillRunSummary {
    pub attempted: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub desired: usize,
    pub stored: usize,
    pub total_failures: usize,
    pub remaining_unfetched: usize,
    pub remaining_due: usize,
}

pub fn fetch_tfr_detail_backfill_once(
    fetch: &LiveFeedFetchConfig,
    store_root: &Path,
    scratch_dir: &Path,
    max_fetches: usize,
) -> anyhow::Result<TfrDetailBackfillRunSummary> {
    let store = TfrDetailBackfillStore::new(store_root);
    store.initialize()?;
    let targets = store.due_fetch_targets(max_fetches)?;
    let input_root = fresh_dir(scratch_dir)?;
    let mut succeeded = 0_usize;
    let mut failed = 0_usize;
    for target in &targets {
        match fetch_one_tfr_detail_backfill(fetch, &store, &input_root, target) {
            Ok(()) => succeeded += 1,
            Err(error) => {
                failed += 1;
                let _ = store.record_failure(target, format!("{error:#}"));
                eprintln!(
                    "TFR detail backfill failed for {}: {error:#}",
                    target.tfr_id
                );
            }
        }
    }
    let summary = store.summary()?;
    Ok(TfrDetailBackfillRunSummary {
        attempted: targets.len(),
        succeeded,
        failed,
        desired: summary.desired,
        stored: summary.stored,
        total_failures: summary.failures,
        remaining_unfetched: summary.remaining_unfetched,
        remaining_due: summary.due,
    })
}

fn fetch_one_tfr_detail_backfill(
    fetch: &LiveFeedFetchConfig,
    store: &TfrDetailBackfillStore,
    input_root: &Path,
    target: &TfrDetailFetchTarget,
) -> anyhow::Result<()> {
    let file_name = format!("{}.xml", sanitize_notam_id(&target.tfr_id));
    let input_dir = fresh_dir(&input_root.join(sanitize_notam_id(&target.tfr_id)))?;
    let provenance_dir = input_dir.join("provenance");
    let request = PrefetchRequest::new(&target.source_url)
        .with_logical_file_name(&file_name)
        .with_cache_key(format!("tfr-detail-backfill:{}", target.tfr_id));
    prefetch_requests_with_provenance(
        &[request],
        &input_dir,
        1,
        fetch.fetch_cache.as_ref(),
        &provenance_dir,
        "tfr-detail-backfill",
    )?;
    let xml_path = input_dir.join(&file_name);
    let xml = fs::read_to_string(&xml_path)
        .with_context(|| format!("failed to read TFR detail XML {}", xml_path.display()))?;
    store.record_success(target, &xml)?;
    Ok(())
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
        event: &UpstreamEvent,
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
            generated_at_utc: Some(normalized_event_time(event.observed_at_utc)),
        })?;
        let state_value = read_json_value(&result.manifest_path)?;
        let state_source_dir = result
            .manifest_path
            .parent()
            .context("obstacle HAD manifest has no parent")?
            .to_path_buf();
        Ok(with_collected_at(
            nav_kv_live_feed_state(
                "obstacles",
                version,
                state_source_dir,
                result.manifest_path,
                state_value,
                result.state_sha256,
                result.had_pairs,
                result.had_page_paths.len(),
            ),
            normalized_event_time(event.observed_at_utc),
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
        Ok(with_status_timestamps(
            nexrad_live_feed_state(version, output_dir, manifest_path, manifest_value)?,
            LiveFeedStatusTimestamps {
                published_at_utc: Some(observed_at),
                collected_at_utc: Some(normalized_event_time(event.observed_at_utc)),
            },
        ))
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
    let output = Command::new("gzip")
        .arg("-d")
        .arg(path)
        .output()
        .with_context(|| format!("failed to run gzip on {}", path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            bail!("gzip failed for {}", path.display());
        }
        bail!("gzip failed for {}: {}", path.display(), stderr);
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
    use crate::engine::{
        canonical_json_sha256, read_live_feeds_current, FileLiveFeedPublisher, FixedClock,
        LiveFeedPublisher, LiveFeedVersionManifest,
    };
    use chrono::TimeZone;
    use serde::Deserialize;
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
    fn notam_live_feed_builder_publishes_keyed_record_state() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let state_root = temp.path().join("notam-state");
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<AIXMBasicMessage xmlns:event="http://www.aixm.aero/schema/5.1/event">
  <hasMember>
    <event:Event>
      <event:timeSlice>
        <event:EventTimeSlice>
          <event:scenario>95</event:scenario>
          <event:textNOTAM>
            <event:NOTAM>
              <event:number>1</event:number>
              <event:year>2026</event:year>
              <event:type>N</event:type>
              <event:issued>2026-07-11T02:00:00.000Z</event:issued>
              <event:location>AAA</event:location>
              <event:effectiveStart>202607110200</event:effectiveStart>
              <event:effectiveEnd>202607111200</event:effectiveEnd>
              <event:text>RWY 01 CLSD.</event:text>
            </event:NOTAM>
          </event:textNOTAM>
        </event:EventTimeSlice>
      </event:timeSlice>
    </event:Event>
  </hasMember>
</AIXMBasicMessage>"#;
        let line = serde_json::json!({
            "jmsMessageId": "ID:test",
            "receivedAtUtc": "2026-07-11T02:03:00Z",
            "properties": {
                "us_gov_dot_faa_aim_fns_nds_SourceType": "D",
                "us_gov_dot_faa_aim_fns_nds_ICAOId": "KAAA",
                "us_gov_dot_faa_aim_fns_nds_LocationDesignator": "AAA",
                "us_gov_dot_faa_aim_fns_nds_NOTAMStatus": "ACTIVE",
                "us_gov_dot_faa_aim_fns_nds_NOTAMFunction": "NOTAMN",
                "us_gov_dot_faa_aim_fns_nds_NOTAMKeyword": "RWY"
            },
            "bodyText": xml
        });
        let store = NotamPersistentStore::new(&state_root);
        store.initialize(&crate::notam_store::SwimNotamSubscriptionIdentity {
            provider_url: "smfs://example.test:55443".to_string(),
            queue: "example.queue".to_string(),
            connection_factory: "example.CF".to_string(),
            username: "example.user".to_string(),
            vpn: "example-vpn".to_string(),
        })?;
        store.insert_raw_message_for_test(
            "message-a",
            "2026-07-11T02:03:00Z",
            &line.to_string(),
        )?;
        store.apply_pending_raw_messages(10)?;

        let observed_at_utc = Utc.with_ymd_and_hms(2026, 7, 11, 2, 3, 0).unwrap();
        let event = UpstreamEvent {
            product: "notams".to_string(),
            source_id: "notams:test".to_string(),
            previous_source_id: None,
            observed_at_utc,
            payload_path: None,
        };
        let builder = NotamLiveFeedBuilder::new(&state_root);
        let built = builder.build_state(&event, &temp.path().join("scratch"))?;
        assert_eq!(built.product, "notams");
        let LiveFeedStatePayload::JsonFile { value, .. } = &built.payload else {
            panic!("NOTAM live-feed state should be JSON");
        };
        assert_eq!(value["notam_count"], 1);
        assert_eq!(
            value["notams_by_id"]["D:AAA:2026:N:1"]["text"],
            "RWY 01 CLSD."
        );

        let live_root = temp.path().join("live");
        let publisher = FileLiveFeedPublisher::new(
            live_root.clone(),
            FixedClock::new(Utc.with_ymd_and_hms(2026, 7, 11, 2, 4, 0).unwrap()),
        );
        let update = publisher.publish(built)?;
        assert_eq!(update.product, "notams");
        assert_eq!(update.changed_count, 1);
        let current = read_live_feeds_current(&live_root)?.expect("current manifest");
        assert!(current.products.contains_key("notams"));
        Ok(())
    }

    #[test]
    fn tfr_builder_queues_and_consumes_detail_backfill_metadata() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let state_root = temp.path().join("tfr-detail-backfill");
        let builder = TfrLiveFeedBuilder::new(LiveFeedFetchConfig::cache_first(
            1,
            temp.path().join("cache"),
        ))
        .with_tfr_detail_backfill_state_root(&state_root);
        let missing = BTreeSet::from(["6/8212".to_string()]);

        let first = builder
            .current_tfr_detail_backfill_metadata(&missing)
            .expect("metadata query");
        assert!(first.is_empty());
        let store = TfrDetailBackfillStore::new(&state_root);
        let targets = store.due_fetch_targets(10)?;
        assert_eq!(targets.len(), 1);
        assert_eq!(
            targets[0].source_url,
            "https://tfr.faa.gov/download/detail_6_8212.xml"
        );

        store.record_success(
            &targets[0],
            r#"
            <XNOTAM-Update>
              <Not>
                <NotUid><dateIndexYear>2026</dateIndexYear><noSeqNo>8212</noSeqNo></NotUid>
                <codeFacility>ZTL</codeFacility>
                <dateEffective>2026-06-17T04:01:00</dateEffective>
                <dateExpire>2026-07-17T03:59:00</dateExpire>
                <txtNameCity>Kennesaw</txtNameCity>
                <txtNameUSState>GEORGIA</txtNameUSState>
                <valDistVerUpper>400</valDistVerUpper>
                <uomDistVerUpper>FT</uomDistVerUpper>
                <valDistVerLower>0</valDistVerLower>
                <uomDistVerLower>FT</uomDistVerLower>
              </Not>
            </XNOTAM-Update>
            "#,
        )?;

        let second = builder
            .current_tfr_detail_backfill_metadata(&missing)
            .expect("metadata query");
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].0, "6/8212");
        assert_eq!(second[0].1.record_id, "F:ZTL:2026:N:8212");
        assert!(second[0]
            .1
            .text
            .as_deref()
            .expect("text")
            .contains("SFC-400FT"));
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

    #[derive(Debug, Deserialize)]
    struct NexradFixtureManifest {
        product: String,
        fixture: String,
        frame_count: usize,
        frames: Vec<NexradFixtureFrame>,
    }

    #[derive(Debug, Deserialize)]
    struct NexradFixtureFrame {
        file: String,
        observed_at_utc: String,
        bytes: u64,
        sha256: String,
    }

    fn nexrad_three_hour_fixture_root() -> anyhow::Result<Option<PathBuf>> {
        let root = if let Some(root) = std::env::var_os("AEROBAG_TEST_ARTIFACTS_ROOT")
            .or_else(|| std::env::var_os("AEROBAG_TEST_ARTIFACTS"))
        {
            PathBuf::from(root)
        } else {
            let default_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("..")
                .join("..")
                .join("aerobag-test-artifacts");
            if !default_root.exists() {
                eprintln!(
                    "skipping large-fixture NEXRAD test: AEROBAG_TEST_ARTIFACTS_ROOT is not set and {} is absent",
                    default_root.display()
                );
                return Ok(None);
            }
            default_root
        };
        let fixture_root = root.join("nexrad").join("source-grid-three-hour");
        if !fixture_root.join("manifest.json").is_file() {
            bail!(
                "test artifacts do not contain nexrad/source-grid-three-hour/manifest.json under {}",
                root.display()
            );
        }
        Ok(Some(fixture_root))
    }

    fn read_nexrad_fixture_manifest(fixture_root: &Path) -> anyhow::Result<NexradFixtureManifest> {
        let manifest_path = fixture_root.join("manifest.json");
        let manifest: NexradFixtureManifest = serde_json::from_slice(
            &fs::read(&manifest_path)
                .with_context(|| format!("failed to read {}", manifest_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
        Ok(manifest)
    }

    fn parsed_fixture_time(frame: &NexradFixtureFrame) -> anyhow::Result<DateTime<Utc>> {
        Ok(DateTime::parse_from_rfc3339(&frame.observed_at_utc)
            .with_context(|| format!("bad observed_at_utc {}", frame.observed_at_utc))?
            .with_timezone(&Utc))
    }

    fn collect_png_paths(root: &Path, paths: &mut Vec<PathBuf>) -> anyhow::Result<()> {
        for entry in
            fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                collect_png_paths(&path, paths)?;
            } else if path.extension().is_some_and(|extension| extension == "png") {
                paths.push(path);
            }
        }
        Ok(())
    }

    fn png_color_type_and_palette_lengths(path: &Path) -> anyhow::Result<(u8, Vec<usize>)> {
        let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
        let signature = b"\x89PNG\r\n\x1a\n";
        if bytes.len() < signature.len() || &bytes[..signature.len()] != signature {
            bail!("{} is not a PNG", path.display());
        }
        let mut offset = signature.len();
        let mut color_type = None;
        let mut palette_lengths = Vec::new();
        while offset + 12 <= bytes.len() {
            let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?) as usize;
            let chunk_type = &bytes[offset + 4..offset + 8];
            let data_start = offset + 8;
            let data_end = data_start + length;
            if data_end + 4 > bytes.len() {
                bail!("{} has truncated PNG chunk", path.display());
            }
            if chunk_type == b"IHDR" {
                if length != 13 {
                    bail!("{} has unexpected IHDR length {length}", path.display());
                }
                color_type = Some(bytes[data_start + 9]);
            } else if chunk_type == b"PLTE" {
                palette_lengths.push(length);
            } else if chunk_type == b"IEND" {
                break;
            }
            offset = data_end + 4;
        }
        Ok((color_type.context("PNG missing IHDR")?, palette_lengths))
    }

    #[test]
    fn nexrad_three_hour_fixture_manifest_validates_real_upstream_frames() -> anyhow::Result<()> {
        let Some(fixture_root) = nexrad_three_hour_fixture_root()? else {
            return Ok(());
        };
        let manifest = read_nexrad_fixture_manifest(&fixture_root)?;

        assert_eq!(manifest.product, "nexrad");
        assert_eq!(manifest.fixture, "source-grid-three-hour");
        assert_eq!(manifest.frame_count, manifest.frames.len());
        assert!(manifest.frames.len() >= 90);

        let mut previous_time: Option<DateTime<Utc>> = None;
        let mut max_gap_seconds = 0;
        for frame in &manifest.frames {
            let path = fixture_root.join("raw").join(&frame.file);
            let metadata = fs::metadata(&path)
                .with_context(|| format!("missing fixture frame {}", path.display()))?;
            assert_eq!(metadata.len(), frame.bytes, "fixture frame byte count");
            assert_eq!(hash_file(&path)?, frame.sha256, "fixture frame sha256");

            let parsed_from_name = parse_nexrad_observed_at_utc(&frame.file)?.and_utc();
            let parsed_from_manifest = parsed_fixture_time(frame)?;
            assert_eq!(parsed_from_name, parsed_from_manifest);

            if let Some(previous) = previous_time {
                let gap_seconds = (parsed_from_manifest - previous).num_seconds();
                assert!(
                    gap_seconds > 0,
                    "fixture frames must be strictly time ordered"
                );
                max_gap_seconds = max_gap_seconds.max(gap_seconds);
            }
            previous_time = Some(parsed_from_manifest);
        }
        assert!(
            max_gap_seconds <= 180,
            "fixture should be contiguous enough for a live-feed timeline; max gap was {max_gap_seconds}s"
        );
        Ok(())
    }

    #[test]
    fn nexrad_three_hour_fixture_builds_and_publishes_source_grid_states() -> anyhow::Result<()> {
        let Some(fixture_root) = nexrad_three_hour_fixture_root()? else {
            return Ok(());
        };
        let manifest = read_nexrad_fixture_manifest(&fixture_root)?;
        let temp = tempdir()?;
        let live_root = temp.path().join("live-feeds");
        let publisher = FileLiveFeedPublisher::new(
            live_root.clone(),
            FixedClock::new(Utc.with_ymd_and_hms(2026, 5, 18, 0, 0, 0).unwrap()),
        );
        let palette_hash = hash_text(NEXRAD_FIXED_OPAQUE_PALETTE_JSON);

        let mut previous_version: Option<String> = None;
        let mut expected_last_version = None;
        for (index, frame) in manifest.frames.iter().enumerate() {
            let source_path = fixture_root.join("raw").join(&frame.file);
            let observed_at = parsed_fixture_time(frame)?;
            let version = format!(
                "{}_{}_png8{}",
                observed_at.format("%Y%m%dT%H%M%SZ"),
                &frame.sha256[..16],
                &palette_hash[..8]
            );
            expected_last_version = Some(version.clone());
            let output_dir = temp.path().join("states").join(format!("{index:03}"));
            fs::create_dir_all(&output_dir)?;

            build_nexrad_source_grid_tiles(
                &source_path,
                &output_dir,
                &version,
                &observed_at.to_rfc3339(),
                &frame.file,
                &frame.sha256,
                false,
            )?;

            let manifest_path = output_dir.join("manifest.json");
            let manifest_value = read_json_value(&manifest_path)?;
            assert_eq!(manifest_value["product"], "nexrad");
            assert_eq!(manifest_value["state_id"], version);
            assert_eq!(manifest_value["source_file"], frame.file);
            assert_eq!(manifest_value["source_sha256"], frame.sha256);
            assert_eq!(manifest_value["tile_encoding"], "png8-fixed-palette");
            assert_eq!(manifest_value["palette"]["sha256"], palette_hash);
            assert_eq!(
                manifest_value["res-levels"],
                serde_json::json!([0, 1, 2, 3])
            );
            assert_eq!(manifest_value["source_grid"]["width"], 7000);
            assert_eq!(manifest_value["source_grid"]["height"], 3500);
            let tile_count = live_nexrad_tile_count(&manifest_value)?;
            assert_eq!(tile_count, 136);

            let mut png_paths = Vec::new();
            collect_png_paths(&output_dir.join("tiles"), &mut png_paths)?;
            assert_eq!(png_paths.len(), tile_count);
            let mut min_palette_length = usize::MAX;
            for png_path in &png_paths {
                let (color_type, palette_lengths) = png_color_type_and_palette_lengths(png_path)?;
                assert_eq!(
                    color_type,
                    3,
                    "{} should be indexed PNG",
                    png_path.display()
                );
                assert_eq!(
                    palette_lengths.len(),
                    1,
                    "{} should carry one PNG palette",
                    png_path.display()
                );
                assert_eq!(
                    palette_lengths[0] % 3,
                    0,
                    "{} PLTE length should be RGB triples",
                    png_path.display()
                );
                assert!(
                    palette_lengths[0] <= 768,
                    "{} PLTE should fit PNG8",
                    png_path.display()
                );
                min_palette_length = min_palette_length.min(palette_lengths[0]);
            }
            assert!(
                min_palette_length < 768,
                "at least one NEXRAD tile should use a compact palette"
            );

            let built = nexrad_live_feed_state(
                version.clone(),
                output_dir,
                manifest_path,
                manifest_value.clone(),
            )?;
            let result = publisher.publish(built)?;
            assert_eq!(result.product, "nexrad");
            assert_eq!(result.version, version);
            assert_eq!(result.changed_count, tile_count);
            assert!(result.delta_path.is_none());

            let current = read_live_feeds_current(&live_root)?.expect("live-feeds current");
            let current_nexrad = current.products.get("nexrad").expect("nexrad current");
            assert_eq!(current_nexrad.current, version);
            assert_eq!(
                current_nexrad.state_sha256,
                canonical_json_sha256(&manifest_value)?
            );

            let version_manifest_path = live_root
                .join("versions")
                .join("nexrad")
                .join(format!("{version}.json"));
            let version_manifest: LiveFeedVersionManifest =
                serde_json::from_slice(&fs::read(&version_manifest_path)?)?;
            assert_eq!(version_manifest.product, "nexrad");
            assert_eq!(version_manifest.version, version);
            assert!(version_manifest.previous.is_none());
            assert_eq!(version_manifest.state.kind.as_deref(), Some("json"));
            assert_eq!(
                version_manifest
                    .install_state
                    .as_ref()
                    .and_then(|state| state.kind.as_deref()),
                Some("directory_package")
            );
            assert!(version_manifest.delta_from_previous.is_none());

            if let Some(previous) = previous_version.as_deref() {
                assert_ne!(previous, current_nexrad.current);
            }
            previous_version = Some(current_nexrad.current.clone());
        }

        assert_eq!(previous_version, expected_last_version);
        Ok(())
    }
}
