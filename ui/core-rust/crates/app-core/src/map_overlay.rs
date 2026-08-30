// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use airspace_geometry::{expand_airspace_path, AirspaceSegment};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use notam_state::{NotamDelta, NotamMutation, NotamState, NotamStateError};
#[cfg(test)]
use product_contracts::NOTAM_LIVE_FEED_CONTRACT_VERSION;
use product_contracts::{
    AirportNotamEffect, ProcedureRendezvousIdentity, ProcedureRendezvousKey,
    ProcedureRendezvousKind,
};
use serde::{Deserialize, Serialize};

pub use app_ui_contracts::nav_query::NavSymbolFeature;
use serde_json::json;

use crate::{
    core_clock_ms, core_perf_debug_log,
    data_status::{DataStatusRecord, UiStatusSeverity},
    geometry::LatLon,
    great_circle_distance_nm, AppError, AppErrorKind, AppResult, FlightPlan, FlightPlanRowActionId,
    MapViewport, NavRef, RouteComponentViewKind,
};

pub const VECTOR_DISPLAY_FEATURE_LIMIT: usize = 500;
pub const AIRSPACE_DISPLAY_FEATURE_LIMIT: usize = 700;
pub const AIRSPACE_FEATHER_LIMIT: usize = 5_000;
const AIRSPACE_DECORATION_SCREEN_MARGIN_PX: f64 = 256.0;
const LABEL_COLLISION_PADDING_PX: f64 = 3.0;
const POINT_TILE_ZOOM: u32 = 9;
const AIRSPACE_MIN_DISPLAY_ZOOM: f64 = 6.0;
const AIRPORT_MIN_DISPLAY_ZOOM: f64 = 8.0;
const FIX_MIN_DISPLAY_ZOOM: f64 = 9.0;
const NAV_MIN_DISPLAY_ZOOM: f64 = 7.0;
const OBSTACLE_MIN_DISPLAY_ZOOM: f64 = 8.0;
// Search result focus should land at a local chart scale, not preserve a
// previously zoomed-out continental view. Preserve closer views, but raise
// wider views to the app's default chart startup zoom.
pub const MAP_SELECTION_NAV_REF_MIN_FOCUS_ZOOM: f64 = 10.0;
const OBSTACLE_LOOKAHEAD_MINUTES: f64 = 5.0;
const OBSTACLE_LOOKAHEAD_DEFAULT_DIAMETER_NM: f64 = 5.0;
const OBSTACLE_LOOKAHEAD_CENTER_OFFSET_DIAMETER_RATIO: f64 = 0.3;
const OBSTACLE_BELOW_OWNERSHIP_HIDE_FT: f64 = 1000.0;
const OBSTACLE_CAUTION_LOWER_FT: f64 = 800.0;
const OBSTACLE_DANGER_LOWER_FT: f64 = 200.0;
const WEATHER_CAMERA_AIRPORT_BADGE_OFFSET_LOGICAL_PX: f64 = 14.0;
pub(crate) const WEATHER_DISPLAY_FEATURE_LIMIT: usize = 1_000;
const WEATHER_MIN_DISPLAY_ZOOM: f64 = 5.0;
// Full observations routinely exceed the cap at z6 in an ordinary 1200x900
// viewport. Keep the sparse station set through z6; z7 remains below the cap
// for high-probability 1080p workloads while retaining the hard safety limit.
const WEATHER_FULL_DETAIL_MIN_DISPLAY_ZOOM: f64 = 7.0;
pub(crate) const VECTOR_DISPLAY_LIMIT_STATUS_ID: &str = "map_overlay:vector_display_feature_limit";
pub(crate) const WEATHER_DISPLAY_LIMIT_STATUS_ID: &str = "map_overlay:metar_display_feature_limit";
pub(crate) const AIRSPACE_DISPLAY_LIMIT_STATUS_ID: &str =
    "map_overlay:airspace_display_feature_limit";
pub(crate) const AIRSPACE_FEATHER_LIMIT_STATUS_ID: &str = "map_overlay:airspace_feather_limit";
pub(crate) const MAP_OVERLAY_DISPLAY_LIMIT_STATUS_IDS: [&str; 4] = [
    VECTOR_DISPLAY_LIMIT_STATUS_ID,
    WEATHER_DISPLAY_LIMIT_STATUS_ID,
    AIRSPACE_DISPLAY_LIMIT_STATUS_ID,
    AIRSPACE_FEATHER_LIMIT_STATUS_ID,
];
const WORLD_SIZE: f64 = 256.0;
const MAX_LATITUDE: f64 = 85.051_128_78;
const UI_THUMB_SIZE_LOGICAL_PX: f64 = 56.0;
const INSPECTOR_HIT_RADIUS_THUMBS: f64 = 0.5;
const TFR_ACTIVE_STYLE_KEY: &str = "tfr_active";
const TFR_UPCOMING_STYLE_KEY: &str = "tfr_upcoming";
const WEATHER_STATION_AIRPORT_ALIAS_MAX_DISTANCE_NM: f64 = 5.0;

#[derive(Debug, Default)]
struct VectorDisplayBudgetAudit {
    scanned_records: usize,
    displayable_records: usize,
    omitted_after_cap: usize,
    drawn_by_layer: BTreeMap<String, usize>,
    omitted_by_layer: BTreeMap<String, usize>,
    hidden_by_layer: BTreeMap<String, usize>,
    no_symbol_by_layer: BTreeMap<String, usize>,
}

struct VectorDisplayBudgetBucket {
    layer: &'static str,
    features: Vec<VisibleMapFeature>,
}

fn bump_layer_count(counts: &mut BTreeMap<String, usize>, layer: &str) {
    *counts.entry(layer.to_string()).or_insert(0) += 1;
}

fn add_layer_count(counts: &mut BTreeMap<String, usize>, layer: &str, amount: usize) {
    if amount > 0 {
        *counts.entry(layer.to_string()).or_insert(0) += amount;
    }
}

fn vector_display_budget_buckets() -> Vec<VectorDisplayBudgetBucket> {
    vec![
        VectorDisplayBudgetBucket {
            layer: "obstacle",
            features: Vec::new(),
        },
        VectorDisplayBudgetBucket {
            layer: "airport",
            features: Vec::new(),
        },
        VectorDisplayBudgetBucket {
            layer: "nav",
            features: Vec::new(),
        },
        VectorDisplayBudgetBucket {
            layer: "fix",
            features: Vec::new(),
        },
        VectorDisplayBudgetBucket {
            layer: "weather_camera",
            features: Vec::new(),
        },
    ]
}

fn vector_display_budget_bucket_index(symbol_kind: &str) -> Option<usize> {
    match symbol_kind {
        "obstacle" => Some(0),
        "airport" => Some(1),
        "nav" => Some(2),
        "fix" => Some(3),
        "weather_camera" => Some(4),
        _ => None,
    }
}

fn layer_counts_summary(counts: &BTreeMap<String, usize>) -> String {
    counts
        .iter()
        .map(|(layer, count)| format!("{layer}={count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MapSurfaceMetrics {
    pub viewport: MapViewport,
    pub width_px: f64,
    pub height_px: f64,
    pub display_scale: f64,
}

impl MapSurfaceMetrics {
    pub fn new(viewport: MapViewport, width_px: f64, height_px: f64, display_scale: f64) -> Self {
        Self {
            viewport,
            width_px,
            height_px,
            display_scale: normalized_display_scale(display_scale),
        }
    }

    pub(crate) fn effective_display_zoom(self) -> f64 {
        let effective_zoom = self.viewport.zoom - self.display_scale.log2();
        if effective_zoom.is_finite() {
            effective_zoom
        } else {
            self.viewport.zoom
        }
    }

    pub(crate) fn raw_zoom_at_least_display_zoom(self, minimum_display_zoom: f64) -> f64 {
        self.viewport
            .zoom
            .max(minimum_display_zoom + self.display_scale.log2())
    }

    fn point_tile_zoom(self) -> u32 {
        self.effective_display_zoom()
            .floor()
            .clamp(0.0, POINT_TILE_ZOOM as f64) as u32
    }

    fn logical_px_to_surface_px(self, logical_px: f64) -> f64 {
        logical_px * self.display_scale
    }

    pub(crate) fn inspector_hit_radius_px(self) -> f64 {
        self.logical_px_to_surface_px(UI_THUMB_SIZE_LOGICAL_PX * INSPECTOR_HIT_RADIUS_THUMBS)
    }

    pub(crate) fn visible_radius_nm(self) -> f64 {
        let center_world = lat_lon_to_world(self.viewport.center);
        let scale = 2.0_f64.powf(self.viewport.zoom);
        let corner = world_to_lat_lon(WorldPoint {
            x: center_world.x + self.width_px / 2.0 / scale,
            y: center_world.y + self.height_px / 2.0 / scale,
        });
        great_circle_distance_nm(self.viewport.center, corner)
    }

    pub(crate) fn project_position(self, position: LatLon) -> (f64, f64) {
        let point = world_to_screen(
            lat_lon_to_world(self.viewport.center),
            2.0_f64.powf(self.viewport.zoom),
            self.width_px,
            self.height_px,
            position,
        );
        (point.x, point.y)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlaySurfaceDecision {
    pub raw_zoom: f64,
    pub width_px: f64,
    pub height_px: f64,
    pub display_scale: f64,
    pub effective_display_zoom: f64,
    pub point_tile_zoom: u32,
    pub metar_tile_zoom: Option<u32>,
    pub airspace_ref_zoom: Option<u32>,
    pub airspace_label_zoom: Option<u32>,
}

pub fn overlay_surface_decision(
    metrics: MapSurfaceMetrics,
    config: &MapOverlayConfig,
) -> OverlaySurfaceDecision {
    let effective_display_zoom = metrics.effective_display_zoom();
    OverlaySurfaceDecision {
        raw_zoom: metrics.viewport.zoom,
        width_px: metrics.width_px,
        height_px: metrics.height_px,
        display_scale: metrics.display_scale,
        effective_display_zoom,
        point_tile_zoom: metrics.point_tile_zoom(),
        metar_tile_zoom: config
            .metar_layer
            .as_ref()
            .and_then(|layer| weather_tile_zoom(layer, effective_display_zoom)),
        airspace_ref_zoom: (effective_display_zoom >= AIRSPACE_MIN_DISPLAY_ZOOM)
            .then(|| airspace_reference_zoom(effective_display_zoom, config)),
        airspace_label_zoom: (effective_display_zoom >= AIRSPACE_MIN_DISPLAY_ZOOM)
            .then(|| airspace_label_zoom(effective_display_zoom, config)),
    }
}

fn weather_tile_zoom(layer: &PointTileLayerConfig, effective_display_zoom: f64) -> Option<u32> {
    if effective_display_zoom < WEATHER_MIN_DISPLAY_ZOOM {
        return None;
    }
    if effective_display_zoom < WEATHER_FULL_DETAIL_MIN_DISPLAY_ZOOM {
        return Some(nearest_available_layer_zoom(layer, layer.min_zoom));
    }
    Some(nearest_available_layer_zoom(
        layer,
        effective_display_zoom.floor().clamp(0.0, u32::MAX as f64) as u32,
    ))
}

fn full_weather_detail_visible(metrics: MapSurfaceMetrics) -> bool {
    metrics.effective_display_zoom() >= WEATHER_FULL_DETAIL_MIN_DISPLAY_ZOOM
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorTileRequest {
    pub layer: String,
    pub z: u32,
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone)]
struct DisplayVectorTile {
    request: VectorTileRequest,
    world_x_offset: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObstacleLayerConfig {
    pub min_zoom: u32,
    pub max_zoom: u32,
    pub available_zooms: Vec<u32>,
    pub high_detail_zoom: u32,
    pub zoom_levels: HashMap<u32, ObstacleZoomLevelConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObstacleZoomLevelConfig {
    pub zoom: u32,
    pub filtered: bool,
    pub min_agl_ft: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObstacleOverlayContext {
    pub position: LatLon,
    pub track_deg_true: Option<f64>,
    pub ground_speed_kt: Option<f64>,
    pub altitude_ft: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObstaclePointSemantics {
    pub height_agl_ft: f64,
    pub elevation_msl_ft: f64,
    pub top_msl_ft: f64,
    pub is_tall: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherCameraPointSemantics {
    pub site_id: String,
    pub site_name: String,
    #[serde(default)]
    pub site_identifier: Option<String>,
    #[serde(default)]
    pub icao: Option<String>,
    pub page_url: String,
    #[serde(default)]
    pub operated_by: Option<String>,
    #[serde(default)]
    pub attribution: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
    #[serde(default)]
    pub in_maintenance: Option<bool>,
    #[serde(default)]
    pub third_party: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointVectorRecord {
    pub id: String,
    pub kind: String,
    pub lat: f64,
    pub lon: f64,
    pub label: String,
    #[serde(default)]
    pub location_label: Option<String>,
    pub style_class: String,
    #[serde(default)]
    pub towered: Option<bool>,
    #[serde(default)]
    pub fuel_available: Option<bool>,
    #[serde(default)]
    pub public_use: Option<bool>,
    #[serde(default)]
    pub private_use: Option<bool>,
    #[serde(default)]
    pub has_paved_runway: Option<bool>,
    #[serde(default)]
    pub heliport: Option<bool>,
    #[serde(default)]
    pub has_water_runway: Option<bool>,
    #[serde(default)]
    pub longest_runway_length_ft: Option<f64>,
    #[serde(default)]
    pub longest_runway_heading_true_deg: Option<f64>,
    #[serde(default)]
    pub elevation_msl_ft: Option<f64>,
    #[serde(default)]
    pub obstacle: Option<ObstaclePointSemantics>,
    #[serde(default)]
    pub weather_camera: Option<WeatherCameraPointSemantics>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointTilePayload {
    pub schema_version: u32,
    pub layer: String,
    pub z: u32,
    pub x: u32,
    pub y: u32,
    pub records: Vec<PointVectorRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorAggregateTilePayload {
    pub schema_version: u32,
    pub z: u32,
    pub x: u32,
    pub y: u32,
    #[serde(default)]
    pub airports: Vec<PointVectorRecord>,
    #[serde(default)]
    pub fixes: Vec<PointVectorRecord>,
    #[serde(default)]
    pub navaids: Vec<PointVectorRecord>,
    #[serde(default)]
    pub airspace_refs: Vec<String>,
    #[serde(default)]
    pub airspace_labels: Vec<AirspaceLabelRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetarTileRecord {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetarTilePayload {
    pub schema_version: u32,
    pub layer: String,
    pub z: u32,
    pub x: u32,
    pub y: u32,
    pub records: Vec<MetarTileRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetarClouds {
    #[serde(default)]
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetarRecord {
    pub raw_text: String,
    #[serde(default, alias = "observed_at_utc", alias = "observation_time_utc")]
    pub observed_at_utc: Option<String>,
    pub station_id: String,
    #[serde(default)]
    pub flight_category: Option<String>,
    #[serde(default)]
    pub clouds: Option<MetarClouds>,
    pub longitude: f64,
    pub latitude: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PirepRecord {
    pub id: String,
    pub raw_text: String,
    #[serde(default)]
    pub observed_at_utc: Option<String>,
    #[serde(default)]
    pub report_type: Option<String>,
    pub longitude: f64,
    pub latitude: f64,
    pub symbol: String,
    pub icing: String,
    pub turbulence: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TafRecord {
    pub raw_text: String,
    #[serde(default)]
    pub issued_at_utc: Option<String>,
    pub station_id: String,
    pub longitude: f64,
    pub latitude: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TafProductPayload {
    pub schema_version: u32,
    pub version_label: String,
    #[serde(default)]
    pub generated_at_utc: Option<DateTime<Utc>>,
    #[serde(default)]
    pub taf_count: Option<u32>,
    pub tafs_by_station: HashMap<String, TafRecord>,
}

pub use notam_state::NotamRecord;

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotamProductPayload {
    pub schema_version: u32,
    pub version_label: String,
    #[serde(default)]
    pub notam_count: Option<u32>,
    pub notams_by_id: HashMap<String, NotamRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AirportNotamUiView {
    pub id: String,
    pub label: String,
    pub text: String,
    #[serde(skip)]
    priority: u8,
}

pub const NOTAM_DISPLAY_PROJECTION_SCHEMA_VERSION: u32 = 3;

/// Binary-safe form of the shared rendezvous key used across the worker boundary.
///
/// The publication key omits an absent airport in JSON. Prepared NOTAM projections
/// use postcard, whose positional struct encoding requires every field to be present.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NotamDisplayProcedureKey {
    pub kind: ProcedureRendezvousKind,
    pub identity: ProcedureRendezvousIdentity,
    pub airport_id: Option<String>,
}

impl From<&ProcedureRendezvousKey> for NotamDisplayProcedureKey {
    fn from(key: &ProcedureRendezvousKey) -> Self {
        Self {
            kind: key.kind,
            identity: key.identity.clone(),
            airport_id: key.airport_id.clone(),
        }
    }
}

impl NotamDisplayProcedureKey {
    fn publication_key(&self) -> ProcedureRendezvousKey {
        ProcedureRendezvousKey {
            kind: self.kind,
            identity: self.identity.clone(),
            airport_id: self.airport_id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotamDisplayRecord {
    pub id: String,
    pub airport_id: Option<String>,
    pub procedure_rendezvous_keys: BTreeSet<NotamDisplayProcedureKey>,
    pub label: String,
    pub text: String,
    pub priority: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotamDisplayCheckpoint {
    pub schema_version: u32,
    pub state_id: String,
    pub records: Vec<NotamDisplayRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotamDisplayDelta {
    pub schema_version: u32,
    pub from_state_id: String,
    pub to_state_id: String,
    pub mutations: Vec<NotamDisplayMutation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotamDisplayMutation {
    Upsert(NotamDisplayRecord),
    Remove { notam_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotamDisplayIndex {
    pub version_label: String,
    records: BTreeMap<String, NotamDisplayRecord>,
    by_airport: BTreeMap<String, Vec<String>>,
    by_procedure: BTreeMap<NotamDisplayProcedureKey, Vec<String>>,
}

impl NotamDisplayIndex {
    #[cfg(test)]
    pub fn from_payload(payload: NotamProductPayload) -> Result<Self, String> {
        if payload.schema_version != product_contracts::NOTAM_LIVE_FEED_CONTRACT_VERSION {
            return Err(format!(
                "unsupported NOTAM live-feed schema {}; expected {}",
                payload.schema_version,
                product_contracts::NOTAM_LIVE_FEED_CONTRACT_VERSION
            ));
        }
        let mut records = payload.notams_by_id.into_values().collect::<Vec<_>>();
        records.sort_by(|left, right| left.id.cmp(&right.id));
        if payload
            .notam_count
            .is_some_and(|count| count as usize != records.len())
        {
            return Err("NOTAM fixture count does not match records".to_string());
        }
        Self::from_projection_checkpoint(NotamDisplayCheckpoint {
            schema_version: NOTAM_DISPLAY_PROJECTION_SCHEMA_VERSION,
            state_id: payload.version_label,
            records: records
                .iter()
                .filter_map(project_notam_display_record)
                .collect(),
        })
        .map_err(|error| error.to_string())
    }

    pub fn from_projection_checkpoint(
        checkpoint: NotamDisplayCheckpoint,
    ) -> Result<Self, NotamStateError> {
        validate_notam_display_projection_schema(checkpoint.schema_version)?;
        validate_projection_record_order(&checkpoint.records)?;
        let mut index = Self {
            version_label: checkpoint.state_id,
            records: BTreeMap::new(),
            by_airport: BTreeMap::new(),
            by_procedure: BTreeMap::new(),
        };
        for record in checkpoint.records {
            index.upsert(record)?;
        }
        Ok(index)
    }

    pub fn apply_projection_delta(
        &mut self,
        delta: NotamDisplayDelta,
    ) -> Result<(), NotamStateError> {
        validate_notam_display_projection_schema(delta.schema_version)?;
        if self.version_label != delta.from_state_id {
            return Err(NotamStateError::BaseStateMismatch {
                expected: delta.from_state_id,
                actual: self.version_label.clone(),
            });
        }
        validate_projection_mutation_order(&delta.mutations)?;
        for mutation in delta.mutations {
            match mutation {
                NotamDisplayMutation::Upsert(record) => self.upsert(record)?,
                NotamDisplayMutation::Remove { notam_id } => {
                    self.remove(&notam_id)?;
                }
            }
        }
        self.version_label = delta.to_state_id;
        Ok(())
    }

    pub fn state_id(&self) -> &str {
        &self.version_label
    }

    fn upsert(&mut self, record: NotamDisplayRecord) -> Result<(), NotamStateError> {
        validate_projection_record(&record)?;
        self.remove(&record.id)?;
        if let Some(airport_id) = &record.airport_id {
            insert_projected_notam_id(&mut self.by_airport, airport_id.clone(), &record.id)?;
        }
        for key in &record.procedure_rendezvous_keys {
            insert_projected_notam_id(&mut self.by_procedure, key.clone(), &record.id)?;
        }
        self.records.insert(record.id.clone(), record);
        Ok(())
    }

    fn remove(&mut self, notam_id: &str) -> Result<(), NotamStateError> {
        let Some(record) = self.records.remove(notam_id) else {
            return Ok(());
        };
        if let Some(airport_id) = &record.airport_id {
            remove_projected_notam_id(&mut self.by_airport, airport_id, notam_id)?;
        }
        for key in &record.procedure_rendezvous_keys {
            remove_projected_notam_id(&mut self.by_procedure, key, notam_id)?;
        }
        Ok(())
    }

    fn airport_records(&self, airport_id: &str) -> Vec<&NotamDisplayRecord> {
        let airport_id = airport_id.trim().to_ascii_uppercase();
        self.by_airport
            .get(&airport_id)
            .into_iter()
            .flatten()
            .filter_map(|id| self.records.get(id))
            .collect()
    }

    pub fn procedure_records(
        &self,
        keys: &BTreeSet<ProcedureRendezvousKey>,
    ) -> Vec<&NotamDisplayRecord> {
        let ids = keys
            .iter()
            .map(NotamDisplayProcedureKey::from)
            .filter_map(|key| self.by_procedure.get(&key))
            .flatten()
            .cloned()
            .collect::<BTreeSet<_>>();
        ids.iter().filter_map(|id| self.records.get(id)).collect()
    }
}

fn insert_projected_notam_id<K: Ord>(
    index: &mut BTreeMap<K, Vec<String>>,
    key: K,
    notam_id: &str,
) -> Result<(), NotamStateError> {
    let ids = index.entry(key).or_default();
    match ids.binary_search_by(|id| id.as_str().cmp(notam_id)) {
        Ok(_) => Err(NotamStateError::Invariant(format!(
            "NOTAM {notam_id} is already indexed"
        ))),
        Err(position) => {
            ids.insert(position, notam_id.to_string());
            Ok(())
        }
    }
}

fn remove_projected_notam_id<K: Ord + std::fmt::Debug>(
    index: &mut BTreeMap<K, Vec<String>>,
    key: &K,
    notam_id: &str,
) -> Result<(), NotamStateError> {
    let remove_key = {
        let ids = index.get_mut(key).ok_or_else(|| {
            NotamStateError::Invariant(format!("NOTAM {notam_id} is missing display index {key:?}"))
        })?;
        let position = ids
            .binary_search_by(|id| id.as_str().cmp(notam_id))
            .map_err(|_| {
                NotamStateError::Invariant(format!(
                    "NOTAM {notam_id} is missing from display index {key:?}"
                ))
            })?;
        ids.remove(position);
        ids.is_empty()
    };
    if remove_key {
        index.remove(key);
    }
    Ok(())
}

pub fn notam_display_checkpoint(state: &NotamState) -> NotamDisplayCheckpoint {
    NotamDisplayCheckpoint {
        schema_version: NOTAM_DISPLAY_PROJECTION_SCHEMA_VERSION,
        state_id: state.state_id().to_string(),
        records: state
            .canonical_records()
            .filter_map(|(_, record)| project_notam_display_record(record))
            .collect(),
    }
}

pub fn notam_display_delta(
    state: &NotamState,
    delta: &NotamDelta,
) -> Result<NotamDisplayDelta, NotamStateError> {
    delta.validate_contract()?;
    if state.state_id() != delta.from_state_id {
        return Err(NotamStateError::BaseStateMismatch {
            expected: delta.from_state_id.clone(),
            actual: state.state_id().to_string(),
        });
    }
    notam_state::validate_mutation_order(&delta.mutations)?;
    let mutations = delta
        .mutations
        .iter()
        .filter_map(|mutation| match mutation {
            NotamMutation::Upsert { record } => project_notam_display_record(record)
                .map(NotamDisplayMutation::Upsert)
                .or_else(|| {
                    state
                        .record(&record.id)
                        .and_then(project_notam_display_record)
                        .map(|_| NotamDisplayMutation::Remove {
                            notam_id: record.id.clone(),
                        })
                }),
            NotamMutation::Remove { notam_id } => state
                .record(notam_id)
                .and_then(project_notam_display_record)
                .map(|_| NotamDisplayMutation::Remove {
                    notam_id: notam_id.clone(),
                }),
        })
        .collect();
    Ok(NotamDisplayDelta {
        schema_version: NOTAM_DISPLAY_PROJECTION_SCHEMA_VERSION,
        from_state_id: delta.from_state_id.clone(),
        to_state_id: delta.to_state_id.clone(),
        mutations,
    })
}

fn project_notam_display_record(record: &NotamRecord) -> Option<NotamDisplayRecord> {
    if !record.is_displayable() {
        return None;
    }
    let airport_id = record
        .airport_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_uppercase);
    let text = record
        .display_text()
        .expect("displayable NOTAM has display text");
    Some(NotamDisplayRecord {
        id: record.id.clone(),
        airport_id,
        procedure_rendezvous_keys: record
            .procedure_rendezvous_keys
            .iter()
            .map(NotamDisplayProcedureKey::from)
            .collect(),
        label: record
            .notam_keyword
            .as_deref()
            .unwrap_or("NOTAM")
            .trim()
            .to_ascii_uppercase(),
        text: text.to_string(),
        priority: airport_notam_priority(&record.airport_effects),
    })
}

fn validate_notam_display_projection_schema(schema_version: u32) -> Result<(), NotamStateError> {
    if schema_version != NOTAM_DISPLAY_PROJECTION_SCHEMA_VERSION {
        return Err(NotamStateError::Contract(format!(
            "unsupported NOTAM display projection schema {schema_version}; expected \
             {NOTAM_DISPLAY_PROJECTION_SCHEMA_VERSION}"
        )));
    }
    Ok(())
}

fn validate_projection_record(record: &NotamDisplayRecord) -> Result<(), NotamStateError> {
    if record.id.is_empty() || record.id.trim() != record.id {
        return Err(NotamStateError::InvalidRecord(format!(
            "invalid projected NOTAM ID {:?}",
            record.id
        )));
    }
    if let Some(airport_id) = &record.airport_id {
        if airport_id.is_empty()
            || airport_id.trim() != airport_id
            || *airport_id != airport_id.to_ascii_uppercase()
        {
            return Err(NotamStateError::InvalidRecord(format!(
                "invalid projected NOTAM airport {airport_id:?}"
            )));
        }
    }
    if record.airport_id.is_none() && record.procedure_rendezvous_keys.is_empty() {
        return Err(NotamStateError::InvalidRecord(format!(
            "projected NOTAM {} has no lookup identity",
            record.id
        )));
    }
    for key in &record.procedure_rendezvous_keys {
        key.publication_key()
            .validate()
            .map_err(NotamStateError::InvalidRecord)?;
    }
    if record.text.trim().is_empty() {
        return Err(NotamStateError::InvalidRecord(format!(
            "projected NOTAM {} has no display text",
            record.id
        )));
    }
    Ok(())
}

fn validate_projection_record_order(records: &[NotamDisplayRecord]) -> Result<(), NotamStateError> {
    let mut previous = None;
    for record in records {
        validate_projection_record(record)?;
        if previous.is_some_and(|previous| previous >= record.id.as_str()) {
            return Err(NotamStateError::InvalidOrdering(format!(
                "projected NOTAM records are not strictly ordered near {}",
                record.id
            )));
        }
        previous = Some(record.id.as_str());
    }
    Ok(())
}

fn validate_projection_mutation_order(
    mutations: &[NotamDisplayMutation],
) -> Result<(), NotamStateError> {
    let mut previous = None;
    for mutation in mutations {
        let id = match mutation {
            NotamDisplayMutation::Upsert(record) => {
                validate_projection_record(record)?;
                record.id.as_str()
            }
            NotamDisplayMutation::Remove { notam_id } => notam_id.as_str(),
        };
        if id.is_empty() || id.trim() != id {
            return Err(NotamStateError::InvalidRecord(format!(
                "invalid projected NOTAM mutation ID {id:?}"
            )));
        }
        if previous.is_some_and(|previous| previous >= id) {
            return Err(NotamStateError::InvalidOrdering(format!(
                "projected NOTAM mutations are not strictly ordered near {id}"
            )));
        }
        previous = Some(id);
    }
    Ok(())
}

pub const WEATHER_DETAIL_ADVISORY_TEXT: &str =
    "NOTAMs and weather may be incomplete; check official sources.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeatherDetailUiView {
    pub station_id: String,
    pub title: String,
    pub advisory_text: String,
    pub sections: Vec<WeatherDetailSectionUiView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metar_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metar_age_label: Option<String>,
    #[serde(default)]
    pub metar_age_warning: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taf_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taf_age_label: Option<String>,
    #[serde(default)]
    pub taf_age_warning: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notams: Vec<AirportNotamUiView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherDetailSectionKind {
    Text,
    Notams,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeatherDetailSectionUiView {
    pub kind: WeatherDetailSectionKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trailing_label: Option<String>,
    #[serde(default)]
    pub trailing_warning: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub empty_text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notams: Vec<AirportNotamUiView>,
}

#[derive(Debug, Clone, Default)]
pub struct WeatherStationAirportAliases {
    station_to_airport: HashMap<String, WeatherStationAirportAlias>,
    airport_to_station: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct WeatherStationAirportAlias {
    airport_id: String,
    airport_position: LatLon,
}

impl WeatherStationAirportAliases {
    pub(crate) fn from_station_to_airport(
        aliases: impl IntoIterator<Item = (String, String, LatLon)>,
    ) -> Self {
        let mut station_to_airport = HashMap::new();
        let mut airport_to_station = HashMap::new();
        for (station_id, airport_id, airport_position) in aliases {
            let station_id = station_id.trim().to_ascii_uppercase();
            let airport_id = airport_id.trim().to_ascii_uppercase();
            if station_id.is_empty()
                || airport_id.is_empty()
                || station_id == airport_id
                || !airport_position.lat.is_finite()
                || !airport_position.lon.is_finite()
            {
                continue;
            }
            station_to_airport.insert(
                station_id.clone(),
                WeatherStationAirportAlias {
                    airport_id: airport_id.clone(),
                    airport_position,
                },
            );
            airport_to_station.insert(airport_id, station_id);
        }
        Self {
            station_to_airport,
            airport_to_station,
        }
    }

    fn airport_id_for_station(&self, station_id: &str, station_position: LatLon) -> Option<&str> {
        self.station_to_airport
            .get(&station_id.trim().to_ascii_uppercase())
            .filter(|alias| {
                great_circle_distance_nm(alias.airport_position, station_position)
                    <= WEATHER_STATION_AIRPORT_ALIAS_MAX_DISTANCE_NM
            })
            .map(|alias| alias.airport_id.as_str())
    }

    fn station_id_for_airport(&self, airport_id: &str) -> Option<&str> {
        self.airport_to_station
            .get(&airport_id.trim().to_ascii_uppercase())
            .map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetarProductPayload {
    pub schema_version: u32,
    pub version_label: String,
    #[serde(default)]
    pub generated_at_utc: Option<DateTime<Utc>>,
    #[serde(default)]
    pub observed_at_utc: Option<DateTime<Utc>>,
    #[serde(default)]
    pub metar_count: Option<u32>,
    pub metars_by_station: HashMap<String, MetarRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PirepProductPayload {
    pub schema_version: u32,
    pub version_label: String,
    #[serde(default)]
    pub generated_at_utc: Option<DateTime<Utc>>,
    #[serde(default)]
    pub observed_at_utc: Option<DateTime<Utc>>,
    #[serde(default)]
    pub pirep_count: Option<u32>,
    pub pireps_by_id: HashMap<String, PirepRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceReferenceTilePayload {
    pub schema_version: u32,
    pub layer: String,
    pub z: u32,
    pub x: u32,
    pub y: u32,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceLabelTilePayload {
    pub schema_version: u32,
    pub layer: String,
    pub z: u32,
    pub x: u32,
    pub y: u32,
    pub labels: Vec<AirspaceLabelRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceLabelRecord {
    pub feature_id: String,
    pub text: String,
    pub lon: f64,
    pub lat: f64,
    #[serde(default)]
    pub rank: u32,
    #[serde(default)]
    pub score: Option<f64>,
    pub style_hint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceFeaturePayload {
    pub schema_version: u32,
    pub id: String,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub ident: String,
    pub airspace_class: String,
    pub style_hint: String,
    pub vertical: AirspaceVerticalPayload,
    pub bbox: [f64; 4],
    pub paths: Vec<AirspaceFeaturePath>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceVerticalPayload {
    pub upper: AirspaceLimitPayload,
    pub lower: AirspaceLimitPayload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceLimitPayload {
    pub display: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceFeaturePath {
    pub role: String,
    pub closed: bool,
    #[serde(default)]
    pub interior_side: Option<String>,
    pub start: [f64; 2],
    pub segments: Vec<AirspaceFeaturePathSegment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AirspaceFeaturePathSegment {
    Line {
        to: [f64; 2],
    },
    Arc {
        center: [f64; 2],
        radius_ft: f64,
        clockwise: bool,
        to: [f64; 2],
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TfrProductPayload {
    pub schema_version: u32,
    pub version_label: String,
    #[serde(default)]
    pub generated_at_utc: Option<DateTime<Utc>>,
    pub notam_count: u32,
    pub area_group_count: u32,
    pub areas: Vec<TfrAreaPayload>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TfrAreaPayload {
    pub notam_id: String,
    pub area_index: u32,
    pub schedule_fragments: Vec<TfrScheduleFragment>,
    pub upper_limit: TfrAltitudeLimit,
    pub lower_limit: TfrAltitudeLimit,
    pub polygon: Vec<TfrLatLonPoint>,
    pub summary_text: String,
    #[serde(default)]
    pub notam: Option<TfrNotamMetadata>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TfrNotamMetadata {
    pub record_id: String,
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub function: Option<String>,
    #[serde(default)]
    pub keyword: Option<String>,
    #[serde(default)]
    pub facility: Option<String>,
    #[serde(default)]
    pub issued_utc: Option<String>,
    #[serde(default)]
    pub effective_start_utc: Option<String>,
    #[serde(default)]
    pub effective_end_utc: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub local_text: Option<String>,
    #[serde(default)]
    pub icao_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TfrScheduleFragment {
    pub kind: String,
    pub value_utc: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TfrAltitudeLimit {
    pub value_text: String,
    pub unit: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TfrLatLonPoint {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceFeatureRequest {
    pub id: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct VectorOverlayInputRequests {
    pub needed_vector_tiles: Vec<VectorTileRequest>,
    pub needed_airspace_features: Vec<AirspaceFeatureRequest>,
}

struct PointVectorTileScan {
    tile_count: usize,
    needed_tiles: Vec<VectorTileRequest>,
}

struct AirspaceInputScan {
    needed_tiles: Vec<VectorTileRequest>,
    feature_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorIdentLabelStyle {
    #[default]
    Default,
    FlightPlan,
    ActiveFlightPlan,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisibleMapFeature {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub symbol_kind: String,
    pub style_class: String,
    #[serde(default)]
    pub obstacle_variant: Option<String>,
    #[serde(default)]
    pub obstacle_tone: Option<String>,
    pub screen_x: f64,
    pub screen_y: f64,
    pub towered: bool,
    pub fuel_available: bool,
    pub has_paved_runway: Option<bool>,
    pub heliport: Option<bool>,
    pub has_water_runway: Option<bool>,
    pub runway_length_ratio: f64,
    pub longest_runway_heading_true_deg: Option<f64>,
    #[serde(default)]
    pub label_style: VectorIdentLabelStyle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisibleMetarFeature {
    pub station_id: String,
    pub screen_x: f64,
    pub screen_y: f64,
    pub flight_category: String,
    pub ceiling_amount: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisiblePirepFeature {
    pub id: String,
    pub screen_x: f64,
    pub screen_y: f64,
    pub symbol: String,
    pub icing: String,
    pub turbulence: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceDisplayStyle {
    pub fill_color_key: String,
    pub fill_opacity: f64,
    pub strokes: Vec<AirspaceDisplayStroke>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceDisplayStroke {
    pub color_key: String,
    pub width_px: f64,
    pub dash_px: Vec<f64>,
    pub line_cap: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AirspaceScreenPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineRegionCatalog {
    pub schema_version: u32,
    pub regions: Vec<OfflineRegionRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineRegionRecord {
    pub id: String,
    pub kind: String,
    pub region_id: String,
    pub label: String,
    pub color_key: String,
    #[serde(default)]
    pub summary: Vec<OfflineRegionSummaryEntry>,
    pub polygons: Vec<Vec<LatLon>>,
    pub label_position: LatLon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineRegionSummaryEntry {
    pub action: String,
    pub cycle: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineRegionDisplay {
    pub id: String,
    pub kind: String,
    pub region_id: String,
    pub label: String,
    pub color_key: String,
    pub summary: Vec<OfflineRegionSummaryEntry>,
    pub points: Vec<AirspaceScreenPoint>,
    pub label_x: f64,
    pub label_y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceDisplaySubpath {
    pub closed: bool,
    #[serde(skip)]
    pub interior_side: Option<String>,
    pub points: Vec<AirspaceScreenPoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceDecorationPath {
    pub color_key: String,
    pub width_px: f64,
    pub line_cap: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<AirspaceDisplaySubpath>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<[f64; 4]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceDisplayPath {
    pub id: String,
    pub name: String,
    pub style_key: String,
    pub style: AirspaceDisplayStyle,
    pub paths: Vec<AirspaceDisplaySubpath>,
    pub decorations: Vec<AirspaceDecorationPath>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceDisplayLabel {
    pub feature_id: String,
    pub glyph: AirspaceLimitGlyph,
    pub screen_x: f64,
    pub screen_y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceLimitGlyph {
    pub upper: String,
    pub lower: String,
    pub style_key: String,
    pub color_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapOverlayQueryResult {
    #[serde(skip)]
    pub needed_vector_tiles: Vec<VectorTileRequest>,
    #[serde(skip)]
    pub needed_metar_tiles: Vec<VectorTileRequest>,
    #[serde(skip)]
    pub needed_airspace_features: Vec<AirspaceFeatureRequest>,
    #[serde(skip)]
    pub needed_metars: bool,
    #[serde(skip)]
    pub needed_tfrs: bool,
    #[serde(skip)]
    pub data_status_records: Vec<DataStatusRecord>,
    pub visible_features: Vec<VisibleMapFeature>,
    #[serde(default)]
    pub flight_plan_features: Vec<VisibleMapFeature>,
    pub visible_metars: Vec<VisibleMetarFeature>,
    pub visible_pireps: Vec<VisiblePirepFeature>,
    #[serde(default)]
    pub visible_traffic: Vec<crate::VisibleAdsbTraffic>,
    #[serde(default)]
    pub traffic_next_refresh_epoch_ms: Option<i64>,
    pub airspace_paths: Vec<AirspaceDisplayPath>,
    pub tfr_paths: Vec<AirspaceDisplayPath>,
    pub airspace_labels: Vec<AirspaceDisplayLabel>,
    pub offline_regions: Vec<OfflineRegionDisplay>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapSelectionQueryResult {
    pub click_lat: f64,
    pub click_lon: f64,
    #[serde(default)]
    pub initial_selected_item_id: Option<String>,
    pub categories: Vec<MapSelectionCategory>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapSelectionForNavRefResult {
    pub position: LatLon,
    pub target_zoom: f64,
    pub selection: MapSelectionQueryResult,
    pub selected_item_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NavRefSelectionPoint {
    pub nav_ref: NavRef,
    pub position: LatLon,
    pub symbol: NavSymbolFeature,
    pub feature_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapSelectionCategory {
    pub id: String,
    pub label: String,
    pub items: Vec<MapSelectionItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapSelectionItem {
    pub id: String,
    pub label: String,
    pub sublabel: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub distance: Option<String>,
    #[serde(default)]
    pub secondary_description: Option<String>,
    #[serde(
        default,
        rename = "distance_target",
        skip_serializing_if = "Option::is_none"
    )]
    pub position: Option<LatLon>,
    #[serde(skip)]
    pub elevation_msl_ft: Option<f64>,
    #[serde(default)]
    pub detail_text: Option<String>,
    pub highlight: MapSelectionHighlight,
    #[serde(default)]
    pub nav_ref: Option<NavRef>,
    #[serde(default)]
    pub symbol_feature: Option<NavSymbolFeature>,
    #[serde(default)]
    pub metar_feature: Option<VisibleMetarFeature>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weather_detail: Option<WeatherDetailUiView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automatic_action_uid: Option<String>,
    #[serde(default)]
    pub pirep_feature: Option<VisiblePirepFeature>,
    #[serde(default)]
    pub airspace_icon: Option<AirspaceDisplayPath>,
    pub actions: Vec<MapSelectionAction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MapSelectionHighlight {
    FeatureRef { id: String },
    Metar { station_id: String },
    Pirep { id: String },
    AdsbTraffic { id: String },
    OfflineRegion { id: String },
    Spot { lat: f64, lon: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapSelectionAction {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub display_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_uid: Option<String>,
    #[serde(default)]
    pub placeholder: bool,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub detail_text: Option<String>,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub detail_title: Option<String>,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub detail_status: Option<MapSelectionDetailStatus>,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub weather_detail: Option<WeatherDetailUiView>,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub airport_info_airport_id: Option<String>,
    #[serde(default)]
    pub disabled_reason: Option<String>,
    #[serde(default)]
    pub airspace_limit: Option<AirspaceLimitGlyph>,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub session_action: Option<String>,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub flight_plan_row_action: Option<MapSelectionFlightPlanRowAction>,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub navigation: Option<MapSelectionNavigationAction>,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub external_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapSelectionActionDecision {
    pub perform_session_mutation: bool,
    pub dismiss_selection: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<MapSelectionActionEffect>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MapSelectionActionEffect {
    ShowWeather {
        detail: WeatherDetailUiView,
    },
    LoadAirportInfo {
        airport_id: String,
        loading_text: String,
        failure_prefix: String,
    },
    ShowDetail {
        title: String,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<MapSelectionDetailStatus>,
    },
    OpenPlateTarget {
        airport_id: String,
        target: String,
        chart_id: String,
    },
    OpenExternalUrl {
        url: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapSelectionDetailStatus {
    pub text: String,
    pub color_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapSelectionFlightPlanRowAction {
    pub row_uid: String,
    pub action_uid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MapSelectionNavigationAction {
    OpenPlateTarget {
        airport_id: String,
        target: String,
        chart_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MapSelectionSessionAction {
    InsertWaypointBestPosition { nav_ref: NavRef },
    ActivateDirectToNavRef { nav_ref: NavRef },
    FollowAdsbRegistration { registration: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AirportPlateAvailability {
    pub plates: bool,
    pub csup: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapOverlayConfig {
    pub airspace_reference_tile_min_zoom: u32,
    pub airspace_reference_tile_max_zoom: u32,
    pub airspace_label_tile_min_zoom: u32,
    pub airspace_label_tile_max_zoom: u32,
    pub airport_layer: PointTileLayerConfig,
    pub fix_layer: PointTileLayerConfig,
    pub nav_layer: PointTileLayerConfig,
    pub obstacle_layer: Option<ObstacleLayerConfig>,
    pub metar_layer: Option<PointTileLayerConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointTileLayerConfig {
    pub min_zoom: u32,
    pub max_zoom: u32,
    pub available_zooms: Vec<u32>,
    pub tile_path_template: Option<String>,
}

pub fn live_metar_layer_config() -> PointTileLayerConfig {
    PointTileLayerConfig {
        min_zoom: 5,
        max_zoom: 7,
        available_zooms: vec![5, 6, 7],
        tile_path_template: None,
    }
}

#[derive(Debug, Deserialize)]
struct VectorOverlayManifest {
    #[serde(default)]
    point_layers: HashMap<String, VectorPointLayerManifest>,
    airspace: VectorAirspaceManifest,
}

#[derive(Debug, Deserialize)]
struct ObstacleOverlayManifest {
    #[serde(default)]
    point_layers: HashMap<String, VectorPointLayerManifest>,
}

#[derive(Debug, Deserialize)]
struct VectorPointLayerManifest {
    #[serde(default)]
    min_zoom: Option<u32>,
    #[serde(default)]
    max_zoom: Option<u32>,
    #[serde(default)]
    available_zooms: Vec<u32>,
    #[serde(default)]
    tile_path_template: Option<String>,
    #[serde(default)]
    zoom_levels: Vec<ObstacleZoomLevelConfig>,
}

#[derive(Debug, Deserialize)]
struct VectorAirspaceManifest {
    reference_tile_min_zoom: u32,
    reference_tile_max_zoom: u32,
    label_tile_min_zoom: u32,
    label_tile_max_zoom: u32,
}

pub fn map_overlay_config_from_vector_manifest_json(
    vector_manifest_json: &str,
) -> AppResult<MapOverlayConfig> {
    let manifest: VectorOverlayManifest =
        serde_json::from_str(vector_manifest_json).map_err(|err| AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("failed to parse vector overlay manifest: {err}"),
        })?;
    if manifest.airspace.reference_tile_min_zoom > manifest.airspace.reference_tile_max_zoom {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: "vector overlay manifest has inverted airspace reference tile zoom range"
                .to_string(),
        });
    }
    if manifest.airspace.label_tile_min_zoom > manifest.airspace.label_tile_max_zoom {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: "vector overlay manifest has inverted airspace label tile zoom range"
                .to_string(),
        });
    }
    let airport_layer = required_point_tile_layer_config(&manifest, "airport")?;
    let fix_layer = required_point_tile_layer_config(&manifest, "fix")?;
    let nav_layer = required_point_tile_layer_config(&manifest, "nav")?;
    Ok(MapOverlayConfig {
        airspace_reference_tile_min_zoom: manifest.airspace.reference_tile_min_zoom,
        airspace_reference_tile_max_zoom: manifest.airspace.reference_tile_max_zoom,
        airspace_label_tile_min_zoom: manifest.airspace.label_tile_min_zoom,
        airspace_label_tile_max_zoom: manifest.airspace.label_tile_max_zoom,
        airport_layer,
        fix_layer,
        nav_layer,
        obstacle_layer: None,
        metar_layer: Some(live_metar_layer_config()),
    })
}

pub fn obstacle_layer_config_from_live_manifest_value(
    value: serde_json::Value,
) -> AppResult<ObstacleLayerConfig> {
    let manifest: ObstacleOverlayManifest =
        serde_json::from_value(value).map_err(|err| AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("failed to parse obstacle overlay manifest: {err}"),
        })?;
    let obstacle = manifest
        .point_layers
        .get("obstacle")
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidManifest,
            message: "obstacle overlay manifest is missing obstacle layer".to_string(),
        })?;
    obstacle_layer_config_from_manifest(obstacle)
}

fn required_point_tile_layer_config(
    manifest: &VectorOverlayManifest,
    layer_name: &str,
) -> AppResult<PointTileLayerConfig> {
    let layer = manifest
        .point_layers
        .get(layer_name)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("vector overlay manifest is missing required {layer_name} layer"),
        })?;
    point_tile_layer_config_from_manifest(layer_name, layer)
}

fn point_tile_layer_config_from_manifest(
    layer_name: &str,
    manifest: &VectorPointLayerManifest,
) -> AppResult<PointTileLayerConfig> {
    let available_zooms = if manifest.available_zooms.is_empty() {
        match (manifest.min_zoom, manifest.max_zoom) {
            (Some(min_zoom), Some(max_zoom)) if min_zoom <= max_zoom => {
                (min_zoom..=max_zoom).collect()
            }
            _ => Vec::new(),
        }
    } else {
        let mut values = manifest.available_zooms.clone();
        values.sort_unstable();
        values.dedup();
        values
    };
    if available_zooms.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!(
                "vector overlay manifest {layer_name} layer is missing available zooms"
            ),
        });
    }
    let min_zoom = manifest
        .min_zoom
        .unwrap_or(*available_zooms.first().unwrap());
    let max_zoom = manifest
        .max_zoom
        .unwrap_or(*available_zooms.last().unwrap());
    if min_zoom > max_zoom {
        return Err(AppError {
            kind: AppErrorKind::InvalidManifest,
            message: format!("vector overlay manifest has inverted {layer_name} zoom range"),
        });
    }
    Ok(PointTileLayerConfig {
        min_zoom,
        max_zoom,
        available_zooms,
        tile_path_template: manifest.tile_path_template.clone(),
    })
}

fn obstacle_layer_config_from_manifest(
    manifest: &VectorPointLayerManifest,
) -> AppResult<ObstacleLayerConfig> {
    let point_config = point_tile_layer_config_from_manifest("obstacle", manifest)?;
    let mut zoom_levels = HashMap::new();
    let mut high_detail_zoom = *point_config.available_zooms.last().unwrap();
    for level in &manifest.zoom_levels {
        zoom_levels.insert(level.zoom, level.clone());
        if !level.filtered {
            high_detail_zoom = high_detail_zoom.max(level.zoom);
        }
    }
    Ok(ObstacleLayerConfig {
        min_zoom: point_config.min_zoom,
        max_zoom: point_config.max_zoom,
        available_zooms: point_config.available_zooms,
        high_detail_zoom,
        zoom_levels,
    })
}

pub fn visible_point_tile_window(
    config: &MapOverlayConfig,
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
) -> Vec<VectorTileRequest> {
    visible_point_tile_window_with_display_scale(config, viewport, width_px, height_px, 1.0)
}

pub fn visible_point_tile_window_with_display_scale(
    config: &MapOverlayConfig,
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    point_display_scale: f64,
) -> Vec<VectorTileRequest> {
    dedupe_vector_tile_requests(
        visible_point_display_tile_window(
            config,
            viewport,
            width_px,
            height_px,
            point_display_scale,
        )
        .into_iter()
        .map(|tile| tile.request),
    )
}

fn visible_point_display_tile_window(
    config: &MapOverlayConfig,
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    point_display_scale: f64,
) -> Vec<DisplayVectorTile> {
    let mut tiles = Vec::new();
    let effective_zoom = effective_point_display_zoom(viewport, point_display_scale);
    let desired_point_tile_zoom = point_vector_tile_zoom(effective_zoom);
    if effective_zoom >= AIRPORT_MIN_DISPLAY_ZOOM {
        tiles.extend(visible_layer_display_tile_window(
            "airport",
            nearest_available_layer_zoom(&config.airport_layer, desired_point_tile_zoom),
            viewport,
            width_px,
            height_px,
        ));
    }
    if effective_zoom >= FIX_MIN_DISPLAY_ZOOM {
        tiles.extend(visible_layer_display_tile_window(
            "fix",
            nearest_available_layer_zoom(&config.fix_layer, desired_point_tile_zoom),
            viewport,
            width_px,
            height_px,
        ));
    }
    if effective_zoom >= NAV_MIN_DISPLAY_ZOOM {
        tiles.extend(visible_layer_display_tile_window(
            "nav",
            nearest_available_layer_zoom(&config.nav_layer, desired_point_tile_zoom),
            viewport,
            width_px,
            height_px,
        ));
    }
    tiles
}

pub(crate) fn visible_obstacle_tile_window(
    config: &ObstacleLayerConfig,
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    obstacle_context: Option<&ObstacleOverlayContext>,
    point_display_scale: f64,
) -> Vec<VectorTileRequest> {
    let effective_zoom = effective_point_display_zoom(viewport, point_display_scale);
    if effective_zoom < OBSTACLE_MIN_DISPLAY_ZOOM {
        return Vec::new();
    }
    let display_zoom = nearest_available_zoom(config, effective_zoom.floor() as u32);
    let mut requests =
        visible_layer_tile_window("obstacle", display_zoom, viewport, width_px, height_px);
    let Some(context) = obstacle_context else {
        return requests;
    };
    if display_zoom >= config.high_detail_zoom {
        return requests;
    }
    let diameter_nm = context
        .ground_speed_kt
        .filter(|value| value.is_finite() && *value > 0.0)
        .map(|speed| speed * (OBSTACLE_LOOKAHEAD_MINUTES / 60.0))
        .unwrap_or(OBSTACLE_LOOKAHEAD_DEFAULT_DIAMETER_NM);
    let radius_nm = diameter_nm / 2.0;
    let center = context
        .track_deg_true
        .filter(|value| value.is_finite())
        .map(|track_deg| {
            destination_point(
                context.position,
                track_deg,
                diameter_nm * OBSTACLE_LOOKAHEAD_CENTER_OFFSET_DIAMETER_RATIO,
            )
        })
        .unwrap_or(context.position);
    let mut seen = requests
        .iter()
        .map(|tile| tile_key(&tile.layer, tile.z, tile.x, tile.y))
        .collect::<HashSet<_>>();
    for tile in tile_window_for_circle("obstacle", config.high_detail_zoom, center, radius_nm) {
        if seen.insert(tile_key(&tile.layer, tile.z, tile.x, tile.y)) {
            requests.push(tile);
        }
    }
    requests
}

fn effective_point_display_zoom(viewport: &MapViewport, point_display_scale: f64) -> f64 {
    MapSurfaceMetrics::new(*viewport, 1.0, 1.0, point_display_scale).effective_display_zoom()
}

fn normalized_display_scale(display_scale: f64) -> f64 {
    if display_scale.is_finite() {
        display_scale.max(0.1)
    } else {
        1.0
    }
}

fn point_vector_tile_zoom(effective_zoom: f64) -> u32 {
    effective_zoom.floor().clamp(0.0, POINT_TILE_ZOOM as f64) as u32
}

fn nearest_available_zoom(config: &ObstacleLayerConfig, desired_zoom: u32) -> u32 {
    nearest_available_zoom_in(
        config.min_zoom,
        config.max_zoom,
        &config.available_zooms,
        desired_zoom,
    )
}

pub(crate) fn nearest_available_layer_zoom(
    config: &PointTileLayerConfig,
    desired_zoom: u32,
) -> u32 {
    nearest_available_zoom_in(
        config.min_zoom,
        config.max_zoom,
        &config.available_zooms,
        desired_zoom,
    )
}

fn nearest_available_zoom_in(
    min_zoom: u32,
    max_zoom: u32,
    available_zooms: &[u32],
    desired_zoom: u32,
) -> u32 {
    let clamped = desired_zoom.clamp(min_zoom, max_zoom);
    available_zooms
        .iter()
        .copied()
        .filter(|zoom| *zoom <= clamped)
        .max()
        .unwrap_or_else(|| *available_zooms.first().unwrap())
}

fn visible_layer_tile_window(
    layer: &str,
    zoom: u32,
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
) -> Vec<VectorTileRequest> {
    dedupe_vector_tile_requests(
        visible_layer_display_tile_window(layer, zoom, viewport, width_px, height_px)
            .into_iter()
            .map(|tile| tile.request),
    )
}

fn visible_layer_display_tile_window(
    layer: &str,
    zoom: u32,
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
) -> Vec<DisplayVectorTile> {
    let center_world = lat_lon_to_world(viewport.center);
    let scale = 2.0_f64.powf(viewport.zoom);
    let min_world_x = center_world.x - width_px / 2.0 / scale;
    let max_world_x = center_world.x + width_px / 2.0 / scale;
    let min_world_y = center_world.y - height_px / 2.0 / scale;
    let max_world_y = center_world.y + height_px / 2.0 / scale;
    let tile_world_size = WORLD_SIZE / (2_u32.pow(zoom) as f64);
    let level_scale = 2_u32.pow(zoom) as i32;
    let max_index = level_scale - 1;
    let x_start = (min_world_x / tile_world_size).floor() as i32;
    let x_end = (max_world_x / tile_world_size).floor() as i32;
    let y_start = (min_world_y / tile_world_size).floor() as i32;
    let y_end = (max_world_y / tile_world_size).floor() as i32;
    let mut tiles = Vec::new();

    for y in y_start.max(0)..=y_end.min(max_index) {
        for display_x in x_start..=x_end {
            let x = positive_mod_i32(display_x, level_scale);
            let world_copy = (display_x - x) / level_scale;
            tiles.push(DisplayVectorTile {
                request: VectorTileRequest {
                    layer: layer.to_string(),
                    z: zoom,
                    x: x as u32,
                    y: y as u32,
                },
                world_x_offset: world_copy as f64 * WORLD_SIZE,
            });
        }
    }

    tiles
}

fn dedupe_vector_tile_requests(
    requests: impl IntoIterator<Item = VectorTileRequest>,
) -> Vec<VectorTileRequest> {
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::new();
    for request in requests {
        if seen.insert((request.layer.clone(), request.z, request.x, request.y)) {
            deduped.push(request);
        }
    }
    deduped
}

fn tile_window_for_circle(
    layer: &str,
    zoom: u32,
    center: LatLon,
    radius_nm: f64,
) -> Vec<VectorTileRequest> {
    let center_world = lat_lon_to_world(center);
    let world_radius = radius_nm / world_nm_per_unit(center.lat);
    let tile_world_size = WORLD_SIZE / (2_u32.pow(zoom) as f64);
    let level_scale = 2_u32.pow(zoom) as i32;
    let max_index = level_scale - 1;
    let min_world_x = center_world.x - world_radius;
    let max_world_x = center_world.x + world_radius;
    let min_world_y = center_world.y - world_radius;
    let max_world_y = center_world.y + world_radius;
    let x_start = (min_world_x / tile_world_size).floor() as i32;
    let x_end = (max_world_x / tile_world_size).floor() as i32;
    let y_start = (min_world_y / tile_world_size).floor() as i32;
    let y_end = (max_world_y / tile_world_size).floor() as i32;
    let mut tiles = Vec::new();

    let mut seen = BTreeSet::new();
    for y in y_start.max(0)..=y_end.min(max_index) {
        for display_x in x_start..=x_end {
            let x = positive_mod_i32(display_x, level_scale);
            if !seen.insert((x, y)) {
                continue;
            }
            if tile_intersects_circle(zoom, x as u32, y as u32, center, radius_nm) {
                tiles.push(VectorTileRequest {
                    layer: layer.to_string(),
                    z: zoom,
                    x: x as u32,
                    y: y as u32,
                });
            }
        }
    }

    tiles
}

fn positive_mod_i32(value: i32, modulus: i32) -> i32 {
    ((value % modulus) + modulus) % modulus
}

fn tile_intersects_circle(zoom: u32, x: u32, y: u32, center: LatLon, radius_nm: f64) -> bool {
    let tile_bounds = tile_bounds_xyz(zoom, x, y);
    let closest_lat = center.lat.clamp(tile_bounds.south, tile_bounds.north);
    let closest_lon = center.lon.clamp(tile_bounds.west, tile_bounds.east);
    great_circle_distance_nm(
        center,
        LatLon {
            lat: closest_lat,
            lon: closest_lon,
        },
    ) <= radius_nm
}

fn tile_bounds_xyz(zoom: u32, x: u32, y: u32) -> TileBounds {
    let tile_world_size = WORLD_SIZE / (2_u32.pow(zoom) as f64);
    let northwest = world_to_lat_lon(WorldPoint {
        x: x as f64 * tile_world_size,
        y: y as f64 * tile_world_size,
    });
    let southeast = world_to_lat_lon(WorldPoint {
        x: (x + 1) as f64 * tile_world_size,
        y: (y + 1) as f64 * tile_world_size,
    });
    TileBounds {
        south: southeast.lat.min(northwest.lat),
        north: southeast.lat.max(northwest.lat),
        west: northwest.lon.min(southeast.lon),
        east: northwest.lon.max(southeast.lon),
    }
}

fn world_nm_per_unit(latitude_deg: f64) -> f64 {
    let nm_per_degree_lon = 60.0 * latitude_deg.to_radians().cos().abs().max(0.01);
    WORLD_SIZE / 360.0 * nm_per_degree_lon
}

pub struct MapOverlayQuery<'a> {
    pub config: &'a MapOverlayConfig,
    pub display_vectors: bool,
    pub display_metars: bool,
    pub offline_region_records: &'a [OfflineRegionRecord],
    pub obstacle_context: Option<&'a ObstacleOverlayContext>,
    pub vector_tile_cache: &'a HashMap<String, VectorAggregateTilePayload>,
    pub obstacle_tile_cache: &'a HashMap<String, PointTilePayload>,
    pub metar_tile_cache: &'a HashMap<String, MetarTilePayload>,
    pub metar_payload: Option<&'a MetarProductPayload>,
    pub pirep_payload: Option<&'a PirepProductPayload>,
    pub airspace_feature_cache: &'a HashMap<String, AirspaceFeaturePayload>,
    pub tfr_payload: Option<&'a TfrProductPayload>,
    pub protected_point_features: &'a [VisibleMapFeature],
    pub tfr_reference_utc: Option<DateTime<Utc>>,
}

impl<'a> MapOverlayQuery<'a> {
    pub fn new(
        config: &'a MapOverlayConfig,
        vector_tile_cache: &'a HashMap<String, VectorAggregateTilePayload>,
        obstacle_tile_cache: &'a HashMap<String, PointTilePayload>,
        metar_tile_cache: &'a HashMap<String, MetarTilePayload>,
        airspace_feature_cache: &'a HashMap<String, AirspaceFeaturePayload>,
    ) -> Self {
        Self {
            config,
            display_vectors: false,
            display_metars: false,
            offline_region_records: &[],
            obstacle_context: None,
            vector_tile_cache,
            obstacle_tile_cache,
            metar_tile_cache,
            metar_payload: None,
            pirep_payload: None,
            airspace_feature_cache,
            tfr_payload: None,
            protected_point_features: &[],
            tfr_reference_utc: None,
        }
    }
}

#[derive(Clone, Copy)]
struct MapProjectionContext<'a> {
    metrics: &'a MapSurfaceMetrics,
    center_world: WorldPoint,
    scale: f64,
}

impl<'a> MapProjectionContext<'a> {
    fn new(metrics: &'a MapSurfaceMetrics) -> Self {
        Self {
            metrics,
            center_world: lat_lon_to_world(metrics.viewport.center),
            scale: 2.0_f64.powf(metrics.viewport.zoom),
        }
    }
}

pub fn query_map_overlay(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    query: MapOverlayQuery<'_>,
) -> MapOverlayQueryResult {
    query_map_overlay_with_point_display_scale(viewport, width_px, height_px, 1.0, query)
}

pub fn query_map_overlay_with_point_display_scale(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    point_display_scale: f64,
    query: MapOverlayQuery<'_>,
) -> MapOverlayQueryResult {
    let metrics = MapSurfaceMetrics::new(*viewport, width_px, height_px, point_display_scale);
    query_map_overlay_for_surface(&metrics, query)
}

pub fn vector_overlay_input_requests(
    metrics: &MapSurfaceMetrics,
    config: &MapOverlayConfig,
    vector_tile_cache: &HashMap<String, VectorAggregateTilePayload>,
    airspace_feature_cache: &HashMap<String, AirspaceFeaturePayload>,
) -> VectorOverlayInputRequests {
    let viewport = &metrics.viewport;
    let width_px = metrics.width_px;
    let height_px = metrics.height_px;
    let point_display_scale = metrics.display_scale;
    let point_scan = visit_point_vector_tiles(
        config,
        viewport,
        width_px,
        height_px,
        point_display_scale,
        vector_tile_cache,
        |_tile, _payload| {},
    );

    let airspace = airspace_overlay_input_requests(
        viewport,
        width_px,
        height_px,
        config,
        vector_tile_cache,
        airspace_feature_cache,
        point_display_scale,
    );

    VectorOverlayInputRequests {
        needed_vector_tiles: merge_aggregate_vector_tile_requests(
            point_scan.needed_tiles,
            airspace.needed_vector_tiles,
        ),
        needed_airspace_features: airspace.needed_airspace_features,
    }
}

fn visit_point_vector_tiles<F>(
    config: &MapOverlayConfig,
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    point_display_scale: f64,
    vector_tile_cache: &HashMap<String, VectorAggregateTilePayload>,
    mut on_loaded: F,
) -> PointVectorTileScan
where
    F: FnMut(&DisplayVectorTile, &VectorAggregateTilePayload),
{
    let tile_window = visible_point_display_tile_window(
        config,
        viewport,
        width_px,
        height_px,
        point_display_scale,
    );
    let tile_count = tile_window.len();
    let mut needed_tiles = Vec::new();
    let mut needed_seen = BTreeSet::new();
    for tile in &tile_window {
        let key = aggregate_vector_tile_cache_key(tile.request.z, tile.request.x, tile.request.y);
        let Some(payload) = vector_tile_cache.get(&key) else {
            if needed_seen.insert(key) {
                needed_tiles.push(aggregate_vector_tile_request(
                    tile.request.z,
                    tile.request.x,
                    tile.request.y,
                ));
            }
            continue;
        };
        on_loaded(tile, payload);
    }
    PointVectorTileScan {
        tile_count,
        needed_tiles,
    }
}

pub fn query_map_overlay_for_surface(
    metrics: &MapSurfaceMetrics,
    query: MapOverlayQuery<'_>,
) -> MapOverlayQueryResult {
    let MapOverlayQuery {
        config,
        display_vectors,
        display_metars,
        offline_region_records,
        obstacle_context,
        vector_tile_cache,
        obstacle_tile_cache,
        metar_tile_cache,
        metar_payload,
        pirep_payload,
        airspace_feature_cache,
        tfr_payload,
        protected_point_features,
        tfr_reference_utc,
    } = query;
    let total_started_at = core_clock_ms();
    let viewport = &metrics.viewport;
    let width_px = metrics.width_px;
    let height_px = metrics.height_px;
    let point_display_scale = metrics.display_scale;
    let decision = overlay_surface_decision(*metrics, config);
    let mut needed_vector_tiles = Vec::new();
    let mut visible_features = Vec::new();
    let mut vector_budget = VectorDisplayBudgetAudit::default();
    let mut vector_budget_buckets = vector_display_budget_buckets();
    let mut weather_camera_airport_idents = HashMap::new();
    let projection = MapProjectionContext::new(metrics);
    let center_world = projection.center_world;
    let scale = projection.scale;
    let offline_started_at = core_clock_ms();
    let offline_regions = project_offline_regions(
        offline_region_records,
        center_world,
        scale,
        width_px,
        height_px,
    );
    let offline_ms = overlay_elapsed_ms(offline_started_at);
    let mut point_tile_count = 0_usize;
    let mut point_vector_ms = 0_u64;
    let mut obstacle_ms = 0_u64;

    if display_vectors {
        let point_vector_started_at = core_clock_ms();
        let point_scan = visit_point_vector_tiles(
            config,
            viewport,
            width_px,
            height_px,
            point_display_scale,
            vector_tile_cache,
            |tile, payload| {
                for record in vector_tile_point_records(payload, &tile.request.layer) {
                    vector_budget.scanned_records += 1;
                    if !should_display_record(record) {
                        bump_layer_count(&mut vector_budget.hidden_by_layer, &tile.request.layer);
                        continue;
                    }
                    let Some(symbol) = point_vector_record_to_symbol_feature(
                        record,
                        obstacle_context.and_then(|context| context.altitude_ft),
                    ) else {
                        bump_layer_count(
                            &mut vector_budget.no_symbol_by_layer,
                            &tile.request.layer,
                        );
                        continue;
                    };
                    vector_budget.displayable_records += 1;
                    let point = world_to_screen_with_x_offset(
                        center_world,
                        scale,
                        width_px,
                        height_px,
                        LatLon {
                            lat: record.lat,
                            lon: record.lon,
                        },
                        tile.world_x_offset,
                    );
                    let feature = visible_map_feature_from_symbol(
                        record.id.clone(),
                        symbol,
                        point,
                        VectorIdentLabelStyle::Default,
                    );
                    remember_weather_camera_airport_ident(
                        record,
                        &mut weather_camera_airport_idents,
                    );
                    if let Some(bucket_index) =
                        vector_display_budget_bucket_index(&feature.symbol_kind)
                    {
                        vector_budget_buckets[bucket_index].features.push(feature);
                    }
                }
            },
        );
        point_tile_count = point_scan.tile_count;
        needed_vector_tiles = point_scan.needed_tiles;
        point_vector_ms = overlay_elapsed_ms(point_vector_started_at);
        let obstacle_started_at = core_clock_ms();
        if let Some(obstacle_layer) = config.obstacle_layer.as_ref() {
            let obstacle_tiles = visible_obstacle_tile_window(
                obstacle_layer,
                viewport,
                width_px,
                height_px,
                obstacle_context,
                point_display_scale,
            );
            point_tile_count += obstacle_tiles.len();
            for tile in obstacle_tiles {
                let key = tile_key(&tile.layer, tile.z, tile.x, tile.y);
                let Some(payload) = obstacle_tile_cache.get(&key) else {
                    continue;
                };
                for record in &payload.records {
                    vector_budget.scanned_records += 1;
                    if !should_display_record(record) {
                        bump_layer_count(&mut vector_budget.hidden_by_layer, &tile.layer);
                        continue;
                    }
                    let Some(symbol) = point_vector_record_to_symbol_feature(
                        record,
                        obstacle_context.and_then(|context| context.altitude_ft),
                    ) else {
                        bump_layer_count(&mut vector_budget.no_symbol_by_layer, &tile.layer);
                        continue;
                    };
                    vector_budget.displayable_records += 1;
                    let point = nearest_wrapped_screen_point(
                        center_world,
                        scale,
                        width_px,
                        height_px,
                        LatLon {
                            lat: record.lat,
                            lon: record.lon,
                        },
                    );
                    let feature = visible_map_feature_from_symbol(
                        record.id.clone(),
                        symbol,
                        point,
                        VectorIdentLabelStyle::Default,
                    );
                    if let Some(bucket_index) =
                        vector_display_budget_bucket_index(&feature.symbol_kind)
                    {
                        vector_budget_buckets[bucket_index].features.push(feature);
                    }
                }
            }
        }
        obstacle_ms = overlay_elapsed_ms(obstacle_started_at);
    }
    let budget_started_at = core_clock_ms();
    for bucket_index in 0..vector_budget_buckets.len() {
        let bucket_len = vector_budget_buckets[bucket_index].features.len();
        if bucket_len == 0 {
            continue;
        }
        let remaining_budget = VECTOR_DISPLAY_FEATURE_LIMIT.saturating_sub(visible_features.len());
        if bucket_len <= remaining_budget {
            add_layer_count(
                &mut vector_budget.drawn_by_layer,
                vector_budget_buckets[bucket_index].layer,
                bucket_len,
            );
            visible_features.append(&mut vector_budget_buckets[bucket_index].features);
        } else {
            for omitted_bucket in vector_budget_buckets.iter().skip(bucket_index) {
                let omitted_len = omitted_bucket.features.len();
                vector_budget.omitted_after_cap += omitted_len;
                add_layer_count(
                    &mut vector_budget.omitted_by_layer,
                    omitted_bucket.layer,
                    omitted_len,
                );
            }
            break;
        }
    }
    layout_weather_camera_badges(
        &mut visible_features,
        &weather_camera_airport_idents,
        point_display_scale,
    );
    sort_visible_point_features_for_paint(&mut visible_features);
    let budget_ms = overlay_elapsed_ms(budget_started_at);
    let mut data_status_records = if vector_budget.omitted_after_cap > 0 {
        let omitted_summary = layer_counts_summary(&vector_budget.omitted_by_layer);
        vec![DataStatusRecord::new(
            VECTOR_DISPLAY_LIMIT_STATUS_ID,
            "VECTORS",
            Some("LIMIT".to_string()),
            UiStatusSeverity::Warning,
            true,
            format!(
                "display budget {} drew {} of {} displayable point features; omitted lower-priority features: {}",
                VECTOR_DISPLAY_FEATURE_LIMIT,
                visible_features.len(),
                vector_budget.displayable_records,
                omitted_summary
            ),
        )]
    } else {
        Vec::new()
    };

    let airspace_started_at = core_clock_ms();
    let airspace = if display_vectors {
        query_airspace_overlay(
            &projection,
            AirspaceOverlayInput {
                config,
                vector_tile_cache,
                feature_cache: airspace_feature_cache,
            },
        )
    } else {
        AirspaceOverlayProjection {
            needed_tiles: Vec::new(),
            needed_features: Vec::new(),
            paths: Vec::new(),
            labels: Vec::new(),
            data_status_records: Vec::new(),
        }
    };
    let airspace_ms = overlay_elapsed_ms(airspace_started_at);
    let tfr_started_at = core_clock_ms();
    let tfrs = if display_vectors {
        query_tfr_overlay(
            &projection,
            TfrOverlayInput {
                payload: tfr_payload,
                point_features: &visible_features,
                protected_point_features,
                reference_utc: tfr_reference_utc,
            },
        )
    } else {
        TfrOverlayProjection {
            needed_tfrs: false,
            paths: Vec::new(),
            labels: Vec::new(),
        }
    };
    let tfr_ms = overlay_elapsed_ms(tfr_started_at);
    let metar_started_at = core_clock_ms();
    let metars = if display_metars {
        query_metar_overlay(
            &projection,
            MetarOverlayInput {
                tile_zoom: decision.metar_tile_zoom,
                tile_cache: metar_tile_cache,
                metar_payload,
                pirep_payload,
            },
        )
    } else {
        MetarOverlayProjection {
            needed_tiles: Vec::new(),
            needed_metars: false,
            visible_metars: Vec::new(),
            visible_pireps: Vec::new(),
            data_status_records: Vec::new(),
        }
    };
    let metar_ms = overlay_elapsed_ms(metar_started_at);
    let status_started_at = core_clock_ms();
    data_status_records.extend(airspace.data_status_records);
    data_status_records.extend(metars.data_status_records);
    let status_ms = overlay_elapsed_ms(status_started_at);

    let labels_started_at = core_clock_ms();
    let mut airspace_labels = {
        let mut labels = airspace.labels;
        labels.extend(tfrs.labels);
        labels
    };
    suppress_overlapping_vector_labels(
        &mut visible_features,
        &mut airspace_labels,
        protected_point_features,
        point_display_scale,
    );
    let labels_ms = overlay_elapsed_ms(labels_started_at);

    let merge_started_at = core_clock_ms();
    let needed_vector_tiles =
        merge_aggregate_vector_tile_requests(needed_vector_tiles, airspace.needed_tiles);
    let merge_ms = overlay_elapsed_ms(merge_started_at);
    core_perf_debug_log("map.overlay.core", || {
        let airspace_path_points = airspace_display_path_point_count(&airspace.paths);
        let airspace_decoration_points =
            airspace_display_path_decoration_point_count(&airspace.paths);
        let airspace_decoration_segments =
            airspace_display_path_decoration_segment_count(&airspace.paths);
        let tfr_path_points = airspace_display_path_point_count(&tfrs.paths);
        let tfr_decoration_points = airspace_display_path_decoration_point_count(&tfrs.paths);
        let tfr_decoration_segments = airspace_display_path_decoration_segment_count(&tfrs.paths);
        let offline_region_points = offline_region_point_count(&offline_regions);
        let timing = json!({
            "total_ms": overlay_elapsed_ms(total_started_at),
            "offline_ms": offline_ms,
            "point_vector_ms": point_vector_ms,
            "obstacle_ms": obstacle_ms,
            "budget_ms": budget_ms,
            "airspace_ms": airspace_ms,
            "tfr_ms": tfr_ms,
            "metar_ms": metar_ms,
            "status_ms": status_ms,
            "labels_ms": labels_ms,
            "merge_ms": merge_ms,
        });
        let data_status = data_status_records
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>();
        json!({
            "center_lat": viewport.center.lat,
            "center_lon": viewport.center.lon,
            "raw_zoom": viewport.zoom,
            "width_px": width_px,
            "height_px": height_px,
            "display_vectors": display_vectors,
            "display_metars": display_metars,
            "point_display_scale": decision.display_scale,
            "point_effective_zoom": decision.effective_display_zoom,
            "point_tile_zoom": decision.point_tile_zoom,
            "metar_tile_zoom": decision.metar_tile_zoom,
            "point_tile_count": point_tile_count,
            "airspace_ref_zoom": decision.airspace_ref_zoom,
            "airspace_label_zoom": decision.airspace_label_zoom,
            "needed_vector_tiles": tile_counts_by_zoom(&needed_vector_tiles),
            "needed_metar_tiles": tile_counts_by_zoom(&metars.needed_tiles),
            "needed_airspace_features": airspace.needed_features.len(),
            "visible_features": visible_features.len(),
            "vector_budget_limit": VECTOR_DISPLAY_FEATURE_LIMIT,
            "vector_budget_scanned": vector_budget.scanned_records,
            "vector_budget_displayable": vector_budget.displayable_records,
            "vector_budget_omitted_after_cap": vector_budget.omitted_after_cap,
            "vector_budget_drawn_by_layer": vector_budget.drawn_by_layer,
            "vector_budget_omitted_by_layer": vector_budget.omitted_by_layer,
            "vector_budget_hidden_by_layer": vector_budget.hidden_by_layer,
            "vector_budget_no_symbol_by_layer": vector_budget.no_symbol_by_layer,
            "visible_metars": metars.visible_metars.len(),
            "visible_pireps": metars.visible_pireps.len(),
            "airspace_paths": airspace.paths.len(),
            "airspace_labels": airspace_labels.len(),
            "airspace_path_points": airspace_path_points,
            "airspace_decoration_points": airspace_decoration_points,
            "airspace_decoration_segments": airspace_decoration_segments,
            "tfr_paths": tfrs.paths.len(),
            "tfr_path_points": tfr_path_points,
            "tfr_decoration_points": tfr_decoration_points,
            "tfr_decoration_segments": tfr_decoration_segments,
            "offline_regions": offline_regions.len(),
            "offline_region_points": offline_region_points,
            "timing": timing,
            "data_status": data_status,
        })
    });

    MapOverlayQueryResult {
        needed_vector_tiles,
        needed_metar_tiles: metars.needed_tiles,
        needed_airspace_features: airspace.needed_features,
        needed_metars: metars.needed_metars,
        needed_tfrs: tfrs.needed_tfrs,
        data_status_records,
        visible_features,
        flight_plan_features: Vec::new(),
        visible_metars: metars.visible_metars,
        visible_pireps: metars.visible_pireps,
        visible_traffic: Vec::new(),
        traffic_next_refresh_epoch_ms: None,
        airspace_paths: airspace.paths,
        tfr_paths: tfrs.paths,
        airspace_labels,
        offline_regions,
    }
}

pub fn project_nav_symbol_feature(
    id: String,
    symbol: NavSymbolFeature,
    position: LatLon,
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    label_style: VectorIdentLabelStyle,
) -> VisibleMapFeature {
    let center_world = lat_lon_to_world(viewport.center);
    let scale = 2.0_f64.powf(viewport.zoom);
    let point = nearest_wrapped_screen_point(center_world, scale, width_px, height_px, position);
    visible_map_feature_from_symbol(id, symbol, point, label_style)
}

fn nearest_wrapped_screen_point(
    center_world: WorldPoint,
    scale: f64,
    width_px: f64,
    height_px: f64,
    position: LatLon,
) -> WorldPoint {
    [-WORLD_SIZE, 0.0, WORLD_SIZE]
        .into_iter()
        .map(|world_x_offset| {
            world_to_screen_with_x_offset(
                center_world,
                scale,
                width_px,
                height_px,
                position,
                world_x_offset,
            )
        })
        .min_by(|a, b| {
            let adx = a.x - width_px / 2.0;
            let ady = a.y - height_px / 2.0;
            let bdx = b.x - width_px / 2.0;
            let bdy = b.y - height_px / 2.0;
            (adx * adx + ady * ady)
                .partial_cmp(&(bdx * bdx + bdy * bdy))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(WorldPoint {
            x: width_px / 2.0,
            y: height_px / 2.0,
        })
}

fn visible_map_feature_from_symbol(
    id: String,
    symbol: NavSymbolFeature,
    point: WorldPoint,
    label_style: VectorIdentLabelStyle,
) -> VisibleMapFeature {
    VisibleMapFeature {
        id,
        kind: symbol.kind,
        label: symbol.label,
        symbol_kind: symbol.symbol_kind,
        style_class: symbol.style_class,
        obstacle_variant: symbol.obstacle_variant,
        obstacle_tone: symbol.obstacle_tone,
        screen_x: point.x,
        screen_y: point.y,
        towered: symbol.towered,
        fuel_available: symbol.fuel_available,
        has_paved_runway: symbol.has_paved_runway,
        heliport: symbol.heliport,
        has_water_runway: symbol.has_water_runway,
        runway_length_ratio: symbol.runway_length_ratio,
        longest_runway_heading_true_deg: symbol.longest_runway_heading_true_deg,
        label_style,
    }
}

fn remember_weather_camera_airport_ident(
    record: &PointVectorRecord,
    camera_airport_idents: &mut HashMap<String, String>,
) {
    if record.style_class != "weather_camera" {
        return;
    }
    let Some(ident) = record
        .weather_camera
        .as_ref()
        .and_then(|camera| camera.icao.as_deref())
        .map(str::trim)
        .filter(|ident| !ident.is_empty())
    else {
        return;
    };
    camera_airport_idents.insert(record.id.clone(), ident.to_ascii_uppercase());
}

#[derive(Debug)]
struct AirportBadgeAnchor {
    id: String,
    ident: String,
    screen_x: f64,
    screen_y: f64,
    symbol_rect: LabelRect,
}

fn layout_weather_camera_badges(
    features: &mut [VisibleMapFeature],
    camera_airport_idents: &HashMap<String, String>,
    display_scale: f64,
) {
    let airports = features
        .iter()
        .filter(|feature| feature.symbol_kind == "airport")
        .filter_map(|feature| {
            Some(AirportBadgeAnchor {
                id: feature.id.clone(),
                ident: feature
                    .id
                    .strip_prefix("airports:")?
                    .trim()
                    .to_ascii_uppercase(),
                screen_x: feature.screen_x,
                screen_y: feature.screen_y,
                symbol_rect: point_feature_symbol_rect(feature, display_scale)?,
            })
        })
        .collect::<Vec<_>>();
    let mut badge_counts = HashMap::<String, usize>::new();
    let offset =
        WEATHER_CAMERA_AIRPORT_BADGE_OFFSET_LOGICAL_PX * normalized_display_scale(display_scale);
    let slots = [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)];

    for camera in features
        .iter_mut()
        .filter(|feature| feature.symbol_kind == "weather_camera")
    {
        let Some(camera_rect) = point_feature_symbol_rect(camera, display_scale) else {
            continue;
        };
        let preferred_ident = camera_airport_idents.get(&camera.id);
        let Some(airport) = airports
            .iter()
            .filter(|airport| camera_rect.overlaps(airport.symbol_rect))
            .min_by(|left, right| {
                let left_preferred = preferred_ident == Some(&left.ident);
                let right_preferred = preferred_ident == Some(&right.ident);
                right_preferred
                    .cmp(&left_preferred)
                    .then_with(|| {
                        let left_distance = (left.screen_x - camera.screen_x).powi(2)
                            + (left.screen_y - camera.screen_y).powi(2);
                        let right_distance = (right.screen_x - camera.screen_x).powi(2)
                            + (right.screen_y - camera.screen_y).powi(2);
                        left_distance.total_cmp(&right_distance)
                    })
                    .then_with(|| left.id.cmp(&right.id))
            })
        else {
            continue;
        };
        let badge_index = badge_counts.entry(airport.id.clone()).or_default();
        let slot = slots[*badge_index % slots.len()];
        let ring = 1.0 + (*badge_index / slots.len()) as f64;
        *badge_index += 1;
        camera.screen_x = airport.screen_x + slot.0 * offset * ring;
        camera.screen_y = airport.screen_y + slot.1 * offset * ring;
        camera.label.clear();
    }
}

fn point_feature_paint_rank(feature: &VisibleMapFeature) -> u8 {
    match feature.symbol_kind.as_str() {
        "weather_camera" => 0,
        "obstacle" if feature.obstacle_tone.as_deref() == Some("muted") => 5,
        "fix" => 10,
        "nav" => 20,
        "airport" => 30,
        "obstacle" if feature.obstacle_tone.as_deref() == Some("danger") => 50,
        "obstacle" => 40,
        _ => 10,
    }
}

fn sort_visible_point_features_for_paint(features: &mut [VisibleMapFeature]) {
    features.sort_by(|left, right| {
        point_feature_paint_rank(left)
            .cmp(&point_feature_paint_rank(right))
            .then_with(|| left.id.cmp(&right.id))
            .then_with(|| left.screen_x.total_cmp(&right.screen_x))
            .then_with(|| left.screen_y.total_cmp(&right.screen_y))
    });
}

fn project_offline_regions(
    regions: &[OfflineRegionRecord],
    center_world: WorldPoint,
    scale: f64,
    width_px: f64,
    height_px: f64,
) -> Vec<OfflineRegionDisplay> {
    let min_visible_world_x = center_world.x - width_px / 2.0 / scale;
    let max_visible_world_x = center_world.x + width_px / 2.0 / scale;
    let mut displays = Vec::new();
    for region in regions {
        for (polygon_index, polygon) in region
            .polygons
            .iter()
            .enumerate()
            .filter(|(_, polygon)| polygon.len() >= 2)
        {
            let unwrapped_polygon = unwrap_world_path_near_center(polygon, center_world);
            let min_polygon_x = unwrapped_polygon
                .iter()
                .map(|world| world.x)
                .fold(f64::INFINITY, f64::min);
            let max_polygon_x = unwrapped_polygon
                .iter()
                .map(|world| world.x)
                .fold(f64::NEG_INFINITY, f64::max);
            if !min_polygon_x.is_finite() || !max_polygon_x.is_finite() {
                continue;
            }
            let polygon_average_x = unwrapped_polygon.iter().map(|world| world.x).sum::<f64>()
                / unwrapped_polygon.len() as f64;
            let copy_start = ((min_visible_world_x - max_polygon_x) / WORLD_SIZE).floor() as i32;
            let copy_end = ((max_visible_world_x - min_polygon_x) / WORLD_SIZE).ceil() as i32;
            for copy in copy_start..=copy_end {
                let offset = copy as f64 * WORLD_SIZE;
                let points = unwrapped_polygon
                    .iter()
                    .map(|world| WorldPoint {
                        x: world.x + offset,
                        y: world.y,
                    })
                    .map(|world| {
                        let screen = projected_world_to_screen(
                            center_world,
                            scale,
                            width_px,
                            height_px,
                            world,
                        );
                        AirspaceScreenPoint {
                            x: screen.x,
                            y: screen.y,
                        }
                    })
                    .collect();
                let mut label_world = lat_lon_to_world(region.label_position);
                label_world.x +=
                    ((polygon_average_x - label_world.x) / WORLD_SIZE).round() * WORLD_SIZE;
                label_world.x += offset;
                let label = projected_world_to_screen(
                    center_world,
                    scale,
                    width_px,
                    height_px,
                    label_world,
                );
                displays.push(OfflineRegionDisplay {
                    id: format!("{}:{polygon_index}:{copy}", region.id),
                    kind: region.kind.clone(),
                    region_id: region.region_id.clone(),
                    label: region.label.clone(),
                    color_key: region.color_key.clone(),
                    summary: region.summary.clone(),
                    points,
                    label_x: label.x,
                    label_y: label.y,
                });
            }
        }
    }
    displays
}

fn unwrap_world_x_near_center(mut world: WorldPoint, center_world: WorldPoint) -> WorldPoint {
    world.x += ((center_world.x - world.x) / WORLD_SIZE).round() * WORLD_SIZE;
    world
}

fn unwrap_world_path_near_center(points: &[LatLon], center_world: WorldPoint) -> Vec<WorldPoint> {
    let mut best = Vec::new();
    let mut best_span = f64::INFINITY;
    for start in 0..points.len() {
        let mut rotated = Vec::with_capacity(points.len());
        rotated.extend(points[start..].iter().copied());
        rotated.extend(points[..start].iter().copied());
        let unwrapped = unwrap_world_path_linear(&rotated);
        let min_x = unwrapped
            .iter()
            .map(|world| world.x)
            .fold(f64::INFINITY, f64::min);
        let max_x = unwrapped
            .iter()
            .map(|world| world.x)
            .fold(f64::NEG_INFINITY, f64::max);
        let span = max_x - min_x;
        if span < best_span {
            best_span = span;
            best = unwrapped;
        }
    }
    if !best.is_empty() {
        let average_x = best.iter().map(|world| world.x).sum::<f64>() / best.len() as f64;
        let shift = ((center_world.x - average_x) / WORLD_SIZE).round() * WORLD_SIZE;
        for world in &mut best {
            world.x += shift;
        }
    }
    best
}

fn unwrap_world_path_linear(points: &[LatLon]) -> Vec<WorldPoint> {
    let mut unwrapped = Vec::with_capacity(points.len());
    let mut previous_x: Option<f64> = None;
    for point in points {
        let mut world = lat_lon_to_world(*point);
        if let Some(previous_x) = previous_x {
            world.x += ((previous_x - world.x) / WORLD_SIZE).round() * WORLD_SIZE;
        }
        previous_x = Some(world.x);
        unwrapped.push(world);
    }
    unwrapped
}

struct MetarOverlayProjection {
    needed_tiles: Vec<VectorTileRequest>,
    needed_metars: bool,
    visible_metars: Vec<VisibleMetarFeature>,
    visible_pireps: Vec<VisiblePirepFeature>,
    data_status_records: Vec<DataStatusRecord>,
}

#[derive(Debug, Clone, Copy)]
enum WorldXProjection {
    NearestWrappedCopy,
    DisplayCopyOffset(f64),
}

struct MetarOverlayInput<'a> {
    tile_zoom: Option<u32>,
    tile_cache: &'a HashMap<String, MetarTilePayload>,
    metar_payload: Option<&'a MetarProductPayload>,
    pirep_payload: Option<&'a PirepProductPayload>,
}

fn query_metar_overlay(
    projection: &MapProjectionContext<'_>,
    input: MetarOverlayInput<'_>,
) -> MetarOverlayProjection {
    let metrics = projection.metrics;
    let viewport = &metrics.viewport;
    let width_px = metrics.width_px;
    let height_px = metrics.height_px;
    let center_world = projection.center_world;
    let scale = projection.scale;
    let MetarOverlayInput {
        tile_zoom: metar_tile_zoom,
        tile_cache: metar_tile_cache,
        metar_payload,
        pirep_payload,
    } = input;
    let Some(metar_zoom) = metar_tile_zoom else {
        return MetarOverlayProjection {
            needed_tiles: Vec::new(),
            needed_metars: false,
            visible_metars: Vec::new(),
            visible_pireps: Vec::new(),
            data_status_records: Vec::new(),
        };
    };
    let needed_metars = metar_payload.is_none();
    let mut needed_tiles = Vec::new();
    let mut visible_metars = Vec::new();
    let mut visible_pireps = Vec::new();
    let display_pireps = full_weather_detail_visible(*metrics);
    let mut limit_hit = false;
    let mut needed_seen = BTreeSet::new();
    for tile in
        visible_layer_display_tile_window("metars", metar_zoom, viewport, width_px, height_px)
    {
        let key = tile_key(
            &tile.request.layer,
            tile.request.z,
            tile.request.x,
            tile.request.y,
        );
        let Some(tile_payload) = metar_tile_cache.get(&key) else {
            if needed_seen.insert(key) {
                needed_tiles.push(tile.request);
            }
            continue;
        };
        for record_ref in &tile_payload.records {
            if visible_metars.len() + visible_pireps.len() >= WEATHER_DISPLAY_FEATURE_LIMIT {
                limit_hit = true;
                break;
            }
            if record_ref.kind == "metar" {
                let Some(metars) = metar_payload else {
                    continue;
                };
                let Some(record) = metars.metars_by_station.get(&record_ref.id) else {
                    continue;
                };
                let feature = visible_metar_feature(
                    record,
                    center_world,
                    scale,
                    width_px,
                    height_px,
                    WorldXProjection::DisplayCopyOffset(tile.world_x_offset),
                );
                if weather_feature_is_on_screen(
                    feature.screen_x,
                    feature.screen_y,
                    width_px,
                    height_px,
                ) {
                    visible_metars.push(feature);
                }
            } else if record_ref.kind == "pirep" && display_pireps {
                let Some(pireps) = pirep_payload else {
                    continue;
                };
                let Some(record) = pireps.pireps_by_id.get(&record_ref.id) else {
                    continue;
                };
                let feature = visible_pirep_feature(
                    record,
                    center_world,
                    scale,
                    width_px,
                    height_px,
                    WorldXProjection::DisplayCopyOffset(tile.world_x_offset),
                );
                if weather_feature_is_on_screen(
                    feature.screen_x,
                    feature.screen_y,
                    width_px,
                    height_px,
                ) {
                    visible_pireps.push(feature);
                }
            }
        }
        if limit_hit {
            break;
        }
    }
    let data_status_records = if limit_hit {
        vec![DataStatusRecord::new(
            WEATHER_DISPLAY_LIMIT_STATUS_ID,
            "WX",
            Some("LIMIT".to_string()),
            UiStatusSeverity::Warning,
            true,
            format!(
                "Display budget reached; only the first {} visible weather observations were drawn.",
                WEATHER_DISPLAY_FEATURE_LIMIT
            ),
        )]
    } else {
        Vec::new()
    };
    MetarOverlayProjection {
        needed_tiles,
        needed_metars,
        visible_metars,
        visible_pireps,
        data_status_records,
    }
}

fn weather_feature_is_on_screen(
    screen_x: f64,
    screen_y: f64,
    width_px: f64,
    height_px: f64,
) -> bool {
    screen_x >= -32.0
        && screen_x <= width_px + 32.0
        && screen_y >= -32.0
        && screen_y <= height_px + 32.0
}

fn normalized_metar_flight_category(record: &MetarRecord) -> String {
    match record.flight_category.as_deref().map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case("VFR") => "vfr".to_string(),
        Some(value) if value.eq_ignore_ascii_case("MVFR") => "mvfr".to_string(),
        Some(value) if value.eq_ignore_ascii_case("IFR") => "ifr".to_string(),
        Some(value) if value.eq_ignore_ascii_case("LIFR") => "lifr".to_string(),
        _ => "missing".to_string(),
    }
}

fn normalized_metar_ceiling_amount(record: &MetarRecord) -> String {
    let amount = record
        .clouds
        .as_ref()
        .and_then(|clouds| clouds.symbol.as_deref())
        .map(str::trim);
    match amount {
        Some(value) if value.eq_ignore_ascii_case("SKC") || value.eq_ignore_ascii_case("CLR") => {
            "skc".to_string()
        }
        Some(value) if value.eq_ignore_ascii_case("FEW") => "few".to_string(),
        Some(value) if value.eq_ignore_ascii_case("SCT") => "sct".to_string(),
        Some(value) if value.eq_ignore_ascii_case("BKN") => "bkn".to_string(),
        Some(value) if value.eq_ignore_ascii_case("OVC") || value.eq_ignore_ascii_case("VV") => {
            "ovc".to_string()
        }
        _ => "missing".to_string(),
    }
}

fn visible_metar_feature(
    record: &MetarRecord,
    center_world: WorldPoint,
    scale: f64,
    width_px: f64,
    height_px: f64,
    projection: WorldXProjection,
) -> VisibleMetarFeature {
    let point = world_to_screen_projected(
        center_world,
        scale,
        width_px,
        height_px,
        LatLon {
            lat: record.latitude,
            lon: record.longitude,
        },
        projection,
    );
    VisibleMetarFeature {
        station_id: record.station_id.clone(),
        screen_x: point.x,
        screen_y: point.y,
        flight_category: normalized_metar_flight_category(record),
        ceiling_amount: normalized_metar_ceiling_amount(record),
    }
}

fn visible_pirep_feature(
    record: &PirepRecord,
    center_world: WorldPoint,
    scale: f64,
    width_px: f64,
    height_px: f64,
    projection: WorldXProjection,
) -> VisiblePirepFeature {
    let point = world_to_screen_projected(
        center_world,
        scale,
        width_px,
        height_px,
        LatLon {
            lat: record.latitude,
            lon: record.longitude,
        },
        projection,
    );
    VisiblePirepFeature {
        id: record.id.clone(),
        screen_x: point.x,
        screen_y: point.y,
        symbol: normalized_pirep_symbol(&record.symbol),
        icing: normalized_pirep_hazard(&record.icing),
        turbulence: normalized_pirep_hazard(&record.turbulence),
    }
}

fn normalized_pirep_symbol(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "light-icing" => "light-icing".to_string(),
        "moderate-icing" => "moderate-icing".to_string(),
        "severe-icing" => "severe-icing".to_string(),
        "light-turbulence" => "light-turbulence".to_string(),
        "moderate-turbulence" => "moderate-turbulence".to_string(),
        "severe-turbulence" => "severe-turbulence".to_string(),
        _ => "generic".to_string(),
    }
}

fn normalized_pirep_hazard(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "light" => "light".to_string(),
        "moderate" => "moderate".to_string(),
        "severe" => "severe".to_string(),
        "unknown" => "unknown".to_string(),
        _ => "none".to_string(),
    }
}

pub struct MapSelectionQuery<'a> {
    pub config: &'a MapOverlayConfig,
    pub plan: Option<&'a FlightPlan>,
    pub click: LatLon,
    pub vector_tile_cache: &'a HashMap<String, VectorAggregateTilePayload>,
    pub metar_tile_cache: &'a HashMap<String, MetarTilePayload>,
    pub metar_payload: Option<&'a MetarProductPayload>,
    pub pirep_payload: Option<&'a PirepProductPayload>,
    pub taf_payload: Option<&'a TafProductPayload>,
    pub notam_payload: Option<&'a NotamDisplayIndex>,
    pub weather_station_airport_aliases: &'a WeatherStationAirportAliases,
    pub offline_region_records: &'a [OfflineRegionRecord],
    pub airspace_feature_cache: &'a HashMap<String, AirspaceFeaturePayload>,
    pub tfr_payload: Option<&'a TfrProductPayload>,
    pub supplemental_nav_ref_points: &'a [NavRefSelectionPoint],
    pub airport_plate_availability: &'a mut dyn FnMut(&str) -> AirportPlateAvailability,
    pub weather_age_reference_utc: Option<DateTime<Utc>>,
    pub local_time_zone: Tz,
    pub time_display_mode: crate::TimeDisplayMode,
}

impl<'a> MapSelectionQuery<'a> {
    pub fn new(
        config: &'a MapOverlayConfig,
        click: LatLon,
        vector_tile_cache: &'a HashMap<String, VectorAggregateTilePayload>,
        metar_tile_cache: &'a HashMap<String, MetarTilePayload>,
        airspace_feature_cache: &'a HashMap<String, AirspaceFeaturePayload>,
        weather_station_airport_aliases: &'a WeatherStationAirportAliases,
        airport_plate_availability: &'a mut dyn FnMut(&str) -> AirportPlateAvailability,
    ) -> Self {
        Self {
            config,
            plan: None,
            click,
            vector_tile_cache,
            metar_tile_cache,
            metar_payload: None,
            pirep_payload: None,
            taf_payload: None,
            notam_payload: None,
            weather_station_airport_aliases,
            offline_region_records: &[],
            airspace_feature_cache,
            tfr_payload: None,
            supplemental_nav_ref_points: &[],
            airport_plate_availability,
            weather_age_reference_utc: None,
            local_time_zone: chrono_tz::UTC,
            time_display_mode: crate::TimeDisplayMode::Utc,
        }
    }
}

#[derive(Clone, Copy)]
struct MapSelectionProjectionContext<'a> {
    map: MapProjectionContext<'a>,
    click_screen: WorldPoint,
    hit_radius_px: f64,
    metar_tile_zoom: Option<u32>,
}

pub fn query_map_selection(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    query: MapSelectionQuery<'_>,
) -> MapSelectionQueryResult {
    query_map_selection_with_point_display_scale(viewport, width_px, height_px, 1.0, query)
}

pub fn query_map_selection_with_point_display_scale(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    point_display_scale: f64,
    query: MapSelectionQuery<'_>,
) -> MapSelectionQueryResult {
    let metrics = MapSurfaceMetrics::new(*viewport, width_px, height_px, point_display_scale);
    query_map_selection_for_surface(&metrics, query)
}

pub fn query_map_selection_for_surface(
    metrics: &MapSurfaceMetrics,
    query: MapSelectionQuery<'_>,
) -> MapSelectionQueryResult {
    query_map_selection_for_surface_in_time_zone(metrics, query)
}

pub fn query_map_selection_for_surface_in_time_zone(
    metrics: &MapSurfaceMetrics,
    query: MapSelectionQuery<'_>,
) -> MapSelectionQueryResult {
    let MapSelectionQuery {
        config,
        plan,
        click,
        vector_tile_cache,
        metar_tile_cache,
        metar_payload,
        pirep_payload,
        taf_payload,
        notam_payload,
        weather_station_airport_aliases,
        offline_region_records,
        airspace_feature_cache,
        tfr_payload,
        supplemental_nav_ref_points,
        airport_plate_availability,
        weather_age_reference_utc,
        local_time_zone,
        time_display_mode,
    } = query;
    let viewport = &metrics.viewport;
    let width_px = metrics.width_px;
    let height_px = metrics.height_px;
    let point_display_scale = metrics.display_scale;
    let metar_tile_zoom = overlay_surface_decision(*metrics, config).metar_tile_zoom;
    let hit_radius_px = metrics.inspector_hit_radius_px();
    let map_projection = MapProjectionContext::new(metrics);
    let center_world = map_projection.center_world;
    let scale = map_projection.scale;
    let click_screen = world_to_screen_projected(
        center_world,
        scale,
        width_px,
        height_px,
        click,
        WorldXProjection::DisplayCopyOffset(0.0),
    );
    let selection_projection = MapSelectionProjectionContext {
        map: map_projection,
        click_screen,
        hit_radius_px,
        metar_tile_zoom,
    };
    let mut airports = Vec::new();
    let mut navaids = Vec::new();
    let mut weather = Vec::new();
    let mut offline_regions = BTreeMap::<String, Vec<&OfflineRegionRecord>>::new();
    let mut airspaces = Vec::new();
    let mut matched_nav_refs = BTreeSet::new();
    let mut vector_layer_airport_ids = BTreeSet::new();
    let mut nearest_weather_camera = None::<(f64, String)>;
    let mut projected_records = Vec::<(&PointVectorRecord, bool, bool, NavSymbolFeature)>::new();
    let mut projected_features = Vec::<VisibleMapFeature>::new();
    let mut weather_camera_airport_idents = HashMap::new();

    for tile in visible_point_display_tile_window(
        config,
        viewport,
        width_px,
        height_px,
        point_display_scale,
    ) {
        let Some(payload) = vector_tile_cache.get(&aggregate_vector_tile_cache_key(
            tile.request.z,
            tile.request.x,
            tile.request.y,
        )) else {
            continue;
        };
        for record in vector_tile_point_records(payload, &tile.request.layer) {
            let is_airport = selection_record_is_airport(record);
            let is_weather_camera = record.style_class == "weather_camera";
            let is_displayed = should_display_record(record);
            if !is_airport && !is_displayed {
                continue;
            }
            let point = world_to_screen_with_x_offset(
                center_world,
                scale,
                width_px,
                height_px,
                LatLon {
                    lat: record.lat,
                    lon: record.lon,
                },
                tile.world_x_offset,
            );
            if !is_airport && !is_weather_camera {
                let distance_px = ((point.x - click_screen.x).powi(2)
                    + (point.y - click_screen.y).powi(2))
                .sqrt();
                if distance_px > hit_radius_px {
                    continue;
                }
                let Some(symbol) = selection_symbol_for_point(record, false) else {
                    continue;
                };
                if record.style_class == "fix" || record.style_class == "nav" {
                    let item = selection_item_for_point(
                        record,
                        &symbol,
                        plan,
                        AirportPlateAvailability::default(),
                        None,
                    );
                    if let Some(key) = nav_ref_match_key(item.nav_ref.as_ref()) {
                        matched_nav_refs.insert(key);
                    }
                    navaids.push(MapSelectionPointMatch { item, distance_px });
                }
                continue;
            }
            let Some(symbol) = selection_symbol_for_point(record, is_airport) else {
                continue;
            };
            remember_weather_camera_airport_ident(record, &mut weather_camera_airport_idents);
            projected_features.push(visible_map_feature_from_symbol(
                record.id.clone(),
                symbol.clone(),
                point,
                VectorIdentLabelStyle::Default,
            ));
            projected_records.push((record, is_airport, is_displayed, symbol));
        }
    }

    let displayed_indices = projected_records
        .iter()
        .enumerate()
        .filter_map(|(index, (_, _, is_displayed, _))| is_displayed.then_some(index))
        .collect::<Vec<_>>();
    let mut displayed_features = displayed_indices
        .iter()
        .map(|index| projected_features[*index].clone())
        .collect::<Vec<_>>();
    layout_weather_camera_badges(
        &mut displayed_features,
        &weather_camera_airport_idents,
        point_display_scale,
    );
    for (index, displayed_feature) in displayed_indices.into_iter().zip(displayed_features) {
        projected_features[index] = displayed_feature;
    }

    for ((record, is_airport, is_displayed, symbol), feature) in
        projected_records.into_iter().zip(projected_features)
    {
        let distance_px = ((feature.screen_x - click_screen.x).powi(2)
            + (feature.screen_y - click_screen.y).powi(2))
        .sqrt();
        if distance_px > hit_radius_px {
            continue;
        }
        if is_airport {
            if is_displayed {
                vector_layer_airport_ids.insert(record.id.clone());
            }
            let availability = selection_nav_ref(record, true)
                .and_then(|nav_ref| match nav_ref {
                    NavRef::Airport(airport_id) => Some(airport_plate_availability(&airport_id)),
                    _ => None,
                })
                .unwrap_or_default();
            let airport_id = selection_nav_ref(record, true).and_then(|nav_ref| match nav_ref {
                NavRef::Airport(airport_id) => Some(airport_id),
                _ => None,
            });
            let weather_detail = airport_id.as_deref().and_then(|airport_id| {
                weather_detail_for_airport(
                    airport_id,
                    weather_station_airport_aliases,
                    metar_payload,
                    taf_payload,
                    notam_payload,
                    weather_age_reference_utc,
                )
            });
            let item =
                selection_item_for_point(record, &symbol, plan, availability, weather_detail);
            if let Some(key) = nav_ref_match_key(item.nav_ref.as_ref()) {
                matched_nav_refs.insert(key);
            }
            airports.push(MapSelectionPointMatch { item, distance_px });
        } else if record.style_class == "weather_camera" {
            if let Some(item) = selection_item_for_weather_camera(record, symbol) {
                let replace_nearest = nearest_weather_camera
                    .as_ref()
                    .is_none_or(|(nearest_distance, _)| distance_px < *nearest_distance);
                if replace_nearest {
                    nearest_weather_camera = Some((distance_px, item.id.clone()));
                }
                weather.push(MapSelectionPointMatch { item, distance_px });
            }
        }
    }

    for matched in query_supplemental_nav_ref_selection_matches(
        &selection_projection,
        SupplementalNavRefSelectionInput {
            points: supplemental_nav_ref_points,
            matched_nav_refs: &matched_nav_refs,
            item_data: SelectionItemData {
                plan,
                metar_payload,
                taf_payload,
                notam_payload,
                weather_station_airport_aliases,
                weather_age_reference_utc,
            },
            airport_plate_availability,
        },
    ) {
        if matches!(matched.item.nav_ref, Some(NavRef::Airport(_))) {
            airports.push(matched);
        } else {
            navaids.push(matched);
        }
    }

    for feature_id in selectable_airspace_feature_ids_for_viewport(
        viewport,
        width_px,
        height_px,
        config,
        vector_tile_cache,
        point_display_scale,
    ) {
        let Some(feature) = airspace_feature_cache.get(&feature_id) else {
            continue;
        };
        if selectable_airspace_feature(feature) && airspace_feature_contains(feature, click) {
            airspaces.push(selection_item_for_airspace(feature));
        }
    }
    if let Some(tfr_payload) = tfr_payload {
        for area in &tfr_payload.areas {
            if tfr_area_contains(area, click) {
                airspaces.push(selection_item_for_tfr(
                    area,
                    weather_age_reference_utc,
                    local_time_zone,
                    time_display_mode,
                ));
            }
        }
    }

    if let Some(metar_payload) = metar_payload {
        weather.extend(query_metar_selection_matches(
            &selection_projection,
            MetarSelectionInput {
                tile_cache: metar_tile_cache,
                metar_payload,
                taf_payload,
                notam_payload,
                weather_station_airport_aliases,
                weather_age_reference_utc,
            },
        ));
    }
    if let Some(pirep_payload) = pirep_payload {
        weather.extend(query_pirep_selection_matches(
            &selection_projection,
            metar_tile_cache,
            pirep_payload,
        ));
    }

    for region in offline_region_records {
        if offline_region_contains(region, click) {
            offline_regions
                .entry(region.region_id.to_ascii_lowercase())
                .or_default()
                .push(region);
        }
    }

    airports.sort_by(|left, right| {
        vector_layer_airport_ids
            .contains(&right.item.id)
            .cmp(&vector_layer_airport_ids.contains(&left.item.id))
            .then_with(|| compare_map_selection_point_matches(left, right))
    });
    navaids.sort_by(compare_map_selection_point_matches);
    weather.sort_by(compare_map_selection_point_matches);
    let offline_region_items = offline_regions
        .values()
        .map(|regions| selection_item_for_offline_region_group(regions))
        .collect::<Vec<_>>();
    airspaces.sort_by(|left, right| {
        left.label
            .cmp(&right.label)
            .then_with(|| left.id.cmp(&right.id))
    });
    let nearest_airport = airports
        .first()
        .map(|matched| (matched.distance_px, matched.item.id.clone()));
    let nearest_point_id = match (nearest_airport, nearest_weather_camera) {
        (Some(airport), Some(camera)) if camera.0 < airport.0 => Some(camera.1),
        (Some(airport), _) => Some(airport.1),
        (None, Some(camera)) => Some(camera.1),
        (None, None) => None,
    };
    let spot = spot_selection_item(click, plan);
    let initial_selected_item_id = nearest_point_id.or_else(|| Some(spot.id.clone()));
    navaids.push(MapSelectionPointMatch {
        item: spot,
        distance_px: f64::INFINITY,
    });

    MapSelectionQueryResult {
        click_lat: click.lat,
        click_lon: click.lon,
        initial_selected_item_id,
        categories: vec![
            MapSelectionCategory {
                id: "airport".to_string(),
                label: "Airport".to_string(),
                items: airports.into_iter().map(|matched| matched.item).collect(),
            },
            MapSelectionCategory {
                id: "navaid".to_string(),
                label: "Navaid".to_string(),
                items: navaids.into_iter().map(|matched| matched.item).collect(),
            },
            MapSelectionCategory {
                id: "airspace".to_string(),
                label: "Airspace".to_string(),
                items: airspaces,
            },
            MapSelectionCategory {
                id: "weather".to_string(),
                label: "Weather".to_string(),
                items: weather.into_iter().map(|matched| matched.item).collect(),
            },
            MapSelectionCategory {
                id: "offline".to_string(),
                label: "Offline".to_string(),
                items: offline_region_items,
            },
        ],
    }
}

struct MetarSelectionInput<'a> {
    tile_cache: &'a HashMap<String, MetarTilePayload>,
    metar_payload: &'a MetarProductPayload,
    taf_payload: Option<&'a TafProductPayload>,
    notam_payload: Option<&'a NotamDisplayIndex>,
    weather_station_airport_aliases: &'a WeatherStationAirportAliases,
    weather_age_reference_utc: Option<DateTime<Utc>>,
}

fn query_metar_selection_matches(
    projection: &MapSelectionProjectionContext<'_>,
    input: MetarSelectionInput<'_>,
) -> Vec<MapSelectionPointMatch> {
    let metrics = projection.map.metrics;
    let viewport = &metrics.viewport;
    let width_px = metrics.width_px;
    let height_px = metrics.height_px;
    let center_world = projection.map.center_world;
    let scale = projection.map.scale;
    let click_screen = projection.click_screen;
    let hit_radius_px = projection.hit_radius_px;
    let metar_tile_zoom = projection.metar_tile_zoom;
    let MetarSelectionInput {
        tile_cache: metar_tile_cache,
        metar_payload,
        taf_payload,
        notam_payload,
        weather_station_airport_aliases,
        weather_age_reference_utc,
    } = input;
    let Some(metar_zoom) = metar_tile_zoom else {
        return Vec::new();
    };
    let mut matches = Vec::new();
    for tile in
        visible_layer_display_tile_window("metars", metar_zoom, viewport, width_px, height_px)
    {
        let Some(tile_payload) = metar_tile_cache.get(&tile_key(
            &tile.request.layer,
            tile.request.z,
            tile.request.x,
            tile.request.y,
        )) else {
            continue;
        };
        for record_ref in &tile_payload.records {
            if record_ref.kind != "metar" {
                continue;
            }
            let Some(record) = metar_payload.metars_by_station.get(&record_ref.id) else {
                continue;
            };
            let feature = visible_metar_feature(
                record,
                center_world,
                scale,
                width_px,
                height_px,
                WorldXProjection::DisplayCopyOffset(tile.world_x_offset),
            );
            let distance_px = ((feature.screen_x - click_screen.x).powi(2)
                + (feature.screen_y - click_screen.y).powi(2))
            .sqrt();
            if distance_px <= hit_radius_px {
                matches.push(MapSelectionPointMatch {
                    item: selection_item_for_metar(
                        record,
                        taf_payload.and_then(|payload| {
                            payload.tafs_by_station.get(record.station_id.trim())
                        }),
                        feature,
                        notam_payload,
                        weather_station_airport_aliases,
                        weather_age_reference_utc,
                    ),
                    distance_px,
                });
            }
        }
    }
    matches
}

fn query_pirep_selection_matches(
    projection: &MapSelectionProjectionContext<'_>,
    metar_tile_cache: &HashMap<String, MetarTilePayload>,
    pirep_payload: &PirepProductPayload,
) -> Vec<MapSelectionPointMatch> {
    let metrics = projection.map.metrics;
    if !full_weather_detail_visible(*metrics) {
        return Vec::new();
    }
    let viewport = &metrics.viewport;
    let width_px = metrics.width_px;
    let height_px = metrics.height_px;
    let center_world = projection.map.center_world;
    let scale = projection.map.scale;
    let click_screen = projection.click_screen;
    let hit_radius_px = projection.hit_radius_px;
    let metar_tile_zoom = projection.metar_tile_zoom;
    let Some(metar_zoom) = metar_tile_zoom else {
        return Vec::new();
    };
    let mut matches = Vec::new();
    for tile in
        visible_layer_display_tile_window("metars", metar_zoom, viewport, width_px, height_px)
    {
        let Some(tile_payload) = metar_tile_cache.get(&tile_key(
            &tile.request.layer,
            tile.request.z,
            tile.request.x,
            tile.request.y,
        )) else {
            continue;
        };
        for record_ref in &tile_payload.records {
            if record_ref.kind != "pirep" {
                continue;
            }
            let Some(record) = pirep_payload.pireps_by_id.get(&record_ref.id) else {
                continue;
            };
            let feature = visible_pirep_feature(
                record,
                center_world,
                scale,
                width_px,
                height_px,
                WorldXProjection::DisplayCopyOffset(tile.world_x_offset),
            );
            let distance_px = ((feature.screen_x - click_screen.x).powi(2)
                + (feature.screen_y - click_screen.y).powi(2))
            .sqrt();
            if distance_px <= hit_radius_px {
                matches.push(MapSelectionPointMatch {
                    item: selection_item_for_pirep(record, feature),
                    distance_px,
                });
            }
        }
    }
    matches
}

fn selection_item_for_point(
    record: &PointVectorRecord,
    symbol: &NavSymbolFeature,
    plan: Option<&FlightPlan>,
    airport_plate_availability: AirportPlateAvailability,
    weather_detail: Option<WeatherDetailUiView>,
) -> MapSelectionItem {
    let is_airport = record.style_class == "airport"
        || record.kind.eq_ignore_ascii_case("airport")
        || record.id.starts_with("airports:");
    let label = if is_airport {
        airport_ident_label(record).unwrap_or_else(|| display_label(record))
    } else if symbol.label.trim().is_empty() {
        display_label(record)
    } else {
        symbol.label.clone()
    };
    let mut symbol_feature = symbol.clone();
    if is_airport {
        symbol_feature.label = label.clone();
    }
    let nav_ref = selection_nav_ref(record, is_airport);
    let remove_row_action = nav_ref.as_ref().and_then(|nav_ref| {
        selection_flight_plan_row_action(plan, nav_ref, FlightPlanRowActionId::Remove)
    });
    let direct_to_row_action = nav_ref.as_ref().and_then(|nav_ref| {
        selection_flight_plan_row_action(plan, nav_ref, FlightPlanRowActionId::DirectTo)
    });
    let insert_action = match &nav_ref {
        Some(nav_ref) if remove_row_action.is_some() => {
            row_action("remove_from_flight_plan", "Remove", remove_row_action)
        }
        Some(nav_ref) if selection_plan_top_level_waypoint_count(plan, nav_ref) > 1 => {
            disabled_action_with_reason(
                "remove_from_flight_plan",
                "Remove",
                "This waypoint appears more than once in the flight plan.",
            )
        }
        Some(nav_ref) if !selection_plan_contains_nav_ref(plan, nav_ref) => {
            insert_best_position_action(plan, nav_ref)
        }
        Some(_) => disabled_action_with_reason(
            "insert",
            "Insert",
            "Edit grouped routes from the Flight Plan page.",
        ),
        None => disabled_action_with_reason(
            "insert",
            "Insert",
            "This item cannot be inserted into the flight plan.",
        ),
    };
    let airport_id = match &nav_ref {
        Some(NavRef::Airport(airport_id)) => Some(airport_id.as_str()),
        _ => None,
    };
    let mut actions = if let Some(airport_id) = airport_id {
        vec![
            direct_to_action(plan, nav_ref.as_ref(), direct_to_row_action),
            insert_action,
            plate_target_action(
                "plates",
                "Plates",
                airport_id,
                "Folder",
                airport_plate_availability.plates,
            ),
            plate_target_action(
                "csup",
                "CSUP",
                airport_id,
                "CSup",
                airport_plate_availability.csup,
            ),
            weather_action(weather_detail.clone()),
            airport_info_action(airport_id),
        ]
    } else {
        vec![
            direct_to_action(plan, nav_ref.as_ref(), direct_to_row_action),
            insert_action,
        ]
    };
    MapSelectionItem {
        id: record.id.clone(),
        label,
        sublabel: record.kind.trim().to_ascii_uppercase(),
        description: selection_item_description(record, is_airport),
        distance: None,
        secondary_description: record.location_label.clone(),
        position: Some(LatLon {
            lat: record.lat,
            lon: record.lon,
        }),
        elevation_msl_ft: record.elevation_msl_ft.or_else(|| {
            record
                .obstacle
                .as_ref()
                .map(|obstacle| obstacle.elevation_msl_ft)
        }),
        detail_text: None,
        highlight: MapSelectionHighlight::FeatureRef {
            id: record.id.clone(),
        },
        nav_ref,
        symbol_feature: Some(symbol_feature),
        metar_feature: None,
        weather_detail,
        automatic_action_uid: None,
        pirep_feature: None,
        airspace_icon: None,
        actions: {
            actions.shrink_to_fit();
            actions
        },
    }
}

fn selection_item_for_weather_camera(
    record: &PointVectorRecord,
    symbol_feature: NavSymbolFeature,
) -> Option<MapSelectionItem> {
    let camera = record.weather_camera.as_ref()?;
    let description = camera
        .operated_by
        .as_deref()
        .map(|operator| format!("Weather camera · {operator}"))
        .or_else(|| Some("Weather camera".to_string()));
    Some(MapSelectionItem {
        id: record.id.clone(),
        label: camera.site_name.clone(),
        sublabel: camera
            .site_identifier
            .clone()
            .or_else(|| camera.icao.clone())
            .unwrap_or_else(|| "CAM".to_string()),
        description,
        distance: None,
        secondary_description: record.location_label.clone(),
        position: Some(LatLon {
            lat: record.lat,
            lon: record.lon,
        }),
        elevation_msl_ft: record.elevation_msl_ft,
        detail_text: None,
        highlight: MapSelectionHighlight::FeatureRef {
            id: record.id.clone(),
        },
        nav_ref: None,
        symbol_feature: Some(symbol_feature),
        metar_feature: None,
        weather_detail: None,
        automatic_action_uid: None,
        pirep_feature: None,
        airspace_icon: None,
        actions: vec![external_url_action(
            "open_weather_camera",
            "Open Camera",
            &camera.page_url,
        )],
    })
}

struct SelectionItemData<'a> {
    plan: Option<&'a FlightPlan>,
    metar_payload: Option<&'a MetarProductPayload>,
    taf_payload: Option<&'a TafProductPayload>,
    notam_payload: Option<&'a NotamDisplayIndex>,
    weather_station_airport_aliases: &'a WeatherStationAirportAliases,
    weather_age_reference_utc: Option<DateTime<Utc>>,
}

struct SupplementalNavRefSelectionInput<'a> {
    points: &'a [NavRefSelectionPoint],
    matched_nav_refs: &'a BTreeSet<String>,
    item_data: SelectionItemData<'a>,
    airport_plate_availability: &'a mut dyn FnMut(&str) -> AirportPlateAvailability,
}

fn query_supplemental_nav_ref_selection_matches(
    projection: &MapSelectionProjectionContext<'_>,
    input: SupplementalNavRefSelectionInput<'_>,
) -> Vec<MapSelectionPointMatch> {
    let metrics = projection.map.metrics;
    let width_px = metrics.width_px;
    let height_px = metrics.height_px;
    let center_world = projection.map.center_world;
    let scale = projection.map.scale;
    let click_screen = projection.click_screen;
    let hit_radius_px = projection.hit_radius_px;
    let SupplementalNavRefSelectionInput {
        points,
        matched_nav_refs,
        item_data,
        airport_plate_availability,
    } = input;
    let mut matches = Vec::new();
    for point in points {
        let Some(key) = nav_ref_match_key(Some(&point.nav_ref)) else {
            continue;
        };
        if matched_nav_refs.contains(&key) {
            continue;
        }
        let screen_point =
            nearest_wrapped_screen_point(center_world, scale, width_px, height_px, point.position);
        let distance_px = ((screen_point.x - click_screen.x).powi(2)
            + (screen_point.y - click_screen.y).powi(2))
        .sqrt();
        if distance_px > hit_radius_px {
            continue;
        }
        let availability = match &point.nav_ref {
            NavRef::Airport(airport_id) => airport_plate_availability(airport_id),
            _ => AirportPlateAvailability::default(),
        };
        matches.push(MapSelectionPointMatch {
            item: selection_item_for_nav_ref_point(point, &item_data, availability),
            distance_px,
        });
    }
    matches
}

fn selection_item_for_nav_ref_point(
    point: &NavRefSelectionPoint,
    data: &SelectionItemData<'_>,
    airport_plate_availability: AirportPlateAvailability,
) -> MapSelectionItem {
    let plan = data.plan;
    let nav_ref = &point.nav_ref;
    let label = chart_ident_label_for_nav_ref_symbol(nav_ref, &point.symbol);
    let mut symbol_feature = point.symbol.clone();
    symbol_feature.label = label.clone();
    let remove_row_action =
        selection_flight_plan_row_action(plan, nav_ref, FlightPlanRowActionId::Remove);
    let direct_to_row_action =
        selection_flight_plan_row_action(plan, nav_ref, FlightPlanRowActionId::DirectTo);
    let insert_action = if remove_row_action.is_some() {
        row_action("remove_from_flight_plan", "Remove", remove_row_action)
    } else if selection_plan_top_level_waypoint_count(plan, nav_ref) > 1 {
        disabled_action_with_reason(
            "remove_from_flight_plan",
            "Remove",
            "This waypoint appears more than once in the flight plan.",
        )
    } else if !selection_plan_contains_nav_ref(plan, nav_ref) {
        insert_best_position_action(plan, nav_ref)
    } else {
        disabled_action_with_reason(
            "insert",
            "Insert",
            "Edit grouped routes from the Flight Plan page.",
        )
    };
    let weather_detail = match nav_ref {
        NavRef::Airport(airport_id) => weather_detail_for_airport(
            airport_id,
            data.weather_station_airport_aliases,
            data.metar_payload,
            data.taf_payload,
            data.notam_payload,
            data.weather_age_reference_utc,
        ),
        _ => None,
    };
    let mut actions = if let NavRef::Airport(airport_id) = nav_ref {
        vec![
            direct_to_action(plan, Some(nav_ref), direct_to_row_action),
            insert_action,
            plate_target_action(
                "plates",
                "Plates",
                airport_id,
                "Folder",
                airport_plate_availability.plates,
            ),
            plate_target_action(
                "csup",
                "CSUP",
                airport_id,
                "CSup",
                airport_plate_availability.csup,
            ),
            weather_action(weather_detail.clone()),
            airport_info_action(airport_id),
        ]
    } else {
        vec![
            direct_to_action(plan, Some(nav_ref), direct_to_row_action),
            insert_action,
        ]
    };
    MapSelectionItem {
        id: point.feature_id.clone(),
        label,
        sublabel: point.symbol.kind.trim().to_ascii_uppercase(),
        description: None,
        distance: None,
        secondary_description: None,
        position: Some(point.position),
        elevation_msl_ft: None,
        detail_text: None,
        highlight: MapSelectionHighlight::FeatureRef {
            id: point.feature_id.clone(),
        },
        nav_ref: Some(nav_ref.clone()),
        symbol_feature: Some(symbol_feature),
        metar_feature: None,
        weather_detail,
        automatic_action_uid: None,
        pirep_feature: None,
        airspace_icon: None,
        actions: {
            actions.shrink_to_fit();
            actions
        },
    }
}

fn nav_ref_match_key(nav_ref: Option<&NavRef>) -> Option<String> {
    nav_ref.map(|nav_ref| serde_json::to_string(nav_ref).unwrap_or_else(|_| format!("{nav_ref:?}")))
}

pub fn selected_map_selection_item_id_for_nav_ref(
    result: &MapSelectionQueryResult,
    nav_ref: &NavRef,
) -> Option<String> {
    let key = nav_ref_match_key(Some(nav_ref))?;
    result.categories.iter().find_map(|category| {
        category
            .items
            .iter()
            .find(|candidate| {
                nav_ref_match_key(candidate.nav_ref.as_ref()).as_deref() == Some(&key)
            })
            .map(|item| item.id.clone())
    })
}

fn insert_best_position_action(plan: Option<&FlightPlan>, nav_ref: &NavRef) -> MapSelectionAction {
    let Some(plan) = plan else {
        return disabled_action_with_reason(
            "insert",
            "Insert",
            "Start a flight plan before inserting a waypoint.",
        );
    };
    if let Some(reason) = crate::had_ops::insert_waypoint_best_position_rejection(plan, nav_ref) {
        return disabled_action_with_reason("insert", "Insert", reason);
    }
    session_action(
        "insert",
        "Insert",
        MapSelectionSessionAction::InsertWaypointBestPosition {
            nav_ref: nav_ref.clone(),
        },
    )
}

fn spot_selection_item(click: LatLon, plan: Option<&FlightPlan>) -> MapSelectionItem {
    let nav_ref = NavRef::Spot(click);
    let coordinates = format!("{:.4}, {:.4}", click.lat, click.lon);
    MapSelectionItem {
        id: format!("spot:{:.6}:{:.6}", click.lat, click.lon),
        label: "SPOT".to_string(),
        sublabel: coordinates.clone(),
        description: None,
        distance: None,
        secondary_description: Some(coordinates),
        position: Some(click),
        elevation_msl_ft: None,
        detail_text: None,
        highlight: MapSelectionHighlight::Spot {
            lat: click.lat,
            lon: click.lon,
        },
        nav_ref: Some(nav_ref.clone()),
        symbol_feature: None,
        metar_feature: None,
        weather_detail: None,
        automatic_action_uid: None,
        pirep_feature: None,
        airspace_icon: None,
        actions: vec![
            direct_to_action(None, Some(&nav_ref), None),
            insert_best_position_action(plan, &nav_ref),
        ],
    }
}

fn selection_item_for_metar(
    record: &MetarRecord,
    taf: Option<&TafRecord>,
    feature: VisibleMetarFeature,
    notam_payload: Option<&NotamDisplayIndex>,
    weather_station_airport_aliases: &WeatherStationAirportAliases,
    weather_age_reference_utc: Option<DateTime<Utc>>,
) -> MapSelectionItem {
    let source_station_id = record.station_id.trim().to_ascii_uppercase();
    let airport_id = weather_station_airport_aliases.airport_id_for_station(
        &source_station_id,
        LatLon {
            lat: record.latitude,
            lon: record.longitude,
        },
    );
    let display_id = airport_id.unwrap_or(&source_station_id);
    let notams = airport_notam_views(display_id, notam_payload);
    let weather_detail = weather_detail_from_records(
        display_id,
        Some(record),
        taf,
        notams,
        weather_age_reference_utc,
    );
    MapSelectionItem {
        id: format!("metar:{}", record.station_id.trim()),
        label: display_id.to_string(),
        sublabel: normalized_metar_flight_category(record).to_ascii_uppercase(),
        description: record.observed_at_utc.clone(),
        distance: None,
        secondary_description: None,
        position: Some(LatLon {
            lat: record.latitude,
            lon: record.longitude,
        }),
        elevation_msl_ft: None,
        detail_text: None,
        highlight: MapSelectionHighlight::Metar {
            station_id: record.station_id.clone(),
        },
        nav_ref: airport_id.map(|airport_id| NavRef::Airport(airport_id.to_string())),
        symbol_feature: None,
        metar_feature: Some(feature),
        weather_detail: weather_detail.clone(),
        automatic_action_uid: None,
        pirep_feature: None,
        airspace_icon: None,
        actions: vec![weather_action(weather_detail)],
    }
}

fn selection_item_for_pirep(
    record: &PirepRecord,
    feature: VisiblePirepFeature,
) -> MapSelectionItem {
    let hazard_label = pirep_hazard_label(record);
    MapSelectionItem {
        id: record.id.clone(),
        label: hazard_label,
        sublabel: record
            .report_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("PIREP")
            .to_ascii_uppercase(),
        description: record.observed_at_utc.clone(),
        distance: None,
        secondary_description: None,
        position: Some(LatLon {
            lat: record.latitude,
            lon: record.longitude,
        }),
        elevation_msl_ft: None,
        detail_text: Some(record.raw_text.clone()),
        highlight: MapSelectionHighlight::Pirep {
            id: record.id.clone(),
        },
        nav_ref: None,
        symbol_feature: None,
        metar_feature: None,
        weather_detail: None,
        automatic_action_uid: None,
        pirep_feature: Some(feature),
        airspace_icon: None,
        actions: vec![display_action("pirep", "PIREP")],
    }
}

fn pirep_hazard_label(record: &PirepRecord) -> String {
    let symbol = normalized_pirep_symbol(&record.symbol);
    match symbol.as_str() {
        "light-icing" => "Lgt Ice".to_string(),
        "moderate-icing" => "Mod Ice".to_string(),
        "severe-icing" => "Sev Ice".to_string(),
        "light-turbulence" => "Lgt Turb".to_string(),
        "moderate-turbulence" => "Mod Turb".to_string(),
        "severe-turbulence" => "Sev Turb".to_string(),
        _ => "PIREP".to_string(),
    }
}

#[cfg(test)]
pub(crate) fn weather_detail_for_station(
    station_id: &str,
    aliases: &WeatherStationAirportAliases,
    metar_payload: Option<&MetarProductPayload>,
    taf_payload: Option<&TafProductPayload>,
    notam_index: Option<&NotamDisplayIndex>,
    age_reference_utc: Option<DateTime<Utc>>,
) -> Option<WeatherDetailUiView> {
    let station_id = station_id.trim().to_ascii_uppercase();
    if station_id.is_empty() {
        return None;
    }
    let metar = metar_payload.and_then(|payload| payload.metars_by_station.get(&station_id));
    let taf = taf_payload.and_then(|payload| payload.tafs_by_station.get(&station_id));
    let station_position = metar
        .map(|record| LatLon {
            lat: record.latitude,
            lon: record.longitude,
        })
        .or_else(|| {
            taf.map(|record| LatLon {
                lat: record.latitude,
                lon: record.longitude,
            })
        });
    let airport_id = station_position
        .and_then(|position| aliases.airport_id_for_station(&station_id, position))
        .unwrap_or(&station_id);
    let notams = airport_notam_views(airport_id, notam_index);
    weather_detail_from_records(airport_id, metar, taf, notams, age_reference_utc)
}

pub(crate) fn weather_detail_for_airport(
    airport_id: &str,
    aliases: &WeatherStationAirportAliases,
    metar_payload: Option<&MetarProductPayload>,
    taf_payload: Option<&TafProductPayload>,
    notam_index: Option<&NotamDisplayIndex>,
    age_reference_utc: Option<DateTime<Utc>>,
) -> Option<WeatherDetailUiView> {
    let airport_id = airport_id.trim().to_ascii_uppercase();
    if airport_id.is_empty() {
        return None;
    }
    let station_id =
        weather_station_id_for_airport(&airport_id, aliases, metar_payload, taf_payload);
    let metar = metar_payload.and_then(|payload| payload.metars_by_station.get(&station_id));
    let taf = taf_payload.and_then(|payload| payload.tafs_by_station.get(&station_id));
    let notams = airport_notam_views(&airport_id, notam_index);
    weather_detail_from_records(&airport_id, metar, taf, notams, age_reference_utc)
}

fn weather_station_id_for_airport(
    airport_id: &str,
    aliases: &WeatherStationAirportAliases,
    metar_payload: Option<&MetarProductPayload>,
    taf_payload: Option<&TafProductPayload>,
) -> String {
    aliases
        .station_id_for_airport(airport_id)
        .filter(|station_id| {
            let metar_matches = metar_payload
                .and_then(|payload| payload.metars_by_station.get(*station_id))
                .is_some_and(|record| {
                    aliases.airport_id_for_station(
                        station_id,
                        LatLon {
                            lat: record.latitude,
                            lon: record.longitude,
                        },
                    ) == Some(airport_id)
                });
            let taf_matches = taf_payload
                .and_then(|payload| payload.tafs_by_station.get(*station_id))
                .is_some_and(|record| {
                    aliases.airport_id_for_station(
                        station_id,
                        LatLon {
                            lat: record.latitude,
                            lon: record.longitude,
                        },
                    ) == Some(airport_id)
                });
            metar_matches || taf_matches
        })
        .unwrap_or(airport_id)
        .to_string()
}

pub(crate) fn flight_plan_weather_badge_for_airport(
    airport_id: &str,
    aliases: &WeatherStationAirportAliases,
    metar_payload: Option<&MetarProductPayload>,
    age_reference_utc: Option<DateTime<Utc>>,
) -> Option<crate::planning::FlightPlanWeatherBadgeUiView> {
    let airport_id = airport_id.trim().to_ascii_uppercase();
    if airport_id.is_empty() {
        return None;
    }
    let station_id = weather_station_id_for_airport(&airport_id, aliases, metar_payload, None);
    let record = metar_payload?.metars_by_station.get(&station_id)?;
    let observed_at = record
        .observed_at_utc
        .as_deref()
        .and_then(crate::freshness::parse_utc_instant)?;
    let reference = age_reference_utc.filter(|value| *value > DateTime::<Utc>::UNIX_EPOCH)?;
    if reference
        .signed_duration_since(observed_at)
        .num_milliseconds()
        .max(0)
        > FLIGHT_PLAN_METAR_BADGE_MAX_AGE_MS
    {
        return None;
    }
    let flight_category = normalized_metar_flight_category(record);
    if flight_category == "missing" {
        return None;
    }
    Some(crate::planning::FlightPlanWeatherBadgeUiView {
        flight_category,
        ceiling_amount: normalized_metar_ceiling_amount(record),
    })
}

fn weather_detail_from_records(
    station_id: &str,
    metar: Option<&MetarRecord>,
    taf: Option<&TafRecord>,
    notams: Vec<AirportNotamUiView>,
    age_reference_utc: Option<DateTime<Utc>>,
) -> Option<WeatherDetailUiView> {
    let metar_text = metar.map(|record| record.raw_text.clone());
    let (metar_age_label, metar_age_warning) = weather_age_status(
        metar.and_then(|record| record.observed_at_utc.as_deref()),
        age_reference_utc,
        METAR_AGE_WARNING_MS,
    );
    let taf_text = taf.map(taf_detail_text);
    let (taf_age_label, taf_age_warning) = weather_age_status(
        taf.and_then(|record| record.issued_at_utc.as_deref()),
        age_reference_utc,
        TAF_AGE_WARNING_MS,
    );
    if metar_text.is_none() && taf_text.is_none() && notams.is_empty() {
        return None;
    }
    Some(WeatherDetailUiView {
        station_id: station_id.trim().to_ascii_uppercase(),
        title: format!("WX {}", station_id.trim().to_ascii_uppercase()),
        advisory_text: WEATHER_DETAIL_ADVISORY_TEXT.to_string(),
        sections: vec![
            WeatherDetailSectionUiView {
                kind: WeatherDetailSectionKind::Text,
                label: "METAR".to_string(),
                trailing_label: metar_age_label.clone(),
                trailing_warning: metar_age_warning,
                text: metar_text.clone(),
                empty_text: "No METAR available.".to_string(),
                notams: Vec::new(),
            },
            WeatherDetailSectionUiView {
                kind: WeatherDetailSectionKind::Text,
                label: "TAF".to_string(),
                trailing_label: taf_age_label.clone(),
                trailing_warning: taf_age_warning,
                text: taf_text.clone(),
                empty_text: "No TAF available.".to_string(),
                notams: Vec::new(),
            },
            WeatherDetailSectionUiView {
                kind: WeatherDetailSectionKind::Notams,
                label: "NOTAM".to_string(),
                trailing_label: Some(notams.len().to_string()),
                trailing_warning: false,
                text: None,
                empty_text: "No airport NOTAMs available.".to_string(),
                notams: notams.clone(),
            },
        ],
        metar_text,
        metar_age_label,
        metar_age_warning,
        taf_text,
        taf_age_label,
        taf_age_warning,
        notams,
    })
}

fn airport_notam_views(
    airport_id: &str,
    index: Option<&NotamDisplayIndex>,
) -> Vec<AirportNotamUiView> {
    let Some(index) = index else {
        return Vec::new();
    };
    let lookup_ids = airport_notam_lookup_ids(airport_id);
    let mut notams = lookup_ids
        .into_iter()
        .flat_map(|lookup_id| index.airport_records(&lookup_id))
        .map(airport_notam_ui_view)
        .collect::<Vec<_>>();
    notams.sort_by(|left, right| left.id.cmp(&right.id));
    notams.dedup_by(|left, right| left.id == right.id);
    sort_airport_notam_views(&mut notams);
    notams
}

pub(crate) fn airport_unmatched_procedure_notam_views(
    airport_id: &str,
    attached_keys: &BTreeSet<ProcedureRendezvousKey>,
    index: Option<&NotamDisplayIndex>,
) -> Vec<AirportNotamUiView> {
    let Some(index) = index else {
        return Vec::new();
    };
    let attached_keys = attached_keys
        .iter()
        .map(NotamDisplayProcedureKey::from)
        .collect::<BTreeSet<_>>();
    let lookup_ids = airport_notam_lookup_ids(airport_id);
    let mut notams = lookup_ids
        .into_iter()
        .flat_map(|lookup_id| index.airport_records(&lookup_id))
        .filter(|record| matches!(record.label.as_str(), "IAP" | "ODP" | "SID" | "STAR"))
        .filter(|record| {
            record.procedure_rendezvous_keys.is_empty()
                || record
                    .procedure_rendezvous_keys
                    .iter()
                    .any(|key| !attached_keys.contains(key))
        })
        .map(airport_notam_ui_view)
        .collect::<Vec<_>>();
    notams.sort_by(|left, right| left.id.cmp(&right.id));
    notams.dedup_by(|left, right| left.id == right.id);
    sort_airport_notam_views(&mut notams);
    notams
}

pub fn procedure_notam_views(
    keys: &BTreeSet<ProcedureRendezvousKey>,
    index: Option<&NotamDisplayIndex>,
) -> Vec<AirportNotamUiView> {
    let Some(index) = index else {
        return Vec::new();
    };
    let mut notams = index
        .procedure_records(keys)
        .into_iter()
        .map(airport_notam_ui_view)
        .collect::<Vec<_>>();
    sort_airport_notam_views(&mut notams);
    notams
}

fn airport_notam_ui_view(record: &NotamDisplayRecord) -> AirportNotamUiView {
    AirportNotamUiView {
        id: record.id.clone(),
        label: record.label.clone(),
        text: record.text.clone(),
        priority: record.priority,
    }
}

fn sort_airport_notam_views(notams: &mut [AirportNotamUiView]) {
    notams.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.label.cmp(&right.label))
            .then_with(|| left.text.cmp(&right.text))
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn airport_notam_priority(effects: &BTreeSet<AirportNotamEffect>) -> u8 {
    effects
        .iter()
        .map(|effect| match effect {
            AirportNotamEffect::AirportClosed => 0,
            AirportNotamEffect::RunwayClosed => 1,
            AirportNotamEffect::ProcedureUnavailable => 2,
            AirportNotamEffect::RunwayRestricted => 3,
            AirportNotamEffect::RunwayEquipmentUnavailable => 4,
            AirportNotamEffect::TaxiwayClosed => 5,
            AirportNotamEffect::ApronClosed => 6,
            AirportNotamEffect::ProcedureRestricted => 7,
            AirportNotamEffect::MovementAreaEquipmentUnavailable => 8,
            AirportNotamEffect::SurfaceCondition => 9,
            AirportNotamEffect::WorkInProgress => 10,
            AirportNotamEffect::RoutineAdvisory => 11,
            AirportNotamEffect::Other => 12,
        })
        .min()
        .unwrap_or(12)
}

fn airport_notam_lookup_ids(airport_id: &str) -> HashSet<String> {
    let airport_id = airport_id.trim().to_ascii_uppercase();
    let mut ids = HashSet::from([airport_id.clone()]);
    if airport_id.len() == 4 && airport_id.starts_with('K') {
        ids.insert(airport_id[1..].to_string());
    } else if airport_id.len() == 3 && airport_id.chars().all(|ch| ch.is_ascii_alphabetic()) {
        ids.insert(format!("K{airport_id}"));
    }
    ids
}

const MINUTE_MS: i64 = 60_000;
const HOUR_MS: i64 = 60 * MINUTE_MS;
const DAY_MS: i64 = 24 * HOUR_MS;
const METAR_AGE_WARNING_MS: i64 = HOUR_MS;
const FLIGHT_PLAN_METAR_BADGE_MAX_AGE_MS: i64 = 90 * MINUTE_MS;
const TAF_AGE_WARNING_MS: i64 = 6 * HOUR_MS;

fn weather_age_status(
    timestamp_utc: Option<&str>,
    reference_utc: Option<DateTime<Utc>>,
    warning_after_ms: i64,
) -> (Option<String>, bool) {
    let Some(timestamp) = timestamp_utc.and_then(crate::freshness::parse_utc_instant) else {
        return (None, false);
    };
    let Some(reference) = reference_utc else {
        return (None, false);
    };
    if reference <= DateTime::<Utc>::UNIX_EPOCH {
        return (None, false);
    }
    let age_ms = reference
        .signed_duration_since(timestamp)
        .num_milliseconds()
        .max(0);
    (
        Some(format!("{} old", format_weather_age(age_ms))),
        age_ms > warning_after_ms,
    )
}

fn format_weather_age(age_ms: i64) -> String {
    let age_ms = age_ms.max(0);
    if age_ms < HOUR_MS {
        return format!("{}m", age_ms.div_euclid(MINUTE_MS));
    }
    if age_ms < DAY_MS {
        let hours = age_ms as f64 / HOUR_MS as f64;
        return format!("{hours:.1}h");
    }
    let days = age_ms as f64 / DAY_MS as f64;
    format!("{days:.1}d")
}

fn taf_detail_text(record: &TafRecord) -> String {
    let mut formatted = String::new();
    for token in record.raw_text.split_whitespace() {
        if token == "BECMG" || taf_token_is_from_time_group(token) {
            formatted.push('\n');
        } else if !formatted.is_empty() {
            formatted.push(' ');
        }
        formatted.push_str(token);
    }
    formatted
}

fn taf_token_is_from_time_group(token: &str) -> bool {
    token
        .strip_prefix("FM")
        .is_some_and(|suffix| suffix.len() >= 6 && suffix.chars().all(|ch| ch.is_ascii_digit()))
}

fn selection_item_for_airspace(feature: &AirspaceFeaturePayload) -> MapSelectionItem {
    let label = airspace_selection_label(feature);
    let published_name = feature.name.trim();
    MapSelectionItem {
        id: feature.id.clone(),
        description: (!published_name.is_empty() && published_name != label)
            .then(|| published_name.to_string()),
        label,
        sublabel: feature.ident.trim().to_string(),
        distance: None,
        secondary_description: None,
        position: None,
        elevation_msl_ft: None,
        detail_text: None,
        highlight: MapSelectionHighlight::FeatureRef {
            id: feature.id.clone(),
        },
        nav_ref: None,
        symbol_feature: None,
        metar_feature: None,
        weather_detail: None,
        automatic_action_uid: None,
        pirep_feature: None,
        airspace_icon: airspace_selection_icon(feature),
        actions: vec![airspace_limit_action_from_parts(
            "limits",
            feature.vertical.upper.display.trim().to_string(),
            feature.vertical.lower.display.trim().to_string(),
            &airspace_style_key(&feature.style_hint),
        )],
    }
}

fn airspace_selection_label(feature: &AirspaceFeaturePayload) -> String {
    let ident = feature.ident.trim();
    let airspace_class = feature.airspace_class.trim().to_ascii_uppercase();
    if !ident.is_empty() && matches!(airspace_class.as_str(), "B" | "C" | "D") {
        return format!("{} {airspace_class}", ident.to_ascii_uppercase());
    }
    let name = feature.name.trim();
    if !name.is_empty() {
        name.to_string()
    } else {
        feature.ident.trim().to_string()
    }
}

struct MapSelectionPointMatch {
    item: MapSelectionItem,
    distance_px: f64,
}

fn compare_map_selection_point_matches(
    left: &MapSelectionPointMatch,
    right: &MapSelectionPointMatch,
) -> std::cmp::Ordering {
    left.distance_px
        .total_cmp(&right.distance_px)
        .then_with(|| left.item.label.cmp(&right.item.label))
        .then_with(|| left.item.id.cmp(&right.item.id))
}

fn selection_item_for_offline_region_group(regions: &[&OfflineRegionRecord]) -> MapSelectionItem {
    let first = regions
        .first()
        .expect("offline region selection group must be non-empty");
    let mut labels = regions
        .iter()
        .map(|region| region.label.clone())
        .collect::<Vec<_>>();
    labels.sort();
    labels.dedup();
    let description = labels.join(", ");
    let region_detail = offline_region_group_detail_text(regions);
    let mode_label = offline_region_mode_action_label(first);
    MapSelectionItem {
        id: format!("offline-region:{}", first.region_id.to_ascii_lowercase()),
        label: first.region_id.to_ascii_uppercase(),
        sublabel: description.clone(),
        description: Some(description),
        distance: None,
        secondary_description: None,
        position: None,
        elevation_msl_ft: None,
        detail_text: Some(region_detail.clone()),
        highlight: MapSelectionHighlight::OfflineRegion {
            id: first.id.clone(),
        },
        nav_ref: None,
        symbol_feature: None,
        metar_feature: None,
        weather_detail: None,
        automatic_action_uid: None,
        pirep_feature: None,
        airspace_icon: None,
        actions: vec![
            text_detail_action(
                "offline_region_mode",
                &mode_label,
                Some(&mode_label),
                Some(region_detail),
                "Offline region details are unavailable.",
            ),
            text_detail_action(
                "offline_packages",
                "Offline\nPkgs",
                Some("Offline Packages"),
                Some("Offline Packages settings are not available on this platform.".to_string()),
                "Offline Packages settings are unavailable.",
            ),
        ],
    }
}

fn offline_region_detail_text(region: &OfflineRegionRecord) -> String {
    if region.summary.is_empty() {
        return region.label.clone();
    }
    let summary = region
        .summary
        .iter()
        .map(|entry| {
            let suffix = if entry.count > 1 {
                format!(" ({})", entry.count)
            } else {
                String::new()
            };
            format!("{} {}{}", entry.action, entry.cycle, suffix)
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}\n{}", region.label, summary)
}

fn offline_region_group_detail_text(regions: &[&OfflineRegionRecord]) -> String {
    regions
        .iter()
        .map(|region| offline_region_detail_text(region))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn offline_region_mode_action_label(region: &OfflineRegionRecord) -> String {
    format!("{} Region", region.region_id.to_ascii_uppercase())
}

fn selectable_airspace_feature(feature: &AirspaceFeaturePayload) -> bool {
    !feature.id.contains(":outline:")
}

fn selectable_airspace_feature_ids_for_viewport(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    config: &MapOverlayConfig,
    vector_tile_cache: &HashMap<String, VectorAggregateTilePayload>,
    point_display_scale: f64,
) -> BTreeSet<String> {
    let effective_zoom = effective_point_display_zoom(viewport, point_display_scale);
    if effective_zoom < AIRSPACE_MIN_DISPLAY_ZOOM || width_px <= 0.0 || height_px <= 0.0 {
        return BTreeSet::new();
    }
    let ref_zoom = airspace_reference_zoom(effective_zoom, config);
    visible_layer_tile_window("airspace", ref_zoom, viewport, width_px, height_px)
        .into_iter()
        .filter_map(|tile| {
            vector_tile_cache.get(&aggregate_vector_tile_cache_key(tile.z, tile.x, tile.y))
        })
        .flat_map(|tile| tile.airspace_refs.iter().cloned())
        .collect()
}

fn airspace_selection_icon(feature: &AirspaceFeaturePayload) -> Option<AirspaceDisplayPath> {
    airspace_icon_paths_from_lon_lat_paths(
        feature.paths.iter().map(|path| AirspaceIconSourcePath {
            closed: path.closed,
            interior_side: path.interior_side.clone(),
            points: lon_lat_points_for_airspace_path(path),
        }),
        &feature.id,
        &feature.name,
        &airspace_style_key(&feature.style_hint),
        None,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TfrTimingKind {
    Active,
    Upcoming { starts_in_ms: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TfrTimingState {
    style_key: &'static str,
    kind: TfrTimingKind,
}

fn tfr_selection_icon(
    area: &TfrAreaPayload,
    reference_utc: Option<DateTime<Utc>>,
) -> Option<AirspaceDisplayPath> {
    let style_key = tfr_timing_state(area, reference_utc).style_key;
    airspace_icon_paths_from_lon_lat_paths(
        std::iter::once(AirspaceIconSourcePath {
            closed: true,
            interior_side: None,
            points: area
                .polygon
                .iter()
                .map(|point| [point.lon, point.lat])
                .collect(),
        }),
        &format!("tfr:{}:{}", area.notam_id.trim(), area.area_index),
        area.notam_id.trim(),
        style_key,
        Some(tfr_display_style(style_key)),
    )
}

struct AirspaceIconSourcePath {
    closed: bool,
    interior_side: Option<String>,
    points: Vec<[f64; 2]>,
}

fn airspace_icon_paths_from_lon_lat_paths(
    source_paths: impl IntoIterator<Item = AirspaceIconSourcePath>,
    id: &str,
    name: &str,
    style_key: &str,
    style_override: Option<AirspaceDisplayStyle>,
) -> Option<AirspaceDisplayPath> {
    const ICON_SIZE_PX: f64 = 64.0;
    const ICON_PAD_PX: f64 = 8.0;
    let source_paths = source_paths
        .into_iter()
        .filter(|path| path.points.len() >= 2)
        .collect::<Vec<_>>();
    if source_paths.is_empty() {
        return None;
    }

    let mut min_lon = f64::INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    let mut min_lat = f64::INFINITY;
    let mut max_lat = f64::NEG_INFINITY;
    for path in &source_paths {
        for point in &path.points {
            let lon = point[0];
            let lat = point[1];
            if !lon.is_finite() || !lat.is_finite() {
                continue;
            }
            min_lon = min_lon.min(lon);
            max_lon = max_lon.max(lon);
            min_lat = min_lat.min(lat);
            max_lat = max_lat.max(lat);
        }
    }
    if !min_lon.is_finite() || !max_lon.is_finite() || !min_lat.is_finite() || !max_lat.is_finite()
    {
        return None;
    }
    let center_lon = (min_lon + max_lon) / 2.0;
    let center_lat = (min_lat + max_lat) / 2.0;
    let lon_scale = center_lat.to_radians().cos().abs().max(0.01);
    let lon_span = (max_lon - min_lon).abs() * lon_scale;
    let lat_span = (max_lat - min_lat).abs();
    let max_span = lon_span.max(lat_span);
    if max_span <= f64::EPSILON {
        return None;
    }
    let scale = (ICON_SIZE_PX - ICON_PAD_PX * 2.0) / max_span;
    let icon_center = ICON_SIZE_PX / 2.0;
    let paths = source_paths
        .iter()
        .filter_map(|path| {
            let points = path
                .points
                .iter()
                .filter_map(|point| {
                    let lon = point[0];
                    let lat = point[1];
                    if !lon.is_finite() || !lat.is_finite() {
                        return None;
                    }
                    Some(AirspaceScreenPoint {
                        x: round_screen_coordinate(
                            icon_center + (lon - center_lon) * lon_scale * scale,
                        ),
                        y: round_screen_coordinate(icon_center - (lat - center_lat) * scale),
                    })
                })
                .collect::<Vec<_>>();
            let points = simplify_projected_points(points);
            (points.len() >= 2).then_some(AirspaceDisplaySubpath {
                closed: path.closed,
                interior_side: path.interior_side.clone(),
                points,
            })
        })
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return None;
    }
    let mut decoration_budget = AirspaceDecorationBudget::default();
    let style = style_override.unwrap_or_else(|| airspace_display_style(style_key));
    Some(AirspaceDisplayPath {
        id: id.to_string(),
        name: name.to_string(),
        style_key: style_key.to_string(),
        style,
        decorations: airspace_decorations(style_key, &paths, &mut decoration_budget, None),
        paths,
    })
}

fn selection_item_for_tfr(
    area: &TfrAreaPayload,
    reference_utc: Option<DateTime<Utc>>,
    local_time_zone: Tz,
    time_display_mode: crate::TimeDisplayMode,
) -> MapSelectionItem {
    let notam = area.notam.as_ref();
    let timing = tfr_timing_state(area, reference_utc);
    let mut actions = vec![airspace_limit_action_from_parts(
        "limits",
        tfr_limit_label(&area.upper_limit),
        tfr_limit_label(&area.lower_limit),
        timing.style_key,
    )];
    let mut text_action = text_detail_action(
        "tfr_text",
        "Text",
        Some("TFR"),
        notam.and_then(tfr_notam_detail_text),
        "No TFR text is available for this area.",
    );
    text_action.detail_status =
        tfr_timing_detail_status(area, reference_utc, local_time_zone, time_display_mode);
    actions.push(text_action);
    MapSelectionItem {
        id: format!("tfr:{}:{}", area.notam_id.trim(), area.area_index),
        label: "TFR".to_string(),
        sublabel: area.notam_id.trim().to_string(),
        description: Some(tfr_timing_description(timing)),
        distance: None,
        secondary_description: None,
        position: None,
        elevation_msl_ft: None,
        detail_text: None,
        highlight: MapSelectionHighlight::FeatureRef {
            id: format!("tfr:{}:{}", area.notam_id.trim(), area.area_index),
        },
        nav_ref: None,
        symbol_feature: None,
        metar_feature: None,
        weather_detail: None,
        automatic_action_uid: None,
        pirep_feature: None,
        airspace_icon: tfr_selection_icon(area, reference_utc),
        actions,
    }
}

fn tfr_timing_state(area: &TfrAreaPayload, reference_utc: Option<DateTime<Utc>>) -> TfrTimingState {
    let Some(reference_utc) = reference_utc else {
        return TfrTimingState {
            style_key: TFR_ACTIVE_STYLE_KEY,
            kind: TfrTimingKind::Active,
        };
    };
    let starts_in_ms = tfr_effective_start_utc(area).map(|start| {
        start
            .signed_duration_since(reference_utc)
            .num_milliseconds()
    });
    if let Some(starts_in_ms) = starts_in_ms.filter(|value| *value > HOUR_MS) {
        TfrTimingState {
            style_key: TFR_UPCOMING_STYLE_KEY,
            kind: TfrTimingKind::Upcoming { starts_in_ms },
        }
    } else {
        TfrTimingState {
            style_key: TFR_ACTIVE_STYLE_KEY,
            kind: TfrTimingKind::Active,
        }
    }
}

fn tfr_timing_description(state: TfrTimingState) -> String {
    match state.kind {
        TfrTimingKind::Active => "Active".to_string(),
        TfrTimingKind::Upcoming { starts_in_ms } => {
            format!("Starts in {}", format_weather_age(starts_in_ms))
        }
    }
}

fn tfr_timing_detail_status(
    area: &TfrAreaPayload,
    reference_utc: Option<DateTime<Utc>>,
    local_time_zone: Tz,
    time_display_mode: crate::TimeDisplayMode,
) -> Option<MapSelectionDetailStatus> {
    let reference_utc = reference_utc.filter(|value| *value > DateTime::<Utc>::UNIX_EPOCH)?;
    let start_utc = tfr_effective_start_utc(area);
    let end_utc = tfr_effective_end_utc(area);
    let timing = tfr_timing_state(area, Some(reference_utc));
    let (text, action_id) =
        if let Some(start_utc) = start_utc.filter(|start| *start > reference_utc) {
            let starts_in_ms = start_utc
                .signed_duration_since(reference_utc)
                .num_milliseconds();
            (
                format!(
                    "Starts in {} ({})",
                    format_tfr_duration(starts_in_ms),
                    format_tfr_time(start_utc, local_time_zone, time_display_mode),
                ),
                Some(crate::TOGGLE_TIME_DISPLAY_MODE_ACTION_ID.to_string()),
            )
        } else if let Some(end_utc) = end_utc.filter(|end| *end > reference_utc) {
            let ends_in_ms = end_utc
                .signed_duration_since(reference_utc)
                .num_milliseconds();
            (
                format!(
                    "Active now; ends in {} ({})",
                    format_tfr_duration(ends_in_ms),
                    format_tfr_time(end_utc, local_time_zone, time_display_mode),
                ),
                Some(crate::TOGGLE_TIME_DISPLAY_MODE_ACTION_ID.to_string()),
            )
        } else if end_utc.is_none() {
            ("Active now; no scheduled end".to_string(), None)
        } else {
            (
                "The published effective interval has ended".to_string(),
                None,
            )
        };
    Some(MapSelectionDetailStatus {
        text,
        color_key: timing.style_key.to_string(),
        action_id,
    })
}

fn format_tfr_duration(duration_ms: i64) -> String {
    let duration_ms = duration_ms.max(0);
    if duration_ms < HOUR_MS {
        return format!("{}m", duration_ms.saturating_add(MINUTE_MS - 1) / MINUTE_MS);
    }
    if duration_ms < DAY_MS {
        return format_compact_time_unit(duration_ms as f64 / HOUR_MS as f64, "h");
    }
    format_compact_time_unit(duration_ms as f64 / DAY_MS as f64, "d")
}

fn format_compact_time_unit(value: f64, suffix: &str) -> String {
    let rounded = value.round();
    if (value - rounded).abs() < 0.05 || value >= 10.0 {
        format!("{rounded:.0}{suffix}")
    } else {
        format!("{value:.1}{suffix}")
    }
}

fn format_tfr_time(
    instant: DateTime<Utc>,
    local_time_zone: Tz,
    time_display_mode: crate::TimeDisplayMode,
) -> String {
    crate::format_dated_time(
        instant.timestamp_millis(),
        time_display_mode,
        local_time_zone,
        crate::DatedTimeStyle::Friendly,
    )
}

fn tfr_effective_start_utc(area: &TfrAreaPayload) -> Option<DateTime<Utc>> {
    area.notam
        .as_ref()
        .and_then(|notam| notam.effective_start_utc.as_deref())
        .or_else(|| {
            area.schedule_fragments
                .iter()
                .find(|fragment| fragment.kind.eq_ignore_ascii_case("effective"))
                .map(|fragment| fragment.value_utc.as_str())
        })
        .and_then(crate::freshness::parse_utc_instant)
}

fn tfr_effective_end_utc(area: &TfrAreaPayload) -> Option<DateTime<Utc>> {
    area.notam
        .as_ref()
        .and_then(|notam| notam.effective_end_utc.as_deref())
        .or_else(|| {
            area.schedule_fragments
                .iter()
                .find(|fragment| fragment.kind.eq_ignore_ascii_case("expires"))
                .map(|fragment| fragment.value_utc.as_str())
        })
        .and_then(crate::freshness::parse_utc_instant)
}

fn tfr_notam_detail_text(notam: &TfrNotamMetadata) -> Option<String> {
    notam
        .text
        .as_deref()
        .or(notam.local_text.as_deref())
        .or(notam.icao_text.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn display_action(id: &str, label: &str) -> MapSelectionAction {
    MapSelectionAction {
        id: id.to_string(),
        label: label.to_string(),
        enabled: false,
        display_only: true,
        action_uid: None,
        placeholder: false,
        detail_text: None,
        detail_title: None,
        detail_status: None,
        disabled_reason: None,
        weather_detail: None,
        airport_info_airport_id: None,
        airspace_limit: None,
        session_action: None,
        flight_plan_row_action: None,
        navigation: None,
        external_url: None,
    }
}

fn external_url_action(id: &str, label: &str, url: &str) -> MapSelectionAction {
    MapSelectionAction {
        id: id.to_string(),
        label: label.to_string(),
        enabled: true,
        display_only: false,
        action_uid: None,
        placeholder: false,
        detail_text: None,
        detail_title: None,
        detail_status: None,
        disabled_reason: None,
        weather_detail: None,
        airport_info_airport_id: None,
        airspace_limit: None,
        session_action: None,
        flight_plan_row_action: None,
        navigation: None,
        external_url: Some(url.to_string()),
    }
}

fn weather_action(weather_detail: Option<WeatherDetailUiView>) -> MapSelectionAction {
    MapSelectionAction {
        id: "wx".to_string(),
        label: "WX".to_string(),
        enabled: weather_detail.is_some(),
        display_only: false,
        action_uid: None,
        placeholder: false,
        detail_text: None,
        detail_title: None,
        detail_status: None,
        disabled_reason: weather_detail
            .is_none()
            .then(|| "No METAR, TAF, or airport NOTAM is available for this station.".to_string()),
        weather_detail,
        airport_info_airport_id: None,
        airspace_limit: None,
        session_action: None,
        flight_plan_row_action: None,
        navigation: None,
        external_url: None,
    }
}

fn airport_info_action(airport_id: &str) -> MapSelectionAction {
    MapSelectionAction {
        id: "airport_info".to_string(),
        label: "Info".to_string(),
        enabled: true,
        display_only: false,
        action_uid: None,
        placeholder: false,
        detail_text: None,
        detail_title: None,
        detail_status: None,
        disabled_reason: None,
        weather_detail: None,
        airport_info_airport_id: Some(airport_id.to_string()),
        airspace_limit: None,
        session_action: None,
        flight_plan_row_action: None,
        navigation: None,
        external_url: None,
    }
}

fn text_detail_action(
    id: &str,
    label: &str,
    detail_title: Option<&str>,
    detail_text: Option<String>,
    missing_reason: &str,
) -> MapSelectionAction {
    let enabled = detail_text.is_some();
    MapSelectionAction {
        id: id.to_string(),
        label: label.to_string(),
        enabled,
        display_only: false,
        action_uid: None,
        placeholder: false,
        detail_text,
        detail_title: detail_title.map(str::to_string),
        detail_status: None,
        disabled_reason: (!enabled).then(|| missing_reason.to_string()),
        weather_detail: None,
        airport_info_airport_id: None,
        airspace_limit: None,
        session_action: None,
        flight_plan_row_action: None,
        navigation: None,
        external_url: None,
    }
}

fn plate_target_action(
    id: &str,
    label: &str,
    airport_id: &str,
    target: &str,
    available: bool,
) -> MapSelectionAction {
    MapSelectionAction {
        id: id.to_string(),
        label: label.to_string(),
        enabled: available,
        display_only: false,
        action_uid: None,
        placeholder: false,
        detail_text: None,
        detail_title: None,
        detail_status: None,
        disabled_reason: (!available)
            .then(|| format!("No {label} are available for this airport.")),
        weather_detail: None,
        airport_info_airport_id: None,
        airspace_limit: None,
        session_action: None,
        flight_plan_row_action: None,
        navigation: available.then(|| {
            let chart_id = format!("Plate:{airport_id}:{target}");
            MapSelectionNavigationAction::OpenPlateTarget {
                airport_id: airport_id.to_string(),
                target: target.to_string(),
                chart_id,
            }
        }),
        external_url: None,
    }
}

fn disabled_action_with_reason(
    id: &str,
    label: &str,
    disabled_reason: impl Into<String>,
) -> MapSelectionAction {
    disabled_action_inner(id, label, Some(disabled_reason.into()))
}

fn disabled_action_inner(
    id: &str,
    label: &str,
    disabled_reason: Option<String>,
) -> MapSelectionAction {
    MapSelectionAction {
        id: id.to_string(),
        label: label.to_string(),
        enabled: false,
        display_only: false,
        action_uid: None,
        placeholder: false,
        detail_text: None,
        detail_title: None,
        detail_status: None,
        disabled_reason,
        weather_detail: None,
        airport_info_airport_id: None,
        airspace_limit: None,
        session_action: None,
        flight_plan_row_action: None,
        navigation: None,
        external_url: None,
    }
}

fn row_action(
    id: &str,
    label: &str,
    flight_plan_row_action: Option<MapSelectionFlightPlanRowAction>,
) -> MapSelectionAction {
    MapSelectionAction {
        id: id.to_string(),
        label: label.to_string(),
        enabled: flight_plan_row_action.is_some(),
        display_only: false,
        action_uid: None,
        placeholder: false,
        detail_text: None,
        detail_title: None,
        detail_status: None,
        disabled_reason: None,
        weather_detail: None,
        airport_info_airport_id: None,
        airspace_limit: None,
        session_action: None,
        flight_plan_row_action,
        navigation: None,
        external_url: None,
    }
}

fn session_action(id: &str, label: &str, action: MapSelectionSessionAction) -> MapSelectionAction {
    let session_action = serde_json::to_string(&action).ok();
    MapSelectionAction {
        id: id.to_string(),
        label: label.to_string(),
        enabled: session_action.is_some(),
        display_only: false,
        action_uid: None,
        placeholder: false,
        detail_text: None,
        detail_title: None,
        detail_status: None,
        disabled_reason: None,
        weather_detail: None,
        airport_info_airport_id: None,
        airspace_limit: None,
        session_action,
        flight_plan_row_action: None,
        navigation: None,
        external_url: None,
    }
}

fn direct_to_action(
    _plan: Option<&FlightPlan>,
    nav_ref: Option<&NavRef>,
    flight_plan_row_action: Option<MapSelectionFlightPlanRowAction>,
) -> MapSelectionAction {
    if let Some(flight_plan_row_action) = flight_plan_row_action {
        return row_action("direct_to", "Direct", Some(flight_plan_row_action));
    }
    if let Some(nav_ref) = nav_ref {
        return session_action(
            "direct_to",
            "Direct",
            MapSelectionSessionAction::ActivateDirectToNavRef {
                nav_ref: nav_ref.clone(),
            },
        );
    }
    disabled_action_with_reason(
        "direct_to",
        "Direct",
        "Direct-to needs a selected waypoint, airport, or fix.",
    )
}

fn airspace_limit_action_from_parts(
    id: &str,
    upper: String,
    lower: String,
    style_key: &str,
) -> MapSelectionAction {
    MapSelectionAction {
        id: id.to_string(),
        label: format!("{upper}/{lower}"),
        enabled: false,
        display_only: true,
        action_uid: None,
        placeholder: false,
        detail_text: None,
        detail_title: None,
        detail_status: None,
        disabled_reason: None,
        weather_detail: None,
        airport_info_airport_id: None,
        airspace_limit: Some(AirspaceLimitGlyph {
            upper,
            lower,
            style_key: style_key.to_string(),
            color_key: airspace_label_color_key(style_key).to_string(),
        }),
        session_action: None,
        flight_plan_row_action: None,
        navigation: None,
        external_url: None,
    }
}

fn airspace_limit_label_parts(label: &str) -> Option<(&str, &str)> {
    let (upper, lower) = label.trim().split_once('/')?;
    let upper = upper.trim();
    let lower = lower.trim();
    if upper.is_empty() || lower.is_empty() {
        None
    } else {
        Some((upper, lower))
    }
}

fn airspace_limit_glyph_from_label(label: &str, style_key: &str) -> Option<AirspaceLimitGlyph> {
    let (upper, lower) = airspace_limit_label_parts(label)?;
    Some(AirspaceLimitGlyph {
        upper: upper.to_string(),
        lower: lower.to_string(),
        style_key: style_key.to_string(),
        color_key: airspace_label_color_key(style_key).to_string(),
    })
}

fn airspace_limit_glyph(upper: String, lower: String, style_key: &str) -> AirspaceLimitGlyph {
    AirspaceLimitGlyph {
        upper,
        lower,
        style_key: style_key.to_string(),
        color_key: airspace_label_color_key(style_key).to_string(),
    }
}

fn selection_record_is_airport(record: &PointVectorRecord) -> bool {
    record.style_class == "airport"
        || record.kind.eq_ignore_ascii_case("airport")
        || record.id.starts_with("airports:")
}

fn airport_ident_label(record: &PointVectorRecord) -> Option<String> {
    record
        .id
        .strip_prefix("airports:")
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(|id| id.to_ascii_uppercase())
}

fn selection_item_description(record: &PointVectorRecord, is_airport: bool) -> Option<String> {
    if is_airport {
        return (!record.label.trim().is_empty()).then(|| record.label.trim().to_string());
    }
    if is_vor_family_kind(&record.kind) {
        return vor_frequency_description(&record.label);
    }
    None
}

fn vor_frequency_description(label: &str) -> Option<String> {
    label
        .split_whitespace()
        .find(|part| part.chars().any(|ch| ch == '.'))
        .map(|part| {
            part.trim_matches(|ch: char| ch == ',' || ch == ';')
                .to_string()
        })
        .filter(|part| !part.is_empty())
}

fn selection_symbol_for_point(
    record: &PointVectorRecord,
    is_airport: bool,
) -> Option<NavSymbolFeature> {
    if is_airport {
        point_vector_record_to_symbol_feature_unfiltered(record, None)
    } else {
        point_vector_record_to_symbol_feature(record, None)
    }
}

fn selection_nav_ref(record: &PointVectorRecord, is_airport: bool) -> Option<NavRef> {
    if is_airport {
        return record
            .id
            .strip_prefix("airports:")
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(|id| NavRef::Airport(id.to_ascii_uppercase()));
    }
    if record.style_class == "nav" {
        return record
            .id
            .strip_prefix("nav:")
            .map(|tail| tail.split(':').next().unwrap_or(tail).trim())
            .filter(|id| !id.is_empty())
            .map(|id| NavRef::Navaid(id.to_ascii_uppercase()));
    }
    if record.style_class == "fix" {
        return record
            .id
            .strip_prefix("fix:")
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(|id| NavRef::Fix(id.to_ascii_uppercase()));
    }
    None
}

fn selection_plan_contains_nav_ref(plan: Option<&FlightPlan>, nav_ref: &NavRef) -> bool {
    plan.map(|plan| crate::flight_plan_contains_nav_ref(plan, nav_ref))
        .unwrap_or(false)
}

fn selection_plan_top_level_waypoint_count(plan: Option<&FlightPlan>, nav_ref: &NavRef) -> usize {
    plan.map(|plan| crate::top_level_waypoint_component_count(plan, nav_ref))
        .unwrap_or(0)
}

fn selection_flight_plan_row_action(
    plan: Option<&FlightPlan>,
    nav_ref: &NavRef,
    action_id: FlightPlanRowActionId,
) -> Option<MapSelectionFlightPlanRowAction> {
    let plan = plan?;
    if crate::top_level_waypoint_component_count(plan, nav_ref) != 1 {
        return None;
    }
    let ui = crate::project_ui_state(plan);
    let row = ui.display_rows.iter().find(|row| {
        row.component_kind == Some(RouteComponentViewKind::Waypoint)
            && row.nav_ref.as_ref() == Some(nav_ref)
            && row.component_uid.is_some()
    })?;
    let action = crate::planning::flight_plan_row_actions(row)
        .find(|action| action.id == action_id && action.enabled)?;
    Some(MapSelectionFlightPlanRowAction {
        row_uid: row.uid.clone(),
        action_uid: action.uid.clone(),
    })
}

fn airspace_feature_contains(feature: &AirspaceFeaturePayload, point: LatLon) -> bool {
    wrapped_lon_candidates(point.lon).into_iter().any(|lon| {
        if lon < feature.bbox[0]
            || point.lat < feature.bbox[1]
            || lon > feature.bbox[2]
            || point.lat > feature.bbox[3]
        {
            return false;
        }
        feature.paths.iter().any(|path| {
            let points = lon_lat_points_for_airspace_path(path);
            path.closed && points.len() >= 3 && lon_lat_polygon_contains(&points, lon, point.lat)
        })
    })
}

fn tfr_area_contains(area: &TfrAreaPayload, point: LatLon) -> bool {
    if area.polygon.len() < 3 {
        return false;
    }
    let polygon = area
        .polygon
        .iter()
        .map(|point| [point.lon, point.lat])
        .collect::<Vec<_>>();
    wrapped_lon_candidates(point.lon)
        .into_iter()
        .any(|lon| lon_lat_polygon_contains(&polygon, lon, point.lat))
}

fn offline_region_contains(region: &OfflineRegionRecord, point: LatLon) -> bool {
    region
        .polygons
        .iter()
        .filter(|polygon| polygon.len() >= 3)
        .any(|polygon| {
            let polygon = polygon
                .iter()
                .map(|point| [point.lon, point.lat])
                .collect::<Vec<_>>();
            wrapped_lon_candidates(point.lon)
                .into_iter()
                .any(|lon| lon_lat_polygon_contains(&polygon, lon, point.lat))
        })
}

fn wrapped_lon_candidates(lon: f64) -> [f64; 3] {
    let normalized = ((lon + 180.0).rem_euclid(360.0)) - 180.0;
    [lon, normalized, normalized + 360.0]
}

fn lon_lat_polygon_contains(points: &[[f64; 2]], lon: f64, lat: f64) -> bool {
    let mut inside = false;
    let mut previous = points.len() - 1;
    for current in 0..points.len() {
        let current_lon = points[current][0];
        let current_lat = points[current][1];
        let previous_lon = points[previous][0];
        let previous_lat = points[previous][1];
        let denom = previous_lat - current_lat;
        let denom = if denom.abs() < f64::EPSILON {
            f64::EPSILON.copysign(denom)
        } else {
            denom
        };
        if ((current_lat > lat) != (previous_lat > lat))
            && (lon < (previous_lon - current_lon) * (lat - current_lat) / denom + current_lon)
        {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

#[derive(Debug, Clone, Copy)]
struct LabelRect {
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
}

impl LabelRect {
    fn padded(self, padding: f64) -> Self {
        Self {
            left: self.left - padding,
            right: self.right + padding,
            top: self.top - padding,
            bottom: self.bottom + padding,
        }
    }

    fn overlaps(self, other: Self) -> bool {
        self.left < other.right
            && self.right > other.left
            && self.top < other.bottom
            && self.bottom > other.top
    }

    fn center(self) -> AirspaceScreenPoint {
        AirspaceScreenPoint {
            x: (self.left + self.right) / 2.0,
            y: (self.top + self.bottom) / 2.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum LabelRef {
    Airspace(usize),
    Point(usize),
    ProtectedPoint,
}

#[derive(Debug, Clone, Copy)]
struct LabelCandidate {
    label_ref: LabelRef,
    rect: LabelRect,
    priority: u8,
    order: usize,
}

fn suppress_overlapping_vector_labels(
    visible_features: &mut [VisibleMapFeature],
    airspace_labels: &mut Vec<AirspaceDisplayLabel>,
    protected_point_features: &[VisibleMapFeature],
    point_display_scale: f64,
) {
    let point_display_scale = point_display_scale.max(0.1);
    let mut candidates = Vec::<LabelCandidate>::new();
    let mut order = 0usize;
    for (index, label) in airspace_labels.iter().enumerate() {
        if let Some(rect) = airspace_label_rect(label, point_display_scale) {
            candidates.push(LabelCandidate {
                label_ref: LabelRef::Airspace(index),
                rect: rect.padded(LABEL_COLLISION_PADDING_PX),
                priority: 0,
                order,
            });
            order += 1;
        }
    }
    for (index, feature) in visible_features.iter().enumerate() {
        if let Some(rect) = point_feature_label_rect(feature, point_display_scale) {
            candidates.push(LabelCandidate {
                label_ref: LabelRef::Point(index),
                rect: rect.padded(LABEL_COLLISION_PADDING_PX),
                priority: point_feature_label_priority(feature),
                order,
            });
            order += 1;
        }
    }
    for feature in protected_point_features {
        if let Some(rect) = point_feature_label_rect(feature, point_display_scale) {
            candidates.push(LabelCandidate {
                label_ref: LabelRef::ProtectedPoint,
                rect: rect.padded(LABEL_COLLISION_PADDING_PX),
                priority: point_feature_label_priority(feature),
                order,
            });
            order += 1;
        }
    }
    candidates.sort_by_key(|candidate| (candidate.priority, candidate.order));

    let mut occupied = Vec::<LabelRect>::new();
    let mut keep_airspace = vec![true; airspace_labels.len()];
    let mut keep_point = vec![true; visible_features.len()];

    for candidate in candidates.into_iter().rev() {
        let label_ref = candidate.label_ref;
        let rect = candidate.rect;
        if occupied.iter().any(|kept| rect.overlaps(*kept)) {
            match label_ref {
                LabelRef::Airspace(index) => keep_airspace[index] = false,
                LabelRef::Point(index) => keep_point[index] = false,
                LabelRef::ProtectedPoint => {}
            }
        } else {
            occupied.push(rect);
        }
    }

    for (index, feature) in visible_features.iter_mut().enumerate() {
        if !keep_point[index] {
            feature.label.clear();
        }
    }
    let mut index = 0usize;
    airspace_labels.retain(|_| {
        let keep = keep_airspace[index];
        index += 1;
        keep
    });
}

fn point_feature_label_priority(feature: &VisibleMapFeature) -> u8 {
    match feature.label_style {
        VectorIdentLabelStyle::ActiveFlightPlan => 60,
        VectorIdentLabelStyle::FlightPlan => 50,
        VectorIdentLabelStyle::Default => match feature.symbol_kind.as_str() {
            "nav" => 40,
            "airport" => 30,
            "weather_camera" => 25,
            "fix" => 20,
            _ => 10,
        },
    }
}

fn point_feature_symbol_rect(feature: &VisibleMapFeature, scale: f64) -> Option<LabelRect> {
    if !feature.screen_x.is_finite() || !feature.screen_y.is_finite() {
        return None;
    }
    let scale = scale.max(0.1);
    let size = match feature.symbol_kind.as_str() {
        "airport" => {
            let runway_span = if feature.has_paved_runway != Some(false)
                && !feature.heliport.unwrap_or(false)
                && !feature.has_water_runway.unwrap_or(false)
                && feature.longest_runway_heading_true_deg.is_some()
            {
                16.0 * feature.runway_length_ratio.max(0.2) + 8.0
            } else {
                20.0
            };
            runway_span.max(20.0)
        }
        "nav" => 22.0,
        "weather_camera" => 22.0,
        "obstacle" => 18.0,
        "fix" => 18.0,
        _ => 16.0,
    } * scale;
    Some(centered_rect(
        feature.screen_x,
        feature.screen_y,
        size,
        size,
    ))
}

fn airspace_label_rect(label: &AirspaceDisplayLabel, scale: f64) -> Option<LabelRect> {
    airspace_limit_label_rect(
        &label.glyph.upper,
        &label.glyph.lower,
        label.screen_x,
        label.screen_y,
        scale,
    )
}

fn airspace_limit_label_rect(
    upper: &str,
    lower: &str,
    screen_x: f64,
    screen_y: f64,
    scale: f64,
) -> Option<LabelRect> {
    if !screen_x.is_finite() || !screen_y.is_finite() {
        return None;
    }
    let scale = scale.max(0.1);
    let width =
        upper.chars().count().max(lower.chars().count()) as f64 * 8.2 * scale + 10.0 * scale;
    let height = 30.0 * scale;
    Some(centered_rect(screen_x, screen_y, width, height))
}

fn point_feature_label_rect(feature: &VisibleMapFeature, scale: f64) -> Option<LabelRect> {
    let text = feature.label.trim();
    if text.is_empty() || !feature.screen_x.is_finite() || !feature.screen_y.is_finite() {
        return None;
    }
    let label_y = if matches!(
        feature.symbol_kind.as_str(),
        "airport" | "nav" | "weather_camera"
    ) {
        -24.0 * scale
    } else if feature.symbol_kind == "obstacle" {
        -14.0 * scale
    } else {
        -15.0 * scale
    };
    let font_px = if feature.symbol_kind == "obstacle" {
        12.0 * scale
    } else {
        14.0 * scale
    };
    let width = text.chars().count() as f64 * font_px * 0.64 + 8.0 * scale;
    Some(centered_rect(
        feature.screen_x,
        feature.screen_y + label_y,
        width,
        font_px + 6.0,
    ))
}

fn centered_rect(center_x: f64, center_y: f64, width: f64, height: f64) -> LabelRect {
    LabelRect {
        left: center_x - width / 2.0,
        right: center_x + width / 2.0,
        top: center_y - height / 2.0,
        bottom: center_y + height / 2.0,
    }
}

struct TfrOverlayProjection {
    needed_tfrs: bool,
    paths: Vec<AirspaceDisplayPath>,
    labels: Vec<AirspaceDisplayLabel>,
}

struct TfrOverlayInput<'a> {
    payload: Option<&'a TfrProductPayload>,
    point_features: &'a [VisibleMapFeature],
    protected_point_features: &'a [VisibleMapFeature],
    reference_utc: Option<DateTime<Utc>>,
}

fn query_tfr_overlay(
    projection: &MapProjectionContext<'_>,
    input: TfrOverlayInput<'_>,
) -> TfrOverlayProjection {
    let metrics = projection.metrics;
    let viewport = &metrics.viewport;
    let width_px = metrics.width_px;
    let height_px = metrics.height_px;
    let center_world = projection.center_world;
    let scale = projection.scale;
    let display_scale = metrics.display_scale;
    let TfrOverlayInput {
        payload: tfr_payload,
        point_features,
        protected_point_features,
        reference_utc,
    } = input;
    if width_px <= 0.0
        || height_px <= 0.0
        || effective_point_display_zoom(viewport, display_scale) < AIRSPACE_MIN_DISPLAY_ZOOM
    {
        return TfrOverlayProjection {
            needed_tfrs: false,
            paths: Vec::new(),
            labels: Vec::new(),
        };
    }
    let Some(payload) = tfr_payload else {
        return TfrOverlayProjection {
            needed_tfrs: true,
            paths: Vec::new(),
            labels: Vec::new(),
        };
    };
    let mut paths = Vec::new();
    let mut labels = Vec::new();
    let mut point_obstacle_rects = Vec::new();
    for feature in point_features.iter().chain(protected_point_features.iter()) {
        if let Some(rect) = point_feature_symbol_rect(feature, display_scale) {
            point_obstacle_rects.push(rect.padded(LABEL_COLLISION_PADDING_PX));
        }
        if let Some(rect) = point_feature_label_rect(feature, display_scale) {
            point_obstacle_rects.push(rect.padded(LABEL_COLLISION_PADDING_PX));
        }
    }
    for area in &payload.areas {
        if area.polygon.len() < 3 {
            continue;
        }
        let Some(bbox) = tfr_bbox(area) else {
            continue;
        };
        if !airspace_bbox_may_intersect_screen(bbox, center_world, scale, width_px, height_px) {
            continue;
        }
        let style_key = tfr_timing_state(area, reference_utc).style_key;
        let projected_points = area
            .polygon
            .iter()
            .map(|point| {
                world_to_screen(
                    center_world,
                    scale,
                    width_px,
                    height_px,
                    LatLon {
                        lat: point.lat,
                        lon: point.lon,
                    },
                )
            })
            .map(|point| AirspaceScreenPoint {
                x: point.x,
                y: point.y,
            })
            .collect::<Vec<_>>();
        if let Some(label_point) =
            tfr_label_screen_point(area, &projected_points, projection, &point_obstacle_rects)
        {
            labels.push(AirspaceDisplayLabel {
                feature_id: format!("tfr:{}:{}", area.notam_id.trim(), area.area_index),
                glyph: airspace_limit_glyph(
                    tfr_limit_label(&area.upper_limit),
                    tfr_limit_label(&area.lower_limit),
                    style_key,
                ),
                screen_x: label_point.x,
                screen_y: label_point.y,
            });
        }
        paths.push(AirspaceDisplayPath {
            id: format!("tfr:{}:{}", area.notam_id.trim(), area.area_index),
            name: area.notam_id.trim().to_string(),
            style_key: style_key.to_string(),
            style: tfr_display_style(style_key),
            paths: vec![AirspaceDisplaySubpath {
                closed: true,
                interior_side: None,
                points: projected_points,
            }],
            decorations: Vec::new(),
        });
    }
    TfrOverlayProjection {
        needed_tfrs: false,
        paths,
        labels,
    }
}

fn tfr_bbox(area: &TfrAreaPayload) -> Option<[f64; 4]> {
    let mut iter = area.polygon.iter();
    let first = iter.next()?;
    let mut west = first.lon;
    let mut south = first.lat;
    let mut east = first.lon;
    let mut north = first.lat;
    for point in iter {
        west = west.min(point.lon);
        south = south.min(point.lat);
        east = east.max(point.lon);
        north = north.max(point.lat);
    }
    Some([west, south, east, north])
}

fn tfr_label_screen_point(
    area: &TfrAreaPayload,
    projected_points: &[AirspaceScreenPoint],
    projection: &MapProjectionContext<'_>,
    point_obstacle_rects: &[LabelRect],
) -> Option<AirspaceScreenPoint> {
    let metrics = projection.metrics;
    let width_px = metrics.width_px;
    let height_px = metrics.height_px;
    let display_scale = metrics.display_scale;
    if !tfr_polygon_can_fit_label(area, projected_points, display_scale) {
        return None;
    }
    let centroid = tfr_polygon_centroid(area)?;
    let point = world_to_screen(
        projection.center_world,
        projection.scale,
        width_px,
        height_px,
        centroid,
    );
    if point.x < 0.0 || point.x > width_px || point.y < 0.0 || point.y > height_px {
        return None;
    }
    let centroid_point = AirspaceScreenPoint {
        x: point.x,
        y: point.y,
    };
    if point_obstacle_rects.is_empty() {
        return Some(centroid_point);
    }
    let upper = tfr_limit_label(&area.upper_limit);
    let lower = tfr_limit_label(&area.lower_limit);
    let Some(centroid_rect) = airspace_limit_label_rect(
        &upper,
        &lower,
        centroid_point.x,
        centroid_point.y,
        display_scale,
    )
    .map(|rect| rect.padded(LABEL_COLLISION_PADDING_PX)) else {
        return Some(centroid_point);
    };
    if !point_obstacle_rects
        .iter()
        .any(|obstacle| centroid_rect.overlaps(*obstacle))
    {
        return Some(centroid_point);
    }
    tfr_decentered_label_point(
        projected_points,
        centroid_point,
        &upper,
        &lower,
        display_scale,
        projection,
        point_obstacle_rects,
    )
    .or(Some(centroid_point))
}

fn tfr_decentered_label_point(
    projected_points: &[AirspaceScreenPoint],
    centroid: AirspaceScreenPoint,
    upper: &str,
    lower: &str,
    display_scale: f64,
    projection: &MapProjectionContext<'_>,
    point_obstacle_rects: &[LabelRect],
) -> Option<AirspaceScreenPoint> {
    let width_px = projection.metrics.width_px;
    let height_px = projection.metrics.height_px;
    let mut best_clear = None::<TfrLabelCandidateScore>;
    let mut best_any = None::<TfrLabelCandidateScore>;
    for candidate in tfr_half_radius_label_candidates(projected_points, centroid) {
        if candidate.x < 0.0
            || candidate.x > width_px
            || candidate.y < 0.0
            || candidate.y > height_px
            || !screen_polygon_contains(projected_points, candidate)
        {
            continue;
        }
        let Some(rect) =
            airspace_limit_label_rect(upper, lower, candidate.x, candidate.y, display_scale)
                .map(|rect| rect.padded(LABEL_COLLISION_PADDING_PX))
        else {
            continue;
        };
        let obstacle_distance_sq = point_obstacle_rects
            .iter()
            .map(|obstacle| squared_screen_distance(rect.center(), obstacle.center()))
            .fold(f64::INFINITY, f64::min);
        let boundary_distance_sq =
            squared_distance_to_nearest_screen_boundary(candidate, projected_points);
        let score = TfrLabelCandidateScore {
            point: candidate,
            score: obstacle_distance_sq.min(boundary_distance_sq * 4.0),
            movement_sq: squared_screen_distance(candidate, centroid),
        };
        update_best_tfr_label_candidate(&mut best_any, score);
        if !point_obstacle_rects
            .iter()
            .any(|obstacle| rect.overlaps(*obstacle))
        {
            update_best_tfr_label_candidate(&mut best_clear, score);
        }
    }
    best_clear.or(best_any).map(|candidate| candidate.point)
}

#[derive(Clone, Copy)]
struct TfrLabelCandidateScore {
    point: AirspaceScreenPoint,
    score: f64,
    movement_sq: f64,
}

fn update_best_tfr_label_candidate(
    best: &mut Option<TfrLabelCandidateScore>,
    candidate: TfrLabelCandidateScore,
) {
    let replace = match *best {
        None => true,
        Some(current) => {
            candidate.score > current.score + 1.0e-6
                || ((candidate.score - current.score).abs() <= 1.0e-6
                    && candidate.movement_sq < current.movement_sq)
        }
    };
    if replace {
        *best = Some(candidate);
    }
}

fn tfr_half_radius_label_candidates(
    projected_points: &[AirspaceScreenPoint],
    centroid: AirspaceScreenPoint,
) -> Vec<AirspaceScreenPoint> {
    let mut candidates = Vec::new();
    for point in projected_points.iter().copied() {
        push_unique_screen_point(&mut candidates, halfway_screen_point(centroid, point));
    }
    for index in 0..projected_points.len() {
        let current = projected_points[index];
        let next = projected_points[(index + 1) % projected_points.len()];
        let edge_midpoint = halfway_screen_point(current, next);
        push_unique_screen_point(
            &mut candidates,
            halfway_screen_point(centroid, edge_midpoint),
        );
    }
    candidates
}

fn push_unique_screen_point(points: &mut Vec<AirspaceScreenPoint>, point: AirspaceScreenPoint) {
    if !point.x.is_finite() || !point.y.is_finite() {
        return;
    }
    if points
        .iter()
        .any(|existing| squared_screen_distance(*existing, point) < 1.0)
    {
        return;
    }
    points.push(point);
}

fn halfway_screen_point(
    first: AirspaceScreenPoint,
    second: AirspaceScreenPoint,
) -> AirspaceScreenPoint {
    AirspaceScreenPoint {
        x: (first.x + second.x) / 2.0,
        y: (first.y + second.y) / 2.0,
    }
}

fn screen_polygon_contains(points: &[AirspaceScreenPoint], point: AirspaceScreenPoint) -> bool {
    if points.len() < 3 || !point.x.is_finite() || !point.y.is_finite() {
        return false;
    }
    let mut inside = false;
    let mut previous = points.len() - 1;
    for current in 0..points.len() {
        let current_point = points[current];
        let previous_point = points[previous];
        let crosses = (current_point.y > point.y) != (previous_point.y > point.y);
        if crosses {
            let crossing_x = (previous_point.x - current_point.x) * (point.y - current_point.y)
                / (previous_point.y - current_point.y)
                + current_point.x;
            if point.x < crossing_x {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}

fn squared_distance_to_nearest_screen_boundary(
    point: AirspaceScreenPoint,
    polygon: &[AirspaceScreenPoint],
) -> f64 {
    if polygon.len() < 2 {
        return f64::INFINITY;
    }
    let mut best = f64::INFINITY;
    for index in 0..polygon.len() {
        best = best.min(squared_distance_to_screen_segment(
            point,
            polygon[index],
            polygon[(index + 1) % polygon.len()],
        ));
    }
    best
}

fn squared_distance_to_screen_segment(
    point: AirspaceScreenPoint,
    start: AirspaceScreenPoint,
    end: AirspaceScreenPoint,
) -> f64 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length_sq = dx * dx + dy * dy;
    if length_sq <= f64::EPSILON {
        return squared_screen_distance(point, start);
    }
    let t = (((point.x - start.x) * dx + (point.y - start.y) * dy) / length_sq).clamp(0.0, 1.0);
    squared_screen_distance(
        point,
        AirspaceScreenPoint {
            x: start.x + t * dx,
            y: start.y + t * dy,
        },
    )
}

fn squared_screen_distance(first: AirspaceScreenPoint, second: AirspaceScreenPoint) -> f64 {
    let dx = first.x - second.x;
    let dy = first.y - second.y;
    dx * dx + dy * dy
}

fn tfr_polygon_centroid(area: &TfrAreaPayload) -> Option<LatLon> {
    if area.polygon.len() < 3 {
        return None;
    }
    let mut twice_signed_area = 0.0;
    let mut centroid_lon = 0.0;
    let mut centroid_lat = 0.0;
    for index in 0..area.polygon.len() {
        let current = &area.polygon[index];
        let next = &area.polygon[(index + 1) % area.polygon.len()];
        let cross = current.lon * next.lat - next.lon * current.lat;
        twice_signed_area += cross;
        centroid_lon += (current.lon + next.lon) * cross;
        centroid_lat += (current.lat + next.lat) * cross;
    }
    if twice_signed_area.abs() < f64::EPSILON {
        let (sum_lat, sum_lon, count) =
            area.polygon
                .iter()
                .fold((0.0, 0.0, 0usize), |(sum_lat, sum_lon, count), point| {
                    (sum_lat + point.lat, sum_lon + point.lon, count + 1)
                });
        if count == 0 {
            return None;
        }
        return Some(LatLon {
            lat: sum_lat / count as f64,
            lon: sum_lon / count as f64,
        });
    }
    let scale = 1.0 / (3.0 * twice_signed_area);
    Some(LatLon {
        lat: centroid_lat * scale,
        lon: centroid_lon * scale,
    })
}

fn tfr_polygon_can_fit_label(
    area: &TfrAreaPayload,
    projected_points: &[AirspaceScreenPoint],
    display_scale: f64,
) -> bool {
    let Some((bbox_width, bbox_height)) = projected_bbox_size(projected_points) else {
        return false;
    };
    let scale = normalized_display_scale(display_scale);
    let label_width = tfr_fraction_label_width_px(area) * scale;
    let label_height = 22.0 * scale;
    bbox_width >= label_width && bbox_height >= label_height
}

fn projected_bbox_size(points: &[AirspaceScreenPoint]) -> Option<(f64, f64)> {
    let mut iter = points.iter();
    let first = iter.next()?;
    let (mut min_x, mut max_x) = (first.x, first.x);
    let (mut min_y, mut max_y) = (first.y, first.y);
    for point in iter {
        min_x = min_x.min(point.x);
        max_x = max_x.max(point.x);
        min_y = min_y.min(point.y);
        max_y = max_y.max(point.y);
    }
    Some((max_x - min_x, max_y - min_y))
}

fn tfr_fraction_label_width_px(area: &TfrAreaPayload) -> f64 {
    let upper = tfr_limit_label(&area.upper_limit);
    let lower = tfr_limit_label(&area.lower_limit);
    let width_chars = upper.len().max(lower.len()).max(2);
    (width_chars as f64) * 7.2 + 6.0
}

fn tfr_limit_label(limit: &TfrAltitudeLimit) -> String {
    let value = limit.value_text.trim();
    if value == "0" {
        return "SFC".to_string();
    }
    if limit.unit.trim() == "FL" {
        return format!("FL{value}");
    }
    value.to_string()
}

struct AirspaceOverlayProjection {
    needed_tiles: Vec<VectorTileRequest>,
    needed_features: Vec<AirspaceFeatureRequest>,
    paths: Vec<AirspaceDisplayPath>,
    labels: Vec<AirspaceDisplayLabel>,
    data_status_records: Vec<DataStatusRecord>,
}

fn airspace_overlay_input_requests(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    config: &MapOverlayConfig,
    vector_tile_cache: &HashMap<String, VectorAggregateTilePayload>,
    feature_cache: &HashMap<String, AirspaceFeaturePayload>,
    point_display_scale: f64,
) -> VectorOverlayInputRequests {
    let scan = scan_airspace_inputs(
        viewport,
        width_px,
        height_px,
        config,
        vector_tile_cache,
        point_display_scale,
        |_payload| {},
    );
    let needed_airspace_features = scan
        .feature_ids
        .into_iter()
        .filter(|feature_id| !feature_cache.contains_key(feature_id))
        .map(|feature_id| AirspaceFeatureRequest {
            path: airspace_feature_path(&feature_id),
            id: feature_id,
        })
        .collect();

    VectorOverlayInputRequests {
        needed_vector_tiles: scan.needed_tiles,
        needed_airspace_features,
    }
}

fn scan_airspace_inputs<F>(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    config: &MapOverlayConfig,
    vector_tile_cache: &HashMap<String, VectorAggregateTilePayload>,
    point_display_scale: f64,
    mut on_label_tile: F,
) -> AirspaceInputScan
where
    F: FnMut(&VectorAggregateTilePayload),
{
    let effective_zoom = effective_point_display_zoom(viewport, point_display_scale);
    if effective_zoom < AIRSPACE_MIN_DISPLAY_ZOOM || width_px <= 0.0 || height_px <= 0.0 {
        return AirspaceInputScan {
            needed_tiles: Vec::new(),
            feature_ids: BTreeSet::new(),
        };
    }

    let mut needed_tiles = Vec::new();
    let mut needed_seen = BTreeSet::new();
    let mut feature_ids = BTreeSet::new();
    let ref_zoom = airspace_reference_zoom(effective_zoom, config);
    for tile in visible_layer_tile_window("airspace", ref_zoom, viewport, width_px, height_px) {
        let key = aggregate_vector_tile_cache_key(tile.z, tile.x, tile.y);
        let Some(payload) = vector_tile_cache.get(&key) else {
            if needed_seen.insert(key) {
                needed_tiles.push(aggregate_vector_tile_request(tile.z, tile.x, tile.y));
            }
            continue;
        };
        feature_ids.extend(payload.airspace_refs.iter().cloned());
    }

    let label_zoom = airspace_label_zoom(effective_zoom, config);
    for tile in
        visible_layer_tile_window("airspace-labels", label_zoom, viewport, width_px, height_px)
    {
        let key = aggregate_vector_tile_cache_key(tile.z, tile.x, tile.y);
        let Some(payload) = vector_tile_cache.get(&key) else {
            if needed_seen.insert(key) {
                needed_tiles.push(aggregate_vector_tile_request(tile.z, tile.x, tile.y));
            }
            continue;
        };
        on_label_tile(payload);
    }

    AirspaceInputScan {
        needed_tiles,
        feature_ids,
    }
}

#[derive(Debug, Clone)]
struct AirspaceLabelCandidate {
    rank: u32,
    label: AirspaceDisplayLabel,
}

fn airspace_label_candidate_is_better(
    candidate: &AirspaceLabelCandidate,
    current: &AirspaceLabelCandidate,
) -> bool {
    candidate.rank < current.rank
}

#[derive(Debug, Default)]
struct AirspaceDecorationBudget {
    used: usize,
    limit_hit: bool,
    missing_interior_side: usize,
    invalid_interior_side: usize,
}

#[derive(Debug, Clone, Copy)]
struct AirspaceDecorationScreenBounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AirspaceInteriorSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AirspaceInteriorSideError {
    Missing,
    Invalid,
}

struct AirspaceOverlayInput<'a> {
    config: &'a MapOverlayConfig,
    vector_tile_cache: &'a HashMap<String, VectorAggregateTilePayload>,
    feature_cache: &'a HashMap<String, AirspaceFeaturePayload>,
}

fn query_airspace_overlay(
    projection: &MapProjectionContext<'_>,
    input: AirspaceOverlayInput<'_>,
) -> AirspaceOverlayProjection {
    let metrics = projection.metrics;
    let viewport = &metrics.viewport;
    let width_px = metrics.width_px;
    let height_px = metrics.height_px;
    let center_world = projection.center_world;
    let scale = projection.scale;
    let point_display_scale = metrics.display_scale;
    let AirspaceOverlayInput {
        config,
        vector_tile_cache,
        feature_cache,
    } = input;
    let effective_zoom = effective_point_display_zoom(viewport, point_display_scale);
    if effective_zoom < AIRSPACE_MIN_DISPLAY_ZOOM || width_px <= 0.0 || height_px <= 0.0 {
        return AirspaceOverlayProjection {
            needed_tiles: Vec::new(),
            needed_features: Vec::new(),
            paths: Vec::new(),
            labels: Vec::new(),
            data_status_records: Vec::new(),
        };
    }

    let mut label_by_feature = HashMap::<String, AirspaceLabelCandidate>::new();
    let scan = scan_airspace_inputs(
        viewport,
        width_px,
        height_px,
        config,
        vector_tile_cache,
        point_display_scale,
        |payload| {
            for label in &payload.airspace_labels {
                let point = world_to_screen(
                    center_world,
                    scale,
                    width_px,
                    height_px,
                    LatLon {
                        lat: label.lat,
                        lon: label.lon,
                    },
                );
                if point.x < 0.0 || point.x > width_px || point.y < 0.0 || point.y > height_px {
                    continue;
                }
                let style_key = airspace_style_key(&label.style_hint);
                let Some(glyph) = airspace_limit_glyph_from_label(label.text.trim(), &style_key)
                else {
                    continue;
                };
                let candidate = AirspaceLabelCandidate {
                    rank: label.rank,
                    label: AirspaceDisplayLabel {
                        feature_id: label.feature_id.clone(),
                        glyph,
                        screen_x: point.x,
                        screen_y: point.y,
                    },
                };
                let entry = label_by_feature
                    .entry(candidate.label.feature_id.clone())
                    .or_insert_with(|| candidate.clone());
                if airspace_label_candidate_is_better(&candidate, entry) {
                    *entry = candidate;
                }
            }
        },
    );

    let mut needed_features = Vec::new();
    let mut paths = Vec::new();
    let mut limit_hit = false;
    let mut decoration_budget = AirspaceDecorationBudget::default();
    for feature_id in scan.feature_ids {
        if paths.len() >= AIRSPACE_DISPLAY_FEATURE_LIMIT {
            limit_hit = true;
            break;
        }
        let Some(feature) = feature_cache.get(&feature_id) else {
            needed_features.push(AirspaceFeatureRequest {
                path: airspace_feature_path(&feature_id),
                id: feature_id,
            });
            continue;
        };
        if !airspace_bbox_may_intersect_screen(
            feature.bbox,
            center_world,
            scale,
            width_px,
            height_px,
        ) {
            continue;
        }
        let projected = project_airspace_feature(
            feature,
            center_world,
            scale,
            width_px,
            height_px,
            &mut decoration_budget,
        );
        if !projected.paths.is_empty() {
            paths.push(projected);
        }
    }

    let mut labels = label_by_feature
        .into_values()
        .map(|candidate| candidate.label)
        .collect::<Vec<_>>();
    labels.sort_by(|left, right| {
        left.feature_id
            .cmp(&right.feature_id)
            .then_with(|| left.glyph.upper.cmp(&right.glyph.upper))
            .then_with(|| left.glyph.lower.cmp(&right.glyph.lower))
    });

    let mut data_status_records = Vec::new();
    if limit_hit {
        data_status_records.push(DataStatusRecord::new(
            AIRSPACE_DISPLAY_LIMIT_STATUS_ID,
            "AIRSPACE",
            Some("LIMIT".to_string()),
            UiStatusSeverity::Warning,
            true,
            format!(
                "display capped at {} visible airspace features",
                AIRSPACE_DISPLAY_FEATURE_LIMIT
            ),
        ));
    }
    if decoration_budget.limit_hit {
        data_status_records.push(DataStatusRecord::new(
            AIRSPACE_FEATHER_LIMIT_STATUS_ID,
            "AIRSPACE",
            Some("LIMIT".to_string()),
            UiStatusSeverity::Warning,
            true,
            format!(
                "display capped at {} airspace feather ticks",
                AIRSPACE_FEATHER_LIMIT
            ),
        ));
    }
    if decoration_budget.missing_interior_side > 0 || decoration_budget.invalid_interior_side > 0 {
        data_status_records.push(DataStatusRecord::new(
            "map_overlay:airspace_interior_side_contract",
            "AIRSPACE",
            Some("BAD DATA".to_string()),
            UiStatusSeverity::Warning,
            true,
            format!(
                "feathered airspace paths require interior_side; {} missing, {} invalid",
                decoration_budget.missing_interior_side, decoration_budget.invalid_interior_side
            ),
        ));
    }

    AirspaceOverlayProjection {
        needed_tiles: scan.needed_tiles,
        needed_features,
        paths,
        labels,
        data_status_records,
    }
}

fn tfr_display_style(style_key: &str) -> AirspaceDisplayStyle {
    let color_key = airspace_label_color_key(style_key);
    AirspaceDisplayStyle {
        fill_color_key: color_key.to_string(),
        fill_opacity: 0.08,
        strokes: vec![AirspaceDisplayStroke {
            color_key: color_key.to_string(),
            width_px: 2.0,
            dash_px: Vec::new(),
            line_cap: "round".to_string(),
        }],
    }
}

fn airspace_reference_zoom(display_zoom: f64, config: &MapOverlayConfig) -> u32 {
    display_zoom.floor().clamp(
        config.airspace_reference_tile_min_zoom as f64,
        config.airspace_reference_tile_max_zoom as f64,
    ) as u32
}

fn airspace_label_zoom(display_zoom: f64, config: &MapOverlayConfig) -> u32 {
    display_zoom.floor().clamp(
        config.airspace_label_tile_min_zoom as f64,
        config.airspace_label_tile_max_zoom as f64,
    ) as u32
}

pub fn airspace_ref_tile_key(z: u32, x: u32, y: u32) -> String {
    tile_key("airspace", z, x, y)
}

pub fn airspace_label_tile_key(z: u32, x: u32, y: u32) -> String {
    tile_key("airspace-labels", z, x, y)
}

pub fn airspace_feature_path(id: &str) -> String {
    format!("had/{}.json", id.replace(':', "/"))
}

fn project_airspace_feature(
    feature: &AirspaceFeaturePayload,
    center_world: WorldPoint,
    scale: f64,
    width_px: f64,
    height_px: f64,
    decoration_budget: &mut AirspaceDecorationBudget,
) -> AirspaceDisplayPath {
    let paths = feature
        .paths
        .iter()
        .filter_map(|path| {
            let points = lon_lat_points_for_airspace_path(path)
                .iter()
                .filter_map(|point| {
                    let lon = point[0];
                    let lat = point[1];
                    if !lon.is_finite() || !lat.is_finite() {
                        return None;
                    }
                    let screen = world_to_screen(
                        center_world,
                        scale,
                        width_px,
                        height_px,
                        LatLon { lat, lon },
                    );
                    Some(AirspaceScreenPoint {
                        x: round_screen_coordinate(screen.x),
                        y: round_screen_coordinate(screen.y),
                    })
                })
                .collect::<Vec<_>>();
            let points = simplify_projected_points(points);
            (points.len() >= 2).then_some(AirspaceDisplaySubpath {
                closed: path.closed,
                interior_side: path.interior_side.clone(),
                points,
            })
        })
        .collect::<Vec<_>>();
    let style_key = airspace_style_key(&feature.style_hint);
    AirspaceDisplayPath {
        id: feature.id.clone(),
        name: feature.name.clone(),
        style: airspace_display_style(&style_key),
        decorations: airspace_decorations(
            &style_key,
            &paths,
            decoration_budget,
            Some(airspace_decoration_screen_bounds(width_px, height_px)),
        ),
        style_key,
        paths,
    }
}

fn lon_lat_points_for_airspace_path(path: &AirspaceFeaturePath) -> Vec<[f64; 2]> {
    let segments = path
        .segments
        .iter()
        .map(|segment| match segment {
            AirspaceFeaturePathSegment::Line { to } => AirspaceSegment::Line { to: *to },
            AirspaceFeaturePathSegment::Arc {
                center,
                radius_ft: _,
                clockwise,
                to,
            } => AirspaceSegment::Arc {
                center: *center,
                clockwise: *clockwise,
                to: *to,
            },
        })
        .collect::<Vec<_>>();
    expand_airspace_path(path.start, &segments)
}

fn airspace_decorations(
    style_key: &str,
    paths: &[AirspaceDisplaySubpath],
    budget: &mut AirspaceDecorationBudget,
    screen_bounds: Option<AirspaceDecorationScreenBounds>,
) -> Vec<AirspaceDecorationPath> {
    let Some((color_key, width_px)) = airspace_feather_style(style_key) else {
        return Vec::new();
    };
    let mut feather_segments = Vec::new();
    for path in paths {
        if !path.closed || path.points.len() < 3 {
            continue;
        }
        let interior_side = match parse_airspace_interior_side(path.interior_side.as_deref()) {
            Ok(interior_side) => interior_side,
            Err(AirspaceInteriorSideError::Missing) => {
                budget.missing_interior_side += 1;
                continue;
            }
            Err(AirspaceInteriorSideError::Invalid) => {
                budget.invalid_interior_side += 1;
                continue;
            }
        };
        feather_segments.extend(airspace_feathers_for_path(
            path,
            interior_side,
            budget,
            screen_bounds,
        ));
        if budget.limit_hit {
            break;
        }
    }
    if feather_segments.is_empty() {
        return Vec::new();
    }
    vec![AirspaceDecorationPath {
        color_key,
        width_px,
        line_cap: "butt".to_string(),
        paths: Vec::new(),
        segments: feather_segments,
    }]
}

fn airspace_decoration_screen_bounds(
    width_px: f64,
    height_px: f64,
) -> AirspaceDecorationScreenBounds {
    AirspaceDecorationScreenBounds {
        min_x: -AIRSPACE_DECORATION_SCREEN_MARGIN_PX,
        min_y: -AIRSPACE_DECORATION_SCREEN_MARGIN_PX,
        max_x: width_px + AIRSPACE_DECORATION_SCREEN_MARGIN_PX,
        max_y: height_px + AIRSPACE_DECORATION_SCREEN_MARGIN_PX,
    }
}

impl AirspaceDecorationScreenBounds {
    fn intersects_segment(self, segment: [f64; 4]) -> bool {
        let min_x = segment[0].min(segment[2]);
        let max_x = segment[0].max(segment[2]);
        let min_y = segment[1].min(segment[3]);
        let max_y = segment[1].max(segment[3]);
        max_x >= self.min_x && min_x <= self.max_x && max_y >= self.min_y && min_y <= self.max_y
    }
}

fn parse_airspace_interior_side(
    value: Option<&str>,
) -> Result<AirspaceInteriorSide, AirspaceInteriorSideError> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) if value.eq_ignore_ascii_case("left") => Ok(AirspaceInteriorSide::Left),
        Some(value) if value.eq_ignore_ascii_case("right") => Ok(AirspaceInteriorSide::Right),
        Some(_) => Err(AirspaceInteriorSideError::Invalid),
        None => Err(AirspaceInteriorSideError::Missing),
    }
}

fn airspace_feather_style(style_key: &str) -> Option<(String, f64)> {
    match style_key {
        "moa" | "alert" => Some(("class_c_magenta".to_string(), 1.4)),
        "restricted" | "prohibited" | "warning" => Some(("class_b_d_blue".to_string(), 1.4)),
        _ => None,
    }
}

fn airspace_label_color_key(style_key: &str) -> &'static str {
    match style_key {
        "class_c" | "moa" | "alert" | "national_security" => "class_c_magenta",
        "tfr" | TFR_ACTIVE_STYLE_KEY => "tfr_active",
        TFR_UPCOMING_STYLE_KEY => "tfr_upcoming",
        _ => "class_b_d_blue",
    }
}

fn airspace_feathers_for_path(
    path: &AirspaceDisplaySubpath,
    interior_side: AirspaceInteriorSide,
    budget: &mut AirspaceDecorationBudget,
    screen_bounds: Option<AirspaceDecorationScreenBounds>,
) -> Vec<[f64; 4]> {
    const FEATHER_SPACING_PX: f64 = 8.0;
    const FEATHER_LENGTH_PX: f64 = 8.0;
    let signed_area = polygon_signed_area(&path.points);
    if signed_area.abs() < 1.0 {
        return Vec::new();
    }
    let side_sign = match interior_side {
        AirspaceInteriorSide::Left => -1.0,
        AirspaceInteriorSide::Right => 1.0,
    };
    let mut feathers = Vec::new();
    let mut path_distance = 0.0;
    let mut next_feather_distance = FEATHER_SPACING_PX * 0.5;
    for index in 0..path.points.len() {
        let start = &path.points[index];
        let end = &path.points[(index + 1) % path.points.len()];
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length = (dx * dx + dy * dy).sqrt();
        if length <= 0.0 {
            continue;
        }
        let nx = -dy / length * side_sign;
        let ny = dx / length * side_sign;
        let segment_end_distance = path_distance + length;
        while next_feather_distance < segment_end_distance {
            if budget.used >= AIRSPACE_FEATHER_LIMIT {
                budget.limit_hit = true;
                return feathers;
            }
            let t = (next_feather_distance - path_distance) / length;
            let base_x = start.x + dx * t;
            let base_y = start.y + dy * t;
            let segment = [
                round_screen_coordinate(base_x),
                round_screen_coordinate(base_y),
                round_screen_coordinate(base_x + nx * FEATHER_LENGTH_PX),
                round_screen_coordinate(base_y + ny * FEATHER_LENGTH_PX),
            ];
            if !screen_bounds.is_none_or(|bounds| bounds.intersects_segment(segment)) {
                next_feather_distance += FEATHER_SPACING_PX;
                continue;
            }
            feathers.push(segment);
            budget.used += 1;
            next_feather_distance += FEATHER_SPACING_PX;
        }
        path_distance = segment_end_distance;
    }
    feathers
}

fn polygon_signed_area(points: &[AirspaceScreenPoint]) -> f64 {
    let mut area = 0.0;
    for index in 0..points.len() {
        let start = &points[index];
        let end = &points[(index + 1) % points.len()];
        area += start.x * end.y - end.x * start.y;
    }
    area / 2.0
}

fn airspace_bbox_may_intersect_screen(
    bbox: [f64; 4],
    center_world: WorldPoint,
    scale: f64,
    width_px: f64,
    height_px: f64,
) -> bool {
    let corners = [
        LatLon {
            lat: bbox[1],
            lon: bbox[0],
        },
        LatLon {
            lat: bbox[3],
            lon: bbox[0],
        },
        LatLon {
            lat: bbox[1],
            lon: bbox[2],
        },
        LatLon {
            lat: bbox[3],
            lon: bbox[2],
        },
    ];
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for corner in corners {
        let point = world_to_screen(center_world, scale, width_px, height_px, corner);
        min_x = min_x.min(point.x);
        max_x = max_x.max(point.x);
        min_y = min_y.min(point.y);
        max_y = max_y.max(point.y);
    }
    let margin = 64.0;
    max_x >= -margin
        && min_x <= width_px + margin
        && max_y >= -margin
        && min_y <= height_px + margin
}

fn simplify_projected_points(points: Vec<AirspaceScreenPoint>) -> Vec<AirspaceScreenPoint> {
    let mut simplified: Vec<AirspaceScreenPoint> = Vec::with_capacity(points.len());
    for point in points {
        let keep = simplified.last().is_none_or(|last| {
            (point.x - last.x).abs() >= 0.35 || (point.y - last.y).abs() >= 0.35
        });
        if keep {
            simplified.push(point);
        }
    }
    simplified
}

fn round_screen_coordinate(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn airspace_style_key(style_hint: &str) -> String {
    match style_hint.to_ascii_lowercase().as_str() {
        "class_b" => "class_b",
        "class_c" => "class_c",
        "class_d" => "class_d",
        "restricted" => "restricted",
        "prohibited" => "prohibited",
        "moa" => "moa",
        "warning" => "warning",
        "alert" => "alert",
        "national_security" => "national_security",
        _ => "airspace",
    }
    .to_string()
}

fn airspace_display_style(style_key: &str) -> AirspaceDisplayStyle {
    match style_key {
        "class_b" => AirspaceDisplayStyle {
            fill_color_key: "class_b_d_blue".to_string(),
            fill_opacity: 0.035,
            strokes: vec![AirspaceDisplayStroke {
                color_key: "class_b_d_blue".to_string(),
                width_px: 4.8,
                dash_px: Vec::new(),
                line_cap: "round".to_string(),
            }],
        },
        "class_c" => AirspaceDisplayStyle {
            fill_color_key: "class_c_magenta".to_string(),
            fill_opacity: 0.03,
            strokes: vec![AirspaceDisplayStroke {
                color_key: "class_c_magenta".to_string(),
                width_px: 4.0,
                dash_px: Vec::new(),
                line_cap: "round".to_string(),
            }],
        },
        "class_d" => AirspaceDisplayStyle {
            fill_color_key: "class_b_d_blue".to_string(),
            fill_opacity: 0.02,
            strokes: vec![AirspaceDisplayStroke {
                color_key: "class_b_d_blue".to_string(),
                width_px: 4.0,
                dash_px: vec![8.0, 8.0],
                line_cap: "butt".to_string(),
            }],
        },
        "restricted" | "prohibited" => AirspaceDisplayStyle {
            fill_color_key: "class_b_d_blue".to_string(),
            fill_opacity: 0.025,
            strokes: vec![AirspaceDisplayStroke {
                color_key: "class_b_d_blue".to_string(),
                width_px: 1.4,
                dash_px: Vec::new(),
                line_cap: "butt".to_string(),
            }],
        },
        "moa" | "alert" => AirspaceDisplayStyle {
            fill_color_key: "class_c_magenta".to_string(),
            fill_opacity: 0.018,
            strokes: vec![AirspaceDisplayStroke {
                color_key: "class_c_magenta".to_string(),
                width_px: 1.4,
                dash_px: Vec::new(),
                line_cap: "butt".to_string(),
            }],
        },
        "warning" => AirspaceDisplayStyle {
            fill_color_key: "class_b_d_blue".to_string(),
            fill_opacity: 0.025,
            strokes: vec![AirspaceDisplayStroke {
                color_key: "class_b_d_blue".to_string(),
                width_px: 1.4,
                dash_px: Vec::new(),
                line_cap: "butt".to_string(),
            }],
        },
        "national_security" => AirspaceDisplayStyle {
            fill_color_key: "class_c_magenta".to_string(),
            fill_opacity: 0.018,
            strokes: vec![AirspaceDisplayStroke {
                color_key: "class_c_magenta".to_string(),
                width_px: 3.6,
                dash_px: vec![6.0, 4.0],
                line_cap: "butt".to_string(),
            }],
        },
        _ => AirspaceDisplayStyle {
            fill_color_key: "class_b_d_blue".to_string(),
            fill_opacity: 0.018,
            strokes: vec![AirspaceDisplayStroke {
                color_key: "class_b_d_blue".to_string(),
                width_px: 3.2,
                dash_px: Vec::new(),
                line_cap: "round".to_string(),
            }],
        },
    }
}

pub fn point_vector_record_to_symbol_feature(
    record: &PointVectorRecord,
    ownship_altitude_ft: Option<f64>,
) -> Option<NavSymbolFeature> {
    should_display_record(record)
        .then(|| point_vector_record_to_symbol_feature_unfiltered(record, ownship_altitude_ft))
        .flatten()
}

pub fn point_vector_record_to_symbol_feature_unfiltered(
    record: &PointVectorRecord,
    ownship_altitude_ft: Option<f64>,
) -> Option<NavSymbolFeature> {
    let mut style_class = record.style_class.clone();
    let mut label = display_label(record);
    let symbol_kind = point_symbol_kind(&record.style_class, &record.kind);
    let mut obstacle_variant = None;
    let mut obstacle_tone = None;
    if record.style_class == "obstacle" {
        let obstacle = record.obstacle.as_ref()?;
        let altitude_ft = obstacle.top_msl_ft;
        if let Some(ownship_altitude_ft) = ownship_altitude_ft.filter(|value| value.is_finite()) {
            let delta_ft = altitude_ft - ownship_altitude_ft;
            if delta_ft < -OBSTACLE_BELOW_OWNERSHIP_HIDE_FT {
                return None;
            }
            let tone = if delta_ft >= -OBSTACLE_DANGER_LOWER_FT {
                "danger"
            } else if delta_ft >= -OBSTACLE_CAUTION_LOWER_FT {
                "caution"
            } else {
                "muted"
            };
            style_class = format!("obstacle-{tone}");
            obstacle_tone = Some(tone.to_string());
        } else {
            style_class = "obstacle-caution".to_string();
            obstacle_tone = Some("caution".to_string());
        }
        obstacle_variant = Some(if obstacle.is_tall {
            "tall".to_string()
        } else {
            "short".to_string()
        });
        label.clear();
    }
    Some(NavSymbolFeature {
        kind: record.kind.clone(),
        label,
        symbol_kind,
        style_class,
        obstacle_variant,
        obstacle_tone,
        towered: record.towered.unwrap_or(false),
        fuel_available: record.fuel_available.unwrap_or(false),
        has_paved_runway: record.has_paved_runway,
        heliport: record.heliport,
        has_water_runway: record.has_water_runway,
        runway_length_ratio: runway_length_ratio(record.longest_runway_length_ft),
        longest_runway_heading_true_deg: record.longest_runway_heading_true_deg,
        elevation_msl_ft: record.elevation_msl_ft,
    })
}

fn point_symbol_kind(style_class: &str, kind: &str) -> String {
    let style = style_class.to_ascii_lowercase();
    let kind = kind.to_ascii_lowercase();
    if style == "airport" || kind == "airport" {
        "airport".to_string()
    } else if style == "weather_camera" || kind == "weather camera" {
        "weather_camera".to_string()
    } else if style == "nav" || kind.contains("vor") {
        "nav".to_string()
    } else if style.starts_with("obstacle") || kind == "obs" || kind == "obstacle" {
        "obstacle".to_string()
    } else {
        "fix".to_string()
    }
}

pub fn tile_key(layer: &str, z: u32, x: u32, y: u32) -> String {
    format!("{layer}:{z}/{x}/{y}")
}

pub fn aggregate_vector_tile_cache_key(z: u32, x: u32, y: u32) -> String {
    tile_key("vector", z, x, y)
}

pub fn aggregate_vector_tile_request(z: u32, x: u32, y: u32) -> VectorTileRequest {
    VectorTileRequest {
        layer: "vector".to_string(),
        z,
        x,
        y,
    }
}

fn vector_tile_point_records<'a>(
    tile: &'a VectorAggregateTilePayload,
    layer: &str,
) -> &'a [PointVectorRecord] {
    match layer {
        "airport" => &tile.airports,
        "fix" => &tile.fixes,
        "nav" => &tile.navaids,
        _ => &[],
    }
}

fn merge_aggregate_vector_tile_requests(
    first: Vec<VectorTileRequest>,
    second: Vec<VectorTileRequest>,
) -> Vec<VectorTileRequest> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for request in first.into_iter().chain(second) {
        let key = aggregate_vector_tile_cache_key(request.z, request.x, request.y);
        if seen.insert(key) {
            out.push(aggregate_vector_tile_request(
                request.z, request.x, request.y,
            ));
        }
    }
    out
}

fn tile_counts_by_zoom(tiles: &[VectorTileRequest]) -> BTreeMap<u32, usize> {
    let mut counts = BTreeMap::new();
    for tile in tiles {
        *counts.entry(tile.z).or_insert(0) += 1;
    }
    counts
}

fn airspace_display_path_point_count(paths: &[AirspaceDisplayPath]) -> usize {
    paths
        .iter()
        .map(|path| {
            path.paths
                .iter()
                .map(|subpath| subpath.points.len())
                .sum::<usize>()
        })
        .sum()
}

fn airspace_display_path_decoration_point_count(paths: &[AirspaceDisplayPath]) -> usize {
    paths
        .iter()
        .map(|path| {
            path.decorations
                .iter()
                .flat_map(|decoration| &decoration.paths)
                .map(|subpath| subpath.points.len())
                .sum::<usize>()
                + path
                    .decorations
                    .iter()
                    .map(|decoration| decoration.segments.len() * 2)
                    .sum::<usize>()
        })
        .sum()
}

fn airspace_display_path_decoration_segment_count(paths: &[AirspaceDisplayPath]) -> usize {
    paths
        .iter()
        .map(|path| {
            path.decorations
                .iter()
                .map(|decoration| decoration.segments.len())
                .sum::<usize>()
        })
        .sum()
}

fn offline_region_point_count(regions: &[OfflineRegionDisplay]) -> usize {
    regions.iter().map(|region| region.points.len()).sum()
}

fn overlay_elapsed_ms(started_at: Option<f64>) -> u64 {
    let Some(started_at) = started_at else {
        return 0;
    };
    let Some(now_ms) = core_clock_ms() else {
        return 0;
    };
    (now_ms - started_at).max(0.0).round() as u64
}

pub fn chart_ident_label_for_nav_ref_symbol(nav_ref: &NavRef, symbol: &NavSymbolFeature) -> String {
    let airport_ident = match nav_ref {
        NavRef::Airport(ident) => Some(ident.as_str()),
        _ => None,
    };
    let nav_ident = match nav_ref {
        NavRef::Navaid(ident)
        | NavRef::ArincNavaid {
            identifier: ident, ..
        }
        | NavRef::TerminalNavaid {
            identifier: ident, ..
        } => Some(ident.as_str()),
        _ => None,
    };
    chart_ident_label(
        &symbol.kind,
        &symbol.style_class,
        &symbol.label,
        airport_ident,
        nav_ident,
    )
}

fn display_label(record: &PointVectorRecord) -> String {
    chart_ident_label(
        &record.kind,
        &record.style_class,
        &record.label,
        record
            .id
            .strip_prefix("airports:")
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        record
            .id
            .strip_prefix("nav:")
            .map(|tail| tail.split(':').next().unwrap_or(tail).trim())
            .filter(|value| !value.is_empty()),
    )
}

fn chart_ident_label(
    kind: &str,
    style_class: &str,
    label: &str,
    airport_ident: Option<&str>,
    nav_ident: Option<&str>,
) -> String {
    if style_class == "airport" || kind.eq_ignore_ascii_case("airport") {
        if let Some(ident) = airport_ident {
            return ident.trim().to_uppercase();
        }
    }
    if style_class == "nav" && is_vor_family_kind(kind) {
        if let Some(ident) = nav_ident {
            return ident.to_uppercase();
        }
    }
    label.trim().to_uppercase()
}

fn is_vor_family_kind(kind: &str) -> bool {
    matches!(
        kind.to_ascii_lowercase().as_str(),
        "vor" | "vor/dme" | "vortac"
    )
}

fn should_display_record(record: &PointVectorRecord) -> bool {
    if record.style_class == "weather_camera"
        && record
            .weather_camera
            .as_ref()
            .is_some_and(|camera| camera.active == Some(false))
    {
        return false;
    }
    if record.style_class == "airport"
        || record.kind.eq_ignore_ascii_case("airport")
        || record.id.starts_with("airports:")
    {
        if record.private_use.unwrap_or(false) {
            return false;
        }
        if record.heliport.unwrap_or(false) || record.kind.eq_ignore_ascii_case("heliport") {
            return false;
        }
        if record.has_water_runway.unwrap_or(false) {
            return false;
        }
    }
    true
}

fn runway_length_ratio(longest_runway_length_ft: Option<f64>) -> f64 {
    (longest_runway_length_ft.unwrap_or(0.0) / 5000.0).clamp(0.0, 1.0)
}

#[derive(Clone, Copy)]
struct WorldPoint {
    x: f64,
    y: f64,
}

#[derive(Clone, Copy)]
struct TileBounds {
    south: f64,
    north: f64,
    west: f64,
    east: f64,
}

fn lat_lon_to_world(position: LatLon) -> WorldPoint {
    let clamped_lat = position.lat.clamp(-MAX_LATITUDE, MAX_LATITUDE);
    WorldPoint {
        x: ((position.lon + 180.0) / 360.0) * WORLD_SIZE,
        y: ((1.0 - clamped_lat.to_radians().tan().asinh() / std::f64::consts::PI) / 2.0)
            * WORLD_SIZE,
    }
}

fn world_to_lat_lon(point: WorldPoint) -> LatLon {
    let lon = (point.x / WORLD_SIZE) * 360.0 - 180.0;
    let n = std::f64::consts::PI - (2.0 * std::f64::consts::PI * point.y) / WORLD_SIZE;
    let lat = n.sinh().atan().to_degrees();
    LatLon { lat, lon }
}

fn world_to_screen(
    center_world: WorldPoint,
    scale: f64,
    width_px: f64,
    height_px: f64,
    position: LatLon,
) -> WorldPoint {
    world_to_screen_projected(
        center_world,
        scale,
        width_px,
        height_px,
        position,
        WorldXProjection::NearestWrappedCopy,
    )
}

fn world_to_screen_with_x_offset(
    center_world: WorldPoint,
    scale: f64,
    width_px: f64,
    height_px: f64,
    position: LatLon,
    world_x_offset: f64,
) -> WorldPoint {
    world_to_screen_projected(
        center_world,
        scale,
        width_px,
        height_px,
        position,
        WorldXProjection::DisplayCopyOffset(world_x_offset),
    )
}

fn world_to_screen_projected(
    center_world: WorldPoint,
    scale: f64,
    width_px: f64,
    height_px: f64,
    position: LatLon,
    projection: WorldXProjection,
) -> WorldPoint {
    let mut world = lat_lon_to_world(position);
    match projection {
        WorldXProjection::NearestWrappedCopy => {
            world = unwrap_world_x_near_center(world, center_world);
        }
        WorldXProjection::DisplayCopyOffset(world_x_offset) => {
            world.x += world_x_offset;
        }
    }
    projected_world_to_screen(center_world, scale, width_px, height_px, world)
}

fn projected_world_to_screen(
    center_world: WorldPoint,
    scale: f64,
    width_px: f64,
    height_px: f64,
    world: WorldPoint,
) -> WorldPoint {
    WorldPoint {
        x: (world.x - center_world.x) * scale + width_px / 2.0,
        y: (world.y - center_world.y) * scale + height_px / 2.0,
    }
}

fn destination_point(origin: LatLon, bearing_deg: f64, distance_nm: f64) -> LatLon {
    const EARTH_RADIUS_NM: f64 = 3440.065;
    let angular_distance = distance_nm / EARTH_RADIUS_NM;
    let bearing = bearing_deg.to_radians();
    let lat1 = origin.lat.to_radians();
    let lon1 = origin.lon.to_radians();
    let sin_lat1 = lat1.sin();
    let cos_lat1 = lat1.cos();
    let sin_ad = angular_distance.sin();
    let cos_ad = angular_distance.cos();
    let lat2 = (sin_lat1 * cos_ad + cos_lat1 * sin_ad * bearing.cos()).asin();
    let lon2 = lon1 + (bearing.sin() * sin_ad * cos_lat1).atan2(cos_ad - sin_lat1 * lat2.sin());
    LatLon {
        lat: lat2.to_degrees(),
        lon: ((lon2.to_degrees() + 540.0) % 360.0) - 180.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RouteComponent;

    fn test_airspace_vertical(upper: &str, lower: &str) -> AirspaceVerticalPayload {
        AirspaceVerticalPayload {
            upper: AirspaceLimitPayload {
                display: upper.to_string(),
            },
            lower: AirspaceLimitPayload {
                display: lower.to_string(),
            },
        }
    }

    fn test_selection_item(id: &str, nav_ref: Option<NavRef>) -> MapSelectionItem {
        MapSelectionItem {
            id: id.to_string(),
            label: id.to_string(),
            sublabel: String::new(),
            description: None,
            distance: None,
            secondary_description: None,
            position: None,
            elevation_msl_ft: None,
            detail_text: None,
            highlight: MapSelectionHighlight::Spot { lat: 0.0, lon: 0.0 },
            nav_ref,
            symbol_feature: None,
            metar_feature: None,
            weather_detail: None,
            automatic_action_uid: None,
            pirep_feature: None,
            airspace_icon: None,
            actions: Vec::new(),
        }
    }

    #[test]
    fn selected_map_selection_item_id_matches_nav_ref_in_core() {
        let spot = LatLon {
            lat: 47.3,
            lon: -122.9,
        };
        let result = MapSelectionQueryResult {
            click_lat: 0.0,
            click_lon: 0.0,
            initial_selected_item_id: None,
            categories: vec![MapSelectionCategory {
                id: "airport".to_string(),
                label: "Airport".to_string(),
                items: vec![
                    test_selection_item("khwd", Some(NavRef::Airport("KHWD".to_string()))),
                    test_selection_item("koak", Some(NavRef::Airport("KOAK".to_string()))),
                    test_selection_item("spot", Some(NavRef::Spot(spot))),
                ],
            }],
        };

        assert_eq!(
            selected_map_selection_item_id_for_nav_ref(
                &result,
                &NavRef::Airport("KHWD".to_string()),
            )
            .as_deref(),
            Some("khwd"),
        );
        assert_eq!(
            selected_map_selection_item_id_for_nav_ref(&result, &NavRef::Spot(spot)).as_deref(),
            Some("spot"),
        );
    }

    #[test]
    fn airspace_feature_payload_accepts_anonymous_feature_without_ident() {
        let payload: AirspaceFeaturePayload = serde_json::from_str(
            r#"{
                "schema_version": 1,
                "id": "airspace:data_2604:d:anon:class_d:4830",
                "kind": "airspace",
                "name": "Anonymous Class D",
                "airspace_class": "D",
                "style_hint": "class_d",
                "vertical": {
                    "upper": { "display": "25" },
                    "lower": { "display": "SFC" }
                },
                "bbox": [-123.0, 45.0, -122.0, 46.0],
                "paths": []
            }"#,
        )
        .expect("anonymous airspace features may omit ident");

        assert_eq!(payload.ident, "");
        assert_eq!(airspace_selection_label(&payload), "Anonymous Class D");
    }

    fn test_airspace_path(
        closed: bool,
        interior_side: Option<String>,
        points: Vec<[f64; 2]>,
    ) -> AirspaceFeaturePath {
        let start = points.first().copied().unwrap_or([0.0, 0.0]);
        let segments = points
            .iter()
            .skip(1)
            .map(|point| AirspaceFeaturePathSegment::Line { to: *point })
            .collect();
        AirspaceFeaturePath {
            role: "boundary".to_string(),
            closed,
            interior_side,
            start,
            segments,
        }
    }

    fn test_point_layer_config() -> PointTileLayerConfig {
        PointTileLayerConfig {
            min_zoom: 0,
            max_zoom: 9,
            available_zooms: (0..=9).collect(),
            tile_path_template: None,
        }
    }

    fn test_map_overlay_config() -> MapOverlayConfig {
        MapOverlayConfig {
            airspace_reference_tile_min_zoom: 0,
            airspace_reference_tile_max_zoom: 12,
            airspace_label_tile_min_zoom: 0,
            airspace_label_tile_max_zoom: 12,
            airport_layer: test_point_layer_config(),
            fix_layer: test_point_layer_config(),
            nav_layer: test_point_layer_config(),
            obstacle_layer: None,
            metar_layer: Some(PointTileLayerConfig {
                min_zoom: 5,
                max_zoom: 7,
                available_zooms: vec![5, 6, 7],
                tile_path_template: Some("points/metars/{z}/{x}/{y}.json".to_string()),
            }),
        }
    }

    fn captured_samsung_surface_metrics() -> (MapSurfaceMetrics, MapSurfaceMetrics) {
        let display_scale = 1.75_f64;
        let android = MapSurfaceMetrics::new(
            MapViewport {
                center: LatLon {
                    lat: 39.824_266_345_443_58,
                    lon: -92.966_592_939_012_33,
                },
                zoom: 6.332_551_672_530_909,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            1440.0,
            2167.0,
            display_scale,
        );
        let web = MapSurfaceMetrics::new(
            MapViewport {
                zoom: android.effective_display_zoom(),
                ..android.viewport
            },
            android.width_px / display_scale,
            android.height_px / display_scale,
            1.0,
        );
        (web, android)
    }

    fn metar_tile_for_position(zoom: u32, position: LatLon, station_id: &str) -> MetarTilePayload {
        let world = lat_lon_to_world(position);
        let tiles_at_zoom = 2_u32.pow(zoom);
        let tile_world_size = WORLD_SIZE / f64::from(tiles_at_zoom);
        let x = (world.x / tile_world_size)
            .floor()
            .clamp(0.0, f64::from(tiles_at_zoom - 1)) as u32;
        let y = (world.y / tile_world_size)
            .floor()
            .clamp(0.0, f64::from(tiles_at_zoom - 1)) as u32;
        MetarTilePayload {
            schema_version: 1,
            layer: "metars".to_string(),
            z: zoom,
            x,
            y,
            records: vec![MetarTileRecord {
                kind: "metar".to_string(),
                id: station_id.to_string(),
            }],
        }
    }

    fn test_metar_record(station_id: &str, position: LatLon) -> MetarRecord {
        MetarRecord {
            raw_text: format!("METAR {station_id} 010000Z 00000KT 10SM SCT020 10/08 A3000"),
            observed_at_utc: Some("2026-07-26T16:44:00Z".to_string()),
            station_id: station_id.to_string(),
            flight_category: Some("VFR".to_string()),
            clouds: None,
            longitude: position.lon,
            latitude: position.lat,
        }
    }

    fn query_map_overlay(
        viewport: &MapViewport,
        width_px: f64,
        height_px: f64,
        point_tile_cache: &HashMap<String, PointTilePayload>,
        airspace_ref_tile_cache: &HashMap<String, AirspaceReferenceTilePayload>,
        airspace_feature_cache: &HashMap<String, AirspaceFeaturePayload>,
        airspace_label_tile_cache: &HashMap<String, AirspaceLabelTilePayload>,
    ) -> MapOverlayQueryResult {
        let vector_tile_cache = aggregate_test_vector_tiles(
            point_tile_cache,
            airspace_ref_tile_cache,
            airspace_label_tile_cache,
        );
        let obstacle_tile_cache = obstacle_test_tiles(point_tile_cache);
        let config = test_map_overlay_config();
        let metar_tile_cache = HashMap::new();
        super::query_map_overlay(
            viewport,
            width_px,
            height_px,
            MapOverlayQuery {
                display_vectors: true,
                ..MapOverlayQuery::new(
                    &config,
                    &vector_tile_cache,
                    &obstacle_tile_cache,
                    &metar_tile_cache,
                    airspace_feature_cache,
                )
            },
        )
    }

    fn query_map_overlay_with_point_display_scale(
        metrics: MapSurfaceMetrics,
        point_tile_cache: &HashMap<String, PointTilePayload>,
        airspace_ref_tile_cache: &HashMap<String, AirspaceReferenceTilePayload>,
        airspace_feature_cache: &HashMap<String, AirspaceFeaturePayload>,
        airspace_label_tile_cache: &HashMap<String, AirspaceLabelTilePayload>,
    ) -> MapOverlayQueryResult {
        let vector_tile_cache = aggregate_test_vector_tiles(
            point_tile_cache,
            airspace_ref_tile_cache,
            airspace_label_tile_cache,
        );
        let obstacle_tile_cache = obstacle_test_tiles(point_tile_cache);
        let config = test_map_overlay_config();
        let metar_tile_cache = HashMap::new();
        super::query_map_overlay_for_surface(
            &metrics,
            MapOverlayQuery {
                display_vectors: true,
                ..MapOverlayQuery::new(
                    &config,
                    &vector_tile_cache,
                    &obstacle_tile_cache,
                    &metar_tile_cache,
                    airspace_feature_cache,
                )
            },
        )
    }

    fn aggregate_test_vector_tiles(
        point_tile_cache: &HashMap<String, PointTilePayload>,
        airspace_ref_tile_cache: &HashMap<String, AirspaceReferenceTilePayload>,
        airspace_label_tile_cache: &HashMap<String, AirspaceLabelTilePayload>,
    ) -> HashMap<String, VectorAggregateTilePayload> {
        let mut out = HashMap::new();
        for tile in point_tile_cache.values() {
            let aggregate = out
                .entry(aggregate_vector_tile_cache_key(tile.z, tile.x, tile.y))
                .or_insert_with(|| empty_test_vector_tile(tile.z, tile.x, tile.y));
            match tile.layer.as_str() {
                "airport" => aggregate.airports = tile.records.clone(),
                "fix" => aggregate.fixes = tile.records.clone(),
                "nav" => aggregate.navaids = tile.records.clone(),
                _ => {}
            }
        }
        for tile in airspace_ref_tile_cache.values() {
            out.entry(aggregate_vector_tile_cache_key(tile.z, tile.x, tile.y))
                .or_insert_with(|| empty_test_vector_tile(tile.z, tile.x, tile.y))
                .airspace_refs = tile.refs.clone();
        }
        for tile in airspace_label_tile_cache.values() {
            out.entry(aggregate_vector_tile_cache_key(tile.z, tile.x, tile.y))
                .or_insert_with(|| empty_test_vector_tile(tile.z, tile.x, tile.y))
                .airspace_labels = tile.labels.clone();
        }
        out
    }

    fn empty_test_vector_tile(z: u32, x: u32, y: u32) -> VectorAggregateTilePayload {
        VectorAggregateTilePayload {
            schema_version: 1,
            z,
            x,
            y,
            airports: Vec::new(),
            fixes: Vec::new(),
            navaids: Vec::new(),
            airspace_refs: Vec::new(),
            airspace_labels: Vec::new(),
        }
    }

    fn obstacle_test_tiles(
        point_tile_cache: &HashMap<String, PointTilePayload>,
    ) -> HashMap<String, PointTilePayload> {
        point_tile_cache
            .iter()
            .filter(|(_, tile)| tile.layer == "obstacle")
            .map(|(key, tile)| (key.clone(), tile.clone()))
            .collect()
    }

    fn test_point_record(id: String, kind: &str, style_class: &str) -> PointVectorRecord {
        PointVectorRecord {
            id,
            kind: kind.to_string(),
            lat: 47.36,
            lon: -121.98,
            label: kind.to_ascii_uppercase(),
            location_label: None,
            style_class: style_class.to_string(),
            towered: None,
            fuel_available: None,
            public_use: None,
            private_use: None,
            has_paved_runway: None,
            heliport: None,
            has_water_runway: None,
            longest_runway_length_ft: None,
            longest_runway_heading_true_deg: None,
            elevation_msl_ft: None,
            obstacle: None,
            weather_camera: None,
        }
    }

    fn test_colocated_airport_and_weather_camera(position: LatLon) -> Vec<PointVectorRecord> {
        let mut airport = test_point_record("airports:KSEA".to_string(), "airport", "airport");
        airport.lat = position.lat;
        airport.lon = position.lon;
        airport.label = "SEATTLE-TACOMA INTL".to_string();
        airport.towered = Some(true);
        airport.has_paved_runway = Some(true);
        airport.longest_runway_length_ft = Some(11_901.0);
        airport.longest_runway_heading_true_deg = Some(160.0);

        let mut camera = test_point_record(
            "weather-camera:150".to_string(),
            "weather camera",
            "weather_camera",
        );
        camera.lat = position.lat;
        camera.lon = position.lon;
        camera.label = "KSEA".to_string();
        camera.weather_camera = Some(WeatherCameraPointSemantics {
            site_id: "150".to_string(),
            site_name: "Seattle-Tacoma International".to_string(),
            site_identifier: Some("KSEA".to_string()),
            icao: Some("KSEA".to_string()),
            page_url: "https://weathercams.faa.gov/cameras/cameraSite/150/summary".to_string(),
            operated_by: Some("FAA".to_string()),
            attribution: None,
            active: Some(true),
            in_maintenance: Some(false),
            third_party: Some(false),
        });
        vec![airport, camera]
    }

    #[test]
    fn colocated_weather_camera_is_an_unlabelled_badge_below_its_airport() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.4489,
                lon: -122.3094,
            },
            zoom: 10.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let airport_tile =
            visible_point_tile_window(&test_map_overlay_config(), &viewport, 1200.0, 900.0)
                .into_iter()
                .find(|tile| tile.layer == "airport")
                .expect("expected airport tile");
        let mut cache = HashMap::new();
        cache.insert(
            tile_key(
                &airport_tile.layer,
                airport_tile.z,
                airport_tile.x,
                airport_tile.y,
            ),
            PointTilePayload {
                schema_version: 1,
                layer: airport_tile.layer,
                z: airport_tile.z,
                x: airport_tile.x,
                y: airport_tile.y,
                records: test_colocated_airport_and_weather_camera(viewport.center),
            },
        );

        let result = query_map_overlay(
            &viewport,
            1200.0,
            900.0,
            &cache,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );

        assert_eq!(
            result
                .visible_features
                .iter()
                .map(|feature| feature.id.as_str())
                .collect::<Vec<_>>(),
            vec!["weather-camera:150", "airports:KSEA"],
            "later features paint above earlier features on both platforms",
        );
        let camera = result
            .visible_features
            .iter()
            .find(|feature| feature.id == "weather-camera:150")
            .expect("weather camera feature");
        assert_eq!(camera.label, "");
        assert!((camera.screen_x - 614.0).abs() < 1e-6);
        assert!((camera.screen_y - 464.0).abs() < 1e-6);
    }

    #[test]
    fn ordinary_vector_paint_order_follows_operational_priority() {
        let mut features = vec![
            test_visible_feature("airports:KSEA", "airport", "airport", "KSEA", 0.0, 0.0),
            test_visible_feature("nav:SEA:VOR", "VOR", "nav", "SEA", 0.0, 0.0),
            test_visible_feature("fix:HAROB", "fix", "fix", "HAROB", 0.0, 0.0),
            test_visible_feature(
                "weather-camera:150",
                "weather camera",
                "weather_camera",
                "KSEA",
                0.0,
                0.0,
            ),
            test_visible_feature("obstacle:danger", "obstacle", "obstacle", "", 0.0, 0.0),
            test_visible_feature("obstacle:caution", "obstacle", "obstacle", "", 0.0, 0.0),
            test_visible_feature("obstacle:muted", "obstacle", "obstacle", "", 0.0, 0.0),
        ];
        features[4].obstacle_tone = Some("danger".to_string());
        features[5].obstacle_tone = Some("caution".to_string());
        features[6].obstacle_tone = Some("muted".to_string());

        sort_visible_point_features_for_paint(&mut features);

        assert_eq!(
            features
                .iter()
                .map(|feature| feature.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "weather-camera:150",
                "obstacle:muted",
                "fix:HAROB",
                "nav:SEA:VOR",
                "airports:KSEA",
                "obstacle:caution",
                "obstacle:danger",
            ],
        );
    }

    #[test]
    fn map_selection_hits_the_displaced_weather_camera_badge() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.4489,
                lon: -122.3094,
            },
            zoom: 10.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let airport_tile =
            visible_point_tile_window(&test_map_overlay_config(), &viewport, 1200.0, 900.0)
                .into_iter()
                .find(|tile| tile.layer == "airport")
                .expect("expected airport tile");
        let mut point_tiles = HashMap::new();
        point_tiles.insert(
            tile_key(
                &airport_tile.layer,
                airport_tile.z,
                airport_tile.x,
                airport_tile.y,
            ),
            PointTilePayload {
                schema_version: 1,
                layer: airport_tile.layer,
                z: airport_tile.z,
                x: airport_tile.x,
                y: airport_tile.y,
                records: test_colocated_airport_and_weather_camera(viewport.center),
            },
        );
        let vector_tiles =
            aggregate_test_vector_tiles(&point_tiles, &HashMap::new(), &HashMap::new());
        let config = test_map_overlay_config();
        let metar_tiles = HashMap::new();
        let airspaces = HashMap::new();
        let aliases = WeatherStationAirportAliases::default();
        let mut availability = |_: &str| AirportPlateAvailability::default();
        let center_world = lat_lon_to_world(viewport.center);
        let scale = 2.0_f64.powf(viewport.zoom);
        let badge_click = world_to_lat_lon(WorldPoint {
            x: center_world.x + 14.0 / scale,
            y: center_world.y + 14.0 / scale,
        });

        let selection = query_map_selection(
            &viewport,
            1200.0,
            900.0,
            MapSelectionQuery::new(
                &config,
                badge_click,
                &vector_tiles,
                &metar_tiles,
                &airspaces,
                &aliases,
                &mut availability,
            ),
        );

        assert_eq!(
            selection.initial_selected_item_id.as_deref(),
            Some("weather-camera:150"),
        );
    }

    #[test]
    fn suppresses_fix_tiles_below_threshold_zoom_but_keeps_airports_and_nav() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.36,
                lon: -121.98,
            },
            zoom: 8.9,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let tiles = visible_point_tile_window(&test_map_overlay_config(), &viewport, 1200.0, 900.0);
        assert!(tiles.iter().any(|tile| tile.layer == "airport"));
        assert!(!tiles.iter().any(|tile| tile.layer == "fix"));
        assert!(tiles.iter().any(|tile| tile.layer == "nav"));
    }

    #[test]
    fn point_tile_zoom_tracks_density_normalized_display_zoom() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.36,
                lon: -121.98,
            },
            zoom: 9.6,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };

        let unscaled =
            visible_point_tile_window(&test_map_overlay_config(), &viewport, 1200.0, 900.0);
        assert!(unscaled
            .iter()
            .any(|tile| tile.layer == "airport" && tile.z == 9));

        let density_scaled = visible_point_tile_window_with_display_scale(
            &test_map_overlay_config(),
            &viewport,
            1200.0,
            900.0,
            3.0,
        );
        assert!(density_scaled
            .iter()
            .any(|tile| tile.layer == "airport" && tile.z == 8));
        assert!(!density_scaled
            .iter()
            .any(|tile| tile.layer == "airport" && tile.z == 9));
    }

    #[test]
    fn airspace_tile_zoom_tracks_density_normalized_display_zoom() {
        let viewport = MapViewport {
            center: LatLon { lat: 0.0, lon: 0.0 },
            zoom: 10.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let feature = AirspaceFeaturePayload {
            schema_version: 1,
            id: "airspace:test:scaled".to_string(),
            kind: "airspace".to_string(),
            name: "SCALED AIRSPACE".to_string(),
            ident: "SCALE".to_string(),
            airspace_class: "B".to_string(),
            style_hint: "class_b".to_string(),
            vertical: test_airspace_vertical("40", "23"),
            bbox: [-1.0, -1.0, 1.0, 1.0],
            paths: vec![test_airspace_path(
                true,
                None,
                vec![[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]],
            )],
        };
        let mut ref_cache = HashMap::new();
        for tile in visible_layer_tile_window(
            "airspace",
            airspace_reference_zoom(8.0, &test_map_overlay_config()),
            &viewport,
            1024.0,
            768.0,
        ) {
            ref_cache.insert(
                airspace_ref_tile_key(tile.z, tile.x, tile.y),
                AirspaceReferenceTilePayload {
                    schema_version: 1,
                    layer: "airspace".to_string(),
                    z: tile.z,
                    x: tile.x,
                    y: tile.y,
                    refs: vec![feature.id.clone()],
                },
            );
        }
        let mut label_cache = HashMap::new();
        for tile in visible_layer_tile_window(
            "airspace-labels",
            airspace_label_zoom(8.0, &test_map_overlay_config()),
            &viewport,
            1024.0,
            768.0,
        ) {
            label_cache.insert(
                airspace_label_tile_key(tile.z, tile.x, tile.y),
                AirspaceLabelTilePayload {
                    schema_version: 1,
                    layer: "airspace-labels".to_string(),
                    z: tile.z,
                    x: tile.x,
                    y: tile.y,
                    labels: vec![AirspaceLabelRecord {
                        feature_id: feature.id.clone(),
                        text: "40/23".to_string(),
                        lon: 0.0,
                        lat: 0.0,
                        rank: 0,
                        score: Some(1.0),
                        style_hint: "class_b".to_string(),
                    }],
                },
            );
        }

        let result = query_map_overlay_with_point_display_scale(
            MapSurfaceMetrics::new(viewport, 1024.0, 768.0, 4.0),
            &HashMap::new(),
            &ref_cache,
            &HashMap::from([(feature.id.clone(), feature)]),
            &label_cache,
        );

        assert_eq!(result.airspace_paths.len(), 1);
        assert_eq!(result.airspace_labels.len(), 1);
        assert!(!result.needed_vector_tiles.iter().any(|tile| tile.z == 10));
    }

    #[test]
    fn surface_decision_normalizes_web_and_android_pixel_spaces() {
        let effective_zoom = 9.3;
        let android_scale = 2.625_f64;
        let web_metrics = MapSurfaceMetrics::new(
            MapViewport {
                center: LatLon {
                    lat: 37.45,
                    lon: -122.25,
                },
                zoom: effective_zoom,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            500.0,
            921.0,
            1.0,
        );
        let android_metrics = MapSurfaceMetrics::new(
            MapViewport {
                center: web_metrics.viewport.center,
                zoom: effective_zoom + android_scale.log2(),
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            1080.0,
            2146.0,
            android_scale,
        );

        let config = test_map_overlay_config();
        let web = overlay_surface_decision(web_metrics, &config);
        let android = overlay_surface_decision(android_metrics, &config);

        assert!((web.effective_display_zoom - android.effective_display_zoom).abs() < 1e-9);
        assert_eq!(web.point_tile_zoom, android.point_tile_zoom);
        assert_eq!(web.metar_tile_zoom, android.metar_tile_zoom);
        assert_eq!(web.airspace_ref_zoom, android.airspace_ref_zoom);
        assert_eq!(web.airspace_label_zoom, android.airspace_label_zoom);
    }

    #[test]
    fn minimum_display_zoom_is_converted_to_surface_zoom() {
        let display_scale = 1.75_f64;
        let viewport = MapViewport {
            center: LatLon { lat: 0.0, lon: 0.0 },
            zoom: 6.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let web = MapSurfaceMetrics::new(viewport, 800.0, 600.0, 1.0);
        let android = MapSurfaceMetrics::new(viewport, 1400.0, 1050.0, display_scale);

        assert_eq!(web.raw_zoom_at_least_display_zoom(10.0), 10.0);
        let android_target = android.raw_zoom_at_least_display_zoom(10.0);
        assert!((android_target - (10.0 + display_scale.log2())).abs() < 1e-9);
        assert!(
            (MapSurfaceMetrics::new(
                MapViewport {
                    zoom: android_target,
                    ..viewport
                },
                android.width_px,
                android.height_px,
                display_scale,
            )
            .effective_display_zoom()
                - 10.0)
                .abs()
                < 1e-9
        );
    }

    #[test]
    fn captured_samsung_metars_match_equivalent_web_overlay_and_selection() {
        let (web_metrics, android_metrics) = captured_samsung_surface_metrics();
        let position = web_metrics.viewport.center;
        let important_station = "KIMPORTANT";
        let dense_station = "KDENSE";
        let important_tile = metar_tile_for_position(5, position, important_station);
        let dense_tile = metar_tile_for_position(6, position, dense_station);
        let metar_tile_cache = HashMap::from([
            (
                tile_key(
                    &important_tile.layer,
                    important_tile.z,
                    important_tile.x,
                    important_tile.y,
                ),
                important_tile,
            ),
            (
                tile_key(&dense_tile.layer, dense_tile.z, dense_tile.x, dense_tile.y),
                dense_tile,
            ),
        ]);
        let metar_payload = MetarProductPayload {
            schema_version: 3,
            version_label: "capture".to_string(),
            generated_at_utc: None,
            observed_at_utc: None,
            metar_count: Some(2),
            metars_by_station: HashMap::from([
                (
                    important_station.to_string(),
                    test_metar_record(important_station, position),
                ),
                (
                    dense_station.to_string(),
                    test_metar_record(dense_station, position),
                ),
            ]),
        };
        let config = test_map_overlay_config();
        let empty_vector_tiles = HashMap::new();
        let empty_obstacle_tiles = HashMap::new();
        let empty_airspaces = HashMap::new();
        let overlay_for = |metrics: &MapSurfaceMetrics| {
            query_map_overlay_for_surface(
                metrics,
                MapOverlayQuery {
                    display_metars: true,
                    metar_payload: Some(&metar_payload),
                    ..MapOverlayQuery::new(
                        &config,
                        &empty_vector_tiles,
                        &empty_obstacle_tiles,
                        &metar_tile_cache,
                        &empty_airspaces,
                    )
                },
            )
        };

        let web_overlay = overlay_for(&web_metrics);
        let android_overlay = overlay_for(&android_metrics);
        let web_stations = web_overlay
            .visible_metars
            .iter()
            .map(|metar| metar.station_id.as_str())
            .collect::<Vec<_>>();
        let android_stations = android_overlay
            .visible_metars
            .iter()
            .map(|metar| metar.station_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(web_stations, vec![important_station]);
        assert_eq!(android_stations, web_stations);

        let selection_for = |metrics: &MapSurfaceMetrics| {
            let mut plate_availability = |_airport_id: &str| AirportPlateAvailability::default();
            let aliases = WeatherStationAirportAliases::default();
            query_map_selection_for_surface(
                metrics,
                MapSelectionQuery {
                    metar_payload: Some(&metar_payload),
                    ..MapSelectionQuery::new(
                        &config,
                        position,
                        &empty_vector_tiles,
                        &metar_tile_cache,
                        &empty_airspaces,
                        &aliases,
                        &mut plate_availability,
                    )
                },
            )
        };
        let selected_stations = |selection: &MapSelectionQueryResult| {
            selection
                .categories
                .iter()
                .find(|category| category.id == "weather")
                .into_iter()
                .flat_map(|category| &category.items)
                .filter_map(|item| item.metar_feature.as_ref())
                .map(|metar| metar.station_id.clone())
                .collect::<Vec<_>>()
        };
        let web_selection = selection_for(&web_metrics);
        let android_selection = selection_for(&android_metrics);
        assert_eq!(
            selected_stations(&web_selection),
            vec![important_station.to_string()]
        );
        assert_eq!(
            selected_stations(&android_selection),
            selected_stations(&web_selection)
        );
    }

    #[test]
    fn tfr_visibility_uses_density_normalized_zoom() {
        let (web_metrics, android_metrics) = captured_samsung_surface_metrics();
        let projection_for = |metrics: MapSurfaceMetrics| {
            query_tfr_overlay(
                &MapProjectionContext::new(&metrics),
                TfrOverlayInput {
                    payload: None,
                    point_features: &[],
                    protected_point_features: &[],
                    reference_utc: None,
                },
            )
        };

        let web = projection_for(web_metrics);
        let android = projection_for(android_metrics);
        assert!(!web.needed_tfrs);
        assert_eq!(android.needed_tfrs, web.needed_tfrs);
    }

    #[test]
    fn inspector_hit_radius_uses_surface_display_scale() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 37.45,
                lon: -122.25,
            },
            zoom: 9.3,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };

        let web_metrics = MapSurfaceMetrics::new(viewport, 500.0, 921.0, 1.0);
        let android_metrics = MapSurfaceMetrics::new(viewport, 1080.0, 2146.0, 2.625);

        assert_eq!(
            web_metrics.inspector_hit_radius_px(),
            UI_THUMB_SIZE_LOGICAL_PX * INSPECTOR_HIT_RADIUS_THUMBS
        );
        assert_eq!(
            android_metrics.inspector_hit_radius_px(),
            UI_THUMB_SIZE_LOGICAL_PX * INSPECTOR_HIT_RADIUS_THUMBS * 2.625
        );
    }

    #[test]
    fn vector_tile_window_wraps_source_x_for_repeated_worlds() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 0.0,
                lon: -540.0,
            },
            zoom: 5.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let tiles = visible_layer_tile_window("airport", 5, &viewport, 800.0, 600.0);

        assert!(!tiles.is_empty());
        assert!(tiles.iter().all(|tile| tile.x < 32));
        assert!(tiles.iter().any(|tile| tile.x == 0 || tile.x == 31));
    }

    #[test]
    fn vector_display_tile_window_keeps_repeated_world_copies() {
        let viewport = MapViewport {
            center: LatLon { lat: 0.0, lon: 0.0 },
            zoom: 1.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let display_tiles =
            visible_layer_display_tile_window("airport", 1, &viewport, 1024.0, 256.0);
        let unique_requests =
            dedupe_vector_tile_requests(display_tiles.iter().cloned().map(|tile| tile.request));

        assert!(display_tiles.len() > unique_requests.len());
        assert!(display_tiles
            .iter()
            .any(|tile| tile.request.x == 1 && tile.world_x_offset < 0.0));
        assert!(display_tiles
            .iter()
            .any(|tile| tile.request.x == 0 && tile.world_x_offset > 0.0));
    }

    #[test]
    fn weather_projection_uses_nearest_wrapped_world_copy() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 52.0,
                lon: -530.0,
            },
            zoom: 4.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let center_world = lat_lon_to_world(viewport.center);
        let feature = visible_pirep_feature(
            &PirepRecord {
                id: "pacific".to_string(),
                raw_text: "UA /OV PAC".to_string(),
                observed_at_utc: None,
                report_type: None,
                longitude: -170.0,
                latitude: 52.0,
                symbol: "generic".to_string(),
                icing: String::new(),
                turbulence: String::new(),
            },
            center_world,
            2.0_f64.powf(viewport.zoom),
            800.0,
            600.0,
            WorldXProjection::NearestWrappedCopy,
        );

        assert!((feature.screen_x - 400.0).abs() < 2.0);
        assert!((feature.screen_y - 300.0).abs() < 2.0);
    }

    #[test]
    fn offline_region_dateline_polygon_stays_short_in_wrapped_world_copy() {
        let center_world = lat_lon_to_world(LatLon {
            lat: 52.0,
            lon: -530.0,
        });
        let regions = vec![OfflineRegionRecord {
            id: "pacific".to_string(),
            kind: "plate".to_string(),
            region_id: "pac".to_string(),
            label: "PAC".to_string(),
            color_key: "pac".to_string(),
            summary: Vec::new(),
            polygons: vec![vec![
                LatLon {
                    lat: 45.0,
                    lon: 170.0,
                },
                LatLon {
                    lat: 45.0,
                    lon: -170.0,
                },
                LatLon {
                    lat: 55.0,
                    lon: -170.0,
                },
                LatLon {
                    lat: 55.0,
                    lon: 170.0,
                },
            ]],
            label_position: LatLon {
                lat: 50.0,
                lon: -180.0,
            },
        }];

        let projected = project_offline_regions(&regions, center_world, 4.0, 800.0, 600.0);
        let xs = projected[0]
            .points
            .iter()
            .map(|point| point.x)
            .collect::<Vec<_>>();
        let span = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - xs.iter().copied().fold(f64::INFINITY, f64::min);

        assert!(span < 80.0, "dateline polygon projected too wide: {span}");
    }

    #[test]
    fn offline_region_dateline_polygon_copy_uses_polygon_center_not_first_vertex() {
        let center_world = lat_lon_to_world(LatLon {
            lat: 52.0,
            lon: 0.0,
        });
        let regions = vec![OfflineRegionRecord {
            id: "pacific".to_string(),
            kind: "plate".to_string(),
            region_id: "pac".to_string(),
            label: "PAC".to_string(),
            color_key: "pac".to_string(),
            summary: Vec::new(),
            polygons: vec![vec![
                LatLon {
                    lat: 45.0,
                    lon: -170.0,
                },
                LatLon {
                    lat: 45.0,
                    lon: 170.0,
                },
                LatLon {
                    lat: 55.0,
                    lon: 170.0,
                },
                LatLon {
                    lat: 55.0,
                    lon: -170.0,
                },
            ]],
            label_position: LatLon {
                lat: 50.0,
                lon: 180.0,
            },
        }];

        let projected = project_offline_regions(&regions, center_world, 4.0, 800.0, 600.0);
        let xs = projected[0]
            .points
            .iter()
            .map(|point| point.x)
            .collect::<Vec<_>>();
        let span = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - xs.iter().copied().fold(f64::INFINITY, f64::min);

        assert!(span < 80.0, "dateline polygon projected too wide: {span}");
        assert!(
            xs.iter().any(|x| *x > 700.0) || xs.iter().any(|x| *x < 100.0),
            "dateline polygon should sit at a wrapped viewport edge near Greenwich: {xs:?}"
        );
    }

    #[test]
    fn airspace_label_tiles_follow_display_zoom_with_max_clamp() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 37.62,
                lon: -122.38,
            },
            zoom: 11.7,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let result = query_map_overlay(
            &viewport,
            1200.0,
            900.0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );
        assert!(result.needed_vector_tiles.iter().any(|tile| tile.z == 11));

        let overzoomed = MapViewport {
            zoom: 13.2,
            ..viewport
        };
        let result = query_map_overlay(
            &overzoomed,
            1200.0,
            900.0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );
        assert!(result
            .needed_vector_tiles
            .iter()
            .any(|tile| tile.z == test_map_overlay_config().airspace_label_tile_max_zoom));
    }

    #[test]
    fn airspace_ref_tiles_follow_display_zoom_to_detailed_shelves() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 33.6367,
                lon: -84.4281,
            },
            zoom: 9.82,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let result = query_map_overlay(
            &viewport,
            1200.0,
            900.0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );

        assert!(result.needed_vector_tiles.iter().any(|tile| tile.z == 9));
    }

    #[test]
    fn vector_manifest_config_controls_airspace_tile_zoom_ranges() {
        let config = map_overlay_config_from_vector_manifest_json(
            r#"{
                "point_layers": {
                    "airport": { "available_zooms": [9] },
                    "fix": { "available_zooms": [9] },
                    "nav": { "available_zooms": [9] }
                },
                "airspace": {
                    "reference_tile_min_zoom": 3,
                    "reference_tile_max_zoom": 11,
                    "label_tile_min_zoom": 2,
                    "label_tile_max_zoom": 10
                }
            }"#,
        )
        .expect("manifest should parse");

        assert_eq!(config.airspace_reference_tile_min_zoom, 3);
        assert_eq!(config.airspace_reference_tile_max_zoom, 11);
        assert_eq!(config.airspace_label_tile_min_zoom, 2);
        assert_eq!(config.airspace_label_tile_max_zoom, 10);
    }

    #[test]
    fn vector_manifest_config_controls_point_tile_zoom_levels() {
        let config = map_overlay_config_from_vector_manifest_json(
            r#"{
                "point_layers": {
                    "airport": { "available_zooms": [9] },
                    "fix": { "available_zooms": [9] },
                    "nav": { "available_zooms": [9] }
                },
                "airspace": {
                    "reference_tile_min_zoom": 0,
                    "reference_tile_max_zoom": 12,
                    "label_tile_min_zoom": 0,
                    "label_tile_max_zoom": 12
                }
            }"#,
        )
        .expect("manifest should parse");
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.36,
                lon: -121.98,
            },
            zoom: 10.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };

        let density_scaled =
            visible_point_tile_window_with_display_scale(&config, &viewport, 1200.0, 900.0, 3.0);

        assert!(density_scaled
            .iter()
            .any(|tile| tile.layer == "airport" && tile.z == 9));
        assert!(!density_scaled
            .iter()
            .any(|tile| tile.layer == "airport" && tile.z == 8));
    }

    #[test]
    fn vector_manifest_config_controls_metar_tile_zoom_levels() {
        let config = map_overlay_config_from_vector_manifest_json(
            r#"{
                "point_layers": {
                    "airport": { "available_zooms": [9] },
                    "fix": { "available_zooms": [9] },
                    "nav": { "available_zooms": [9] },
                    "metars": {
                        "min_zoom": 5,
                        "max_zoom": 7,
                        "available_zooms": [5, 6, 7],
                        "tile_path_template": "points/metars/{z}/{x}/{y}.json"
                    }
                },
                "airspace": {
                    "reference_tile_min_zoom": 0,
                    "reference_tile_max_zoom": 12,
                    "label_tile_min_zoom": 0,
                    "label_tile_max_zoom": 12
                }
            }"#,
        )
        .expect("manifest should parse");
        let viewport = MapViewport {
            center: LatLon { lat: 0.0, lon: 0.0 },
            zoom: 4.2,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let vector_tiles = HashMap::new();
        let obstacle_tiles = HashMap::new();
        let metar_tiles = HashMap::new();
        let airspaces = HashMap::new();
        let low_zoom = super::query_map_overlay(
            &viewport,
            240.0,
            240.0,
            MapOverlayQuery {
                display_metars: true,
                ..MapOverlayQuery::new(
                    &config,
                    &vector_tiles,
                    &obstacle_tiles,
                    &metar_tiles,
                    &airspaces,
                )
            },
        );
        assert!(low_zoom.needed_metar_tiles.is_empty());

        let sparse_zoom = super::query_map_overlay(
            &MapViewport {
                zoom: 6.2,
                ..viewport
            },
            240.0,
            240.0,
            MapOverlayQuery {
                display_metars: true,
                ..MapOverlayQuery::new(
                    &config,
                    &vector_tiles,
                    &obstacle_tiles,
                    &metar_tiles,
                    &airspaces,
                )
            },
        );
        assert!(sparse_zoom
            .needed_metar_tiles
            .iter()
            .all(|tile| tile.z == 5));

        let high_zoom = super::query_map_overlay(
            &MapViewport {
                zoom: 9.0,
                ..viewport
            },
            240.0,
            240.0,
            MapOverlayQuery {
                display_metars: true,
                ..MapOverlayQuery::new(
                    &config,
                    &vector_tiles,
                    &obstacle_tiles,
                    &metar_tiles,
                    &airspaces,
                )
            },
        );
        assert!(high_zoom.needed_metar_tiles.iter().all(|tile| tile.z == 7));
    }

    #[test]
    fn low_zoom_weather_uses_sparse_metars_and_hides_pireps() {
        let position = LatLon { lat: 0.0, lon: 0.0 };
        let mut low_tile = metar_tile_for_position(5, position, "KAAA");
        low_tile.records.push(MetarTileRecord {
            kind: "pirep".to_string(),
            id: "pirep:test".to_string(),
        });
        let mut high_tile = low_tile.clone();
        let high_tile_address = metar_tile_for_position(7, position, "unused");
        high_tile.z = high_tile_address.z;
        high_tile.x = high_tile_address.x;
        high_tile.y = high_tile_address.y;
        let tile_cache = HashMap::from([
            (
                tile_key("metars", low_tile.z, low_tile.x, low_tile.y),
                low_tile,
            ),
            (
                tile_key("metars", high_tile.z, high_tile.x, high_tile.y),
                high_tile,
            ),
        ]);
        let metars = MetarProductPayload {
            schema_version: 3,
            version_label: "test".to_string(),
            generated_at_utc: None,
            observed_at_utc: None,
            metar_count: Some(1),
            metars_by_station: HashMap::from([(
                "KAAA".to_string(),
                test_metar_record("KAAA", position),
            )]),
        };
        let pireps = PirepProductPayload {
            schema_version: 3,
            version_label: "test".to_string(),
            generated_at_utc: None,
            observed_at_utc: None,
            pirep_count: Some(1),
            pireps_by_id: HashMap::from([(
                "pirep:test".to_string(),
                PirepRecord {
                    id: "pirep:test".to_string(),
                    raw_text: "TEST PIREP".to_string(),
                    observed_at_utc: None,
                    report_type: Some("PIREP".to_string()),
                    longitude: position.lon,
                    latitude: position.lat,
                    symbol: "generic".to_string(),
                    icing: "none".to_string(),
                    turbulence: "none".to_string(),
                },
            )]),
        };
        let config = test_map_overlay_config();
        let vectors = HashMap::new();
        let obstacles = HashMap::new();
        let airspaces = HashMap::new();
        let query = |zoom| {
            super::query_map_overlay(
                &MapViewport {
                    center: position,
                    zoom,
                    rotation_deg: 0.0,
                    pitch_deg: 0.0,
                },
                400.0,
                400.0,
                MapOverlayQuery {
                    display_metars: true,
                    metar_payload: Some(&metars),
                    pirep_payload: Some(&pireps),
                    ..MapOverlayQuery::new(&config, &vectors, &obstacles, &tile_cache, &airspaces)
                },
            )
        };

        let sparse = query(6.2);
        assert_eq!(sparse.visible_metars.len(), 1);
        assert!(sparse.visible_pireps.is_empty());

        let detailed = query(7.0);
        assert_eq!(detailed.visible_metars.len(), 1);
        assert_eq!(detailed.visible_pireps.len(), 1);
    }

    #[test]
    fn weather_display_cap_is_a_caution_status() {
        let position = LatLon { lat: 0.0, lon: 0.0 };
        let mut tile = metar_tile_for_position(7, position, "unused");
        let mut metars_by_station = HashMap::new();
        tile.records.clear();
        for index in 0..=WEATHER_DISPLAY_FEATURE_LIMIT {
            let station_id = format!("K{index:04}");
            tile.records.push(MetarTileRecord {
                kind: "metar".to_string(),
                id: station_id.clone(),
            });
            metars_by_station.insert(station_id.clone(), test_metar_record(&station_id, position));
        }
        let tile_cache = HashMap::from([(tile_key("metars", tile.z, tile.x, tile.y), tile)]);
        let metars = MetarProductPayload {
            schema_version: 3,
            version_label: "dense".to_string(),
            generated_at_utc: None,
            observed_at_utc: None,
            metar_count: Some(metars_by_station.len() as u32),
            metars_by_station,
        };
        let config = test_map_overlay_config();
        let vectors = HashMap::new();
        let obstacles = HashMap::new();
        let airspaces = HashMap::new();
        let result = super::query_map_overlay(
            &MapViewport {
                center: position,
                zoom: 7.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            400.0,
            400.0,
            MapOverlayQuery {
                display_metars: true,
                metar_payload: Some(&metars),
                ..MapOverlayQuery::new(&config, &vectors, &obstacles, &tile_cache, &airspaces)
            },
        );

        let status = result
            .data_status_records
            .iter()
            .find(|record| record.id == WEATHER_DISPLAY_LIMIT_STATUS_ID)
            .expect("weather display cap status");
        assert_eq!(status.severity, UiStatusSeverity::Warning);
        assert!(status.drives_caution);
    }

    #[test]
    fn airspace_label_candidates_are_filtered_and_deduped_by_rank() {
        let viewport = MapViewport {
            center: LatLon { lat: 0.0, lon: 0.0 },
            zoom: 6.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let empty = query_map_overlay(
            &viewport,
            100.0,
            100.0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );
        let tile = empty
            .needed_vector_tiles
            .first()
            .expect("expected a visible airspace label tile");

        let mut label_cache = HashMap::new();
        label_cache.insert(
            airspace_label_tile_key(tile.z, tile.x, tile.y),
            AirspaceLabelTilePayload {
                schema_version: 1,
                layer: "airspace-labels".to_string(),
                z: tile.z,
                x: tile.x,
                y: tile.y,
                labels: vec![
                    AirspaceLabelRecord {
                        feature_id: "feature-a".to_string(),
                        text: "A-OFFSCREEN/SFC".to_string(),
                        lon: 10.0,
                        lat: 0.0,
                        rank: 0,
                        score: Some(1.0),
                        style_hint: "class_b".to_string(),
                    },
                    AirspaceLabelRecord {
                        feature_id: "feature-a".to_string(),
                        text: "A2/SFC".to_string(),
                        lon: -1.5,
                        lat: 0.0,
                        rank: 2,
                        score: Some(0.2),
                        style_hint: "class_b".to_string(),
                    },
                    AirspaceLabelRecord {
                        feature_id: "feature-a".to_string(),
                        text: "A1/SFC".to_string(),
                        lon: -1.5,
                        lat: 0.0,
                        rank: 1,
                        score: Some(0.1),
                        style_hint: "class_b".to_string(),
                    },
                    AirspaceLabelRecord {
                        feature_id: "feature-b".to_string(),
                        text: "B1/SFC".to_string(),
                        lon: 1.5,
                        lat: 0.0,
                        rank: 1,
                        score: Some(0.9),
                        style_hint: "class_c".to_string(),
                    },
                    AirspaceLabelRecord {
                        feature_id: "feature-b".to_string(),
                        text: "B0/SFC".to_string(),
                        lon: 1.5,
                        lat: 0.0,
                        rank: 0,
                        score: Some(0.1),
                        style_hint: "class_c".to_string(),
                    },
                ],
            },
        );

        let result = query_map_overlay(
            &viewport,
            240.0,
            240.0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &label_cache,
        );

        assert_eq!(result.airspace_labels.len(), 2);
        assert_eq!(result.airspace_labels[0].feature_id, "feature-a");
        assert_eq!(result.airspace_labels[1].feature_id, "feature-b");
        assert_eq!(result.airspace_labels[0].glyph.upper, "A1");
        assert_eq!(result.airspace_labels[1].glyph.upper, "B0");
    }

    #[test]
    fn metar_overlay_joins_tile_station_ids_to_product_records() {
        let viewport = MapViewport {
            center: LatLon { lat: 0.0, lon: 0.0 },
            zoom: 10.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let tiles = visible_layer_tile_window("metars", 7, &viewport, 240.0, 240.0);
        let tile = tiles.first().expect("expected visible metar tile").clone();
        let mut metar_tile_cache = HashMap::new();
        for requested_tile in &tiles {
            metar_tile_cache.insert(
                tile_key(
                    &requested_tile.layer,
                    requested_tile.z,
                    requested_tile.x,
                    requested_tile.y,
                ),
                MetarTilePayload {
                    schema_version: 1,
                    layer: "metars".to_string(),
                    z: requested_tile.z,
                    x: requested_tile.x,
                    y: requested_tile.y,
                    records: Vec::new(),
                },
            );
        }
        metar_tile_cache.insert(
            tile_key(&tile.layer, tile.z, tile.x, tile.y),
            MetarTilePayload {
                schema_version: 1,
                layer: "metars".to_string(),
                z: tile.z,
                x: tile.x,
                y: tile.y,
                records: vec![MetarTileRecord {
                    kind: "metar".to_string(),
                    id: "KAAA".to_string(),
                }],
            },
        );
        let mut metars_by_station = HashMap::new();
        metars_by_station.insert(
            "KAAA".to_string(),
            MetarRecord {
                raw_text: "METAR KAAA 010000Z 00000KT 10SM SCT020 10/08 A3000".to_string(),
                observed_at_utc: Some("2026-05-03T00:00:00.000Z".to_string()),
                station_id: "KAAA".to_string(),
                flight_category: Some("MVFR".to_string()),
                clouds: Some(MetarClouds {
                    symbol: Some("SCT".to_string()),
                }),
                longitude: viewport.center.lon,
                latitude: viewport.center.lat,
            },
        );
        let metars = MetarProductPayload {
            schema_version: 3,
            version_label: "test".to_string(),
            generated_at_utc: None,
            observed_at_utc: None,
            metar_count: Some(1),
            metars_by_station,
        };
        let config = test_map_overlay_config();
        let vector_tiles = HashMap::new();
        let obstacle_tiles = HashMap::new();
        let airspaces = HashMap::new();
        let result = super::query_map_overlay(
            &viewport,
            240.0,
            240.0,
            MapOverlayQuery {
                display_metars: true,
                metar_payload: Some(&metars),
                ..MapOverlayQuery::new(
                    &config,
                    &vector_tiles,
                    &obstacle_tiles,
                    &metar_tile_cache,
                    &airspaces,
                )
            },
        );

        assert!(result.needed_metar_tiles.is_empty());
        assert!(!result.needed_metars);
        assert_eq!(result.visible_metars.len(), 1);
        assert_eq!(result.visible_metars[0].station_id, "KAAA");
        assert_eq!(result.visible_metars[0].flight_category, "mvfr");
        assert_eq!(result.visible_metars[0].ceiling_amount, "sct");
    }

    #[derive(Debug, Clone, Copy)]
    struct OverlayIngredientMask {
        point_tile: bool,
        airspace_ref_tile: bool,
        airspace_feature: bool,
        airspace_label_tile: bool,
        metar_tile: bool,
        metar_product: bool,
        tfr_product: bool,
        offline_regions: bool,
    }

    impl OverlayIngredientMask {
        fn from_bits(bits: u32) -> Self {
            Self {
                point_tile: bits & (1 << 0) != 0,
                airspace_ref_tile: bits & (1 << 1) != 0,
                airspace_feature: bits & (1 << 2) != 0,
                airspace_label_tile: bits & (1 << 3) != 0,
                metar_tile: bits & (1 << 4) != 0,
                metar_product: bits & (1 << 5) != 0,
                tfr_product: bits & (1 << 6) != 0,
                offline_regions: bits & (1 << 7) != 0,
            }
        }
    }

    #[test]
    fn overlay_query_renders_every_available_ingredient_across_missing_ingredient_combinations() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.0,
                lon: -122.0,
            },
            zoom: 8.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let width_px = 1200.0;
        let height_px = 900.0;
        let point_tile =
            visible_point_tile_window(&test_map_overlay_config(), &viewport, width_px, height_px)
                .into_iter()
                .find(|tile| tile.layer == "airport")
                .expect("expected airport tile");
        let airspace_tile = visible_layer_tile_window(
            "airspace",
            airspace_reference_zoom(viewport.zoom, &test_map_overlay_config()),
            &viewport,
            width_px,
            height_px,
        )
        .into_iter()
        .next()
        .expect("expected airspace tile");
        let airspace_label_tile = visible_layer_tile_window(
            "airspace-labels",
            airspace_label_zoom(viewport.zoom, &test_map_overlay_config()),
            &viewport,
            width_px,
            height_px,
        )
        .into_iter()
        .next()
        .expect("expected airspace label tile");
        let metar_tile = visible_layer_tile_window("metars", 7, &viewport, width_px, height_px)
            .into_iter()
            .next()
            .expect("expected metar tile");

        for bits in 0..(1 << 8) {
            let mask = OverlayIngredientMask::from_bits(bits);
            let mut vector_tile_cache = HashMap::new();
            if mask.point_tile {
                vector_tile_cache.insert(
                    aggregate_vector_tile_cache_key(point_tile.z, point_tile.x, point_tile.y),
                    VectorAggregateTilePayload {
                        schema_version: 1,
                        z: point_tile.z,
                        x: point_tile.x,
                        y: point_tile.y,
                        airports: vec![PointVectorRecord {
                            id: "airports:KAAA".to_string(),
                            kind: "airport".to_string(),
                            lat: viewport.center.lat,
                            lon: viewport.center.lon,
                            label: "KAAA".to_string(),
                            location_label: None,
                            style_class: "airport".to_string(),
                            towered: Some(true),
                            fuel_available: Some(true),
                            public_use: Some(true),
                            private_use: Some(false),
                            has_paved_runway: Some(true),
                            heliport: Some(false),
                            has_water_runway: Some(false),
                            longest_runway_length_ft: Some(4000.0),
                            longest_runway_heading_true_deg: Some(180.0),
                            elevation_msl_ft: Some(100.0),
                            obstacle: None,
                            weather_camera: None,
                        }],
                        fixes: Vec::new(),
                        navaids: Vec::new(),
                        airspace_refs: Vec::new(),
                        airspace_labels: Vec::new(),
                    },
                );
            }
            if mask.airspace_ref_tile || mask.airspace_label_tile {
                let aggregate = vector_tile_cache
                    .entry(aggregate_vector_tile_cache_key(
                        airspace_tile.z,
                        airspace_tile.x,
                        airspace_tile.y,
                    ))
                    .or_insert_with(|| {
                        empty_test_vector_tile(airspace_tile.z, airspace_tile.x, airspace_tile.y)
                    });
                if mask.airspace_ref_tile {
                    aggregate.airspace_refs = vec!["airspace:test:class_b".to_string()];
                }
            }
            if mask.airspace_label_tile {
                let aggregate = vector_tile_cache
                    .entry(aggregate_vector_tile_cache_key(
                        airspace_label_tile.z,
                        airspace_label_tile.x,
                        airspace_label_tile.y,
                    ))
                    .or_insert_with(|| {
                        empty_test_vector_tile(
                            airspace_label_tile.z,
                            airspace_label_tile.x,
                            airspace_label_tile.y,
                        )
                    });
                aggregate.airspace_labels = vec![AirspaceLabelRecord {
                    feature_id: "airspace:test:label".to_string(),
                    text: "40/20".to_string(),
                    lon: -121.6,
                    lat: 47.3,
                    rank: 1,
                    score: Some(1.0),
                    style_hint: "class_b".to_string(),
                }];
            }

            let mut airspace_feature_cache = HashMap::new();
            if mask.airspace_feature {
                airspace_feature_cache.insert(
                    "airspace:test:class_b".to_string(),
                    AirspaceFeaturePayload {
                        schema_version: 1,
                        id: "airspace:test:class_b".to_string(),
                        kind: "airspace".to_string(),
                        name: "TEST CLASS B".to_string(),
                        ident: "TEST".to_string(),
                        airspace_class: "B".to_string(),
                        style_hint: "class_b".to_string(),
                        vertical: test_airspace_vertical("40", "20"),
                        bbox: [-122.4, 46.6, -122.2, 46.8],
                        paths: vec![test_airspace_path(
                            true,
                            None,
                            vec![
                                [-122.4, 46.6],
                                [-122.2, 46.6],
                                [-122.2, 46.8],
                                [-122.4, 46.8],
                            ],
                        )],
                    },
                );
            }

            let mut metar_tile_cache = HashMap::new();
            if mask.metar_tile {
                metar_tile_cache.insert(
                    tile_key(&metar_tile.layer, metar_tile.z, metar_tile.x, metar_tile.y),
                    MetarTilePayload {
                        schema_version: 1,
                        layer: "metars".to_string(),
                        z: metar_tile.z,
                        x: metar_tile.x,
                        y: metar_tile.y,
                        records: vec![MetarTileRecord {
                            kind: "metar".to_string(),
                            id: "KMT1".to_string(),
                        }],
                    },
                );
            }
            let metar_product = if mask.metar_product {
                Some(MetarProductPayload {
                    schema_version: 3,
                    version_label: "test".to_string(),
                    generated_at_utc: None,
                    observed_at_utc: None,
                    metar_count: Some(1),
                    metars_by_station: HashMap::from([(
                        "KMT1".to_string(),
                        MetarRecord {
                            raw_text: "METAR KMT1 010000Z 00000KT 10SM SCT020 10/08 A3000"
                                .to_string(),
                            observed_at_utc: Some("2026-05-03T00:00:00.000Z".to_string()),
                            station_id: "KMT1".to_string(),
                            flight_category: Some("VFR".to_string()),
                            clouds: None,
                            longitude: -121.8,
                            latitude: 47.2,
                        },
                    )]),
                })
            } else {
                None
            };
            let tfr_product = if mask.tfr_product {
                Some(TfrProductPayload {
                    schema_version: 1,
                    version_label: "test".to_string(),
                    generated_at_utc: None,
                    notam_count: 1,
                    area_group_count: 1,
                    areas: vec![TfrAreaPayload {
                        notam_id: "1/2345".to_string(),
                        area_index: 0,
                        schedule_fragments: Vec::new(),
                        upper_limit: TfrAltitudeLimit {
                            value_text: "180".to_string(),
                            unit: "FL".to_string(),
                        },
                        lower_limit: TfrAltitudeLimit {
                            value_text: "SFC".to_string(),
                            unit: "SFC".to_string(),
                        },
                        polygon: vec![
                            TfrLatLonPoint {
                                lat: 46.9,
                                lon: -121.8,
                            },
                            TfrLatLonPoint {
                                lat: 46.9,
                                lon: -121.6,
                            },
                            TfrLatLonPoint {
                                lat: 47.1,
                                lon: -121.6,
                            },
                            TfrLatLonPoint {
                                lat: 47.1,
                                lon: -121.8,
                            },
                        ],
                        summary_text: "test TFR".to_string(),
                        notam: None,
                    }],
                })
            } else {
                None
            };
            let offline_regions = if mask.offline_regions {
                vec![OfflineRegionRecord {
                    id: "chart:nw".to_string(),
                    kind: "chart".to_string(),
                    region_id: "nw".to_string(),
                    label: "NW".to_string(),
                    color_key: "offline_region_chart".to_string(),
                    summary: Vec::new(),
                    polygons: vec![vec![
                        LatLon {
                            lat: 47.35,
                            lon: -121.5,
                        },
                        LatLon {
                            lat: 47.35,
                            lon: -121.3,
                        },
                        LatLon {
                            lat: 47.55,
                            lon: -121.3,
                        },
                        LatLon {
                            lat: 47.55,
                            lon: -121.5,
                        },
                    ]],
                    label_position: LatLon {
                        lat: 47.45,
                        lon: -121.4,
                    },
                }]
            } else {
                Vec::new()
            };

            let config = test_map_overlay_config();
            let obstacle_tile_cache = HashMap::new();
            let result = super::query_map_overlay(
                &viewport,
                width_px,
                height_px,
                MapOverlayQuery {
                    display_vectors: true,
                    display_metars: true,
                    offline_region_records: &offline_regions,
                    metar_payload: metar_product.as_ref(),
                    tfr_payload: tfr_product.as_ref(),
                    ..MapOverlayQuery::new(
                        &config,
                        &vector_tile_cache,
                        &obstacle_tile_cache,
                        &metar_tile_cache,
                        &airspace_feature_cache,
                    )
                },
            );

            let case = format!("{mask:?}");
            assert_eq!(
                result
                    .visible_features
                    .iter()
                    .any(|feature| feature.id == "airports:KAAA"),
                mask.point_tile,
                "{case}: point vectors should depend only on the point vector tile"
            );
            assert_eq!(
                result
                    .airspace_paths
                    .iter()
                    .any(|path| path.id == "airspace:test:class_b"),
                mask.airspace_ref_tile && mask.airspace_feature,
                "{case}: airspace paths should require the ref tile and feature payload"
            );
            assert_eq!(
                result
                    .airspace_labels
                    .iter()
                    .any(|label| label.feature_id == "airspace:test:label"),
                mask.airspace_label_tile,
                "{case}: airspace labels should depend only on the label tile"
            );
            assert_eq!(
                result
                    .visible_metars
                    .iter()
                    .any(|metar| metar.station_id == "KMT1"),
                mask.metar_tile && mask.metar_product,
                "{case}: METARs should require the tile index and product payload"
            );
            assert_eq!(
                result
                    .tfr_paths
                    .iter()
                    .any(|path| path.id == "tfr:1/2345:0"),
                mask.tfr_product,
                "{case}: TFRs should depend only on the TFR product payload"
            );
            assert_eq!(
                result
                    .offline_regions
                    .iter()
                    .any(|region| region.id.starts_with("chart:nw:")),
                mask.offline_regions,
                "{case}: offline regions should depend only on their catalog records"
            );
            assert_eq!(
                result.needed_metars, !mask.metar_product,
                "{case}: missing METAR product should be reported without suppressing other ingredients"
            );
            if mask.airspace_ref_tile && !mask.airspace_feature {
                assert!(
                    result
                        .needed_airspace_features
                        .iter()
                        .any(|feature| feature.id == "airspace:test:class_b"),
                    "{case}: missing airspace feature should be requested without suppressing other ingredients"
                );
            }
            assert!(
                result
                    .data_status_records
                    .iter()
                    .all(|record| record.id
                    != "map_overlay:airspace_interior_side_contract"),
                "{case}: optional ingredient combinations should not create unrelated contract warnings"
            );
        }
    }

    #[test]
    fn map_selection_returns_metars_in_weather_category() {
        let viewport = MapViewport {
            center: LatLon { lat: 0.0, lon: 0.0 },
            zoom: 8.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let tiles = visible_layer_tile_window("metars", 7, &viewport, 240.0, 240.0);
        let tile = tiles.first().expect("expected visible metar tile").clone();
        let mut metar_tile_cache = HashMap::new();
        metar_tile_cache.insert(
            tile_key(&tile.layer, tile.z, tile.x, tile.y),
            MetarTilePayload {
                schema_version: 1,
                layer: "metars".to_string(),
                z: tile.z,
                x: tile.x,
                y: tile.y,
                records: vec![MetarTileRecord {
                    kind: "metar".to_string(),
                    id: "KAAA".to_string(),
                }],
            },
        );
        let raw_text = "METAR KAAA 010000Z 00000KT 10SM SCT020 10/08 A3000";
        let mut metars_by_station = HashMap::new();
        metars_by_station.insert(
            "KAAA".to_string(),
            MetarRecord {
                raw_text: raw_text.to_string(),
                observed_at_utc: Some("2026-05-03T00:00:00.000Z".to_string()),
                station_id: "KAAA".to_string(),
                flight_category: Some("MVFR".to_string()),
                clouds: Some(MetarClouds {
                    symbol: Some("SCT".to_string()),
                }),
                longitude: viewport.center.lon,
                latitude: viewport.center.lat,
            },
        );
        let metars = MetarProductPayload {
            schema_version: 3,
            version_label: "test".to_string(),
            generated_at_utc: None,
            observed_at_utc: None,
            metar_count: Some(1),
            metars_by_station,
        };
        let taf_raw_text = "TAF KAAA 010000Z 0100/0124 00000KT P6SM SCT020 BECMG 0102/0104 BKN030 FM010600 22008KT P6SM SCT050";
        let tafs = TafProductPayload {
            schema_version: 1,
            version_label: "test".to_string(),
            generated_at_utc: None,
            taf_count: Some(1),
            tafs_by_station: HashMap::from([(
                "KAAA".to_string(),
                TafRecord {
                    raw_text: taf_raw_text.to_string(),
                    issued_at_utc: Some("2026-05-03T00:00:00.000Z".to_string()),
                    station_id: "KAAA".to_string(),
                    longitude: viewport.center.lon,
                    latitude: viewport.center.lat,
                },
            )]),
        };
        let config = test_map_overlay_config();
        let vector_tiles = HashMap::new();
        let airspaces = HashMap::new();
        let aliases = WeatherStationAirportAliases::default();
        let mut availability = |_: &str| AirportPlateAvailability::default();
        let result = query_map_selection(
            &viewport,
            240.0,
            240.0,
            MapSelectionQuery {
                metar_payload: Some(&metars),
                taf_payload: Some(&tafs),
                ..MapSelectionQuery::new(
                    &config,
                    viewport.center,
                    &vector_tiles,
                    &metar_tile_cache,
                    &airspaces,
                    &aliases,
                    &mut availability,
                )
            },
        );
        let weather = result
            .categories
            .iter()
            .find(|category| category.id == "weather")
            .expect("weather category");
        let item = weather.items.first().expect("METAR selection item");
        assert!(result
            .initial_selected_item_id
            .as_deref()
            .is_some_and(|id| id.starts_with("spot:")));

        assert_eq!(item.label, "KAAA");
        assert_eq!(item.detail_text.as_deref(), None);
        assert!(matches!(
            item.highlight,
            MapSelectionHighlight::Metar { ref station_id } if station_id == "KAAA"
        ));
        assert_eq!(
            item.metar_feature
                .as_ref()
                .map(|feature| feature.ceiling_amount.as_str()),
            Some("sct")
        );
        let wx_action = item
            .actions
            .iter()
            .find(|action| action.id == "wx")
            .expect("WX action");
        assert!(wx_action.enabled);
        assert_eq!(
            wx_action
                .weather_detail
                .as_ref()
                .and_then(|detail| detail.metar_text.as_deref()),
            Some(raw_text)
        );
        assert_eq!(
            wx_action
                .weather_detail
                .as_ref()
                .and_then(|detail| detail.taf_text.as_deref()),
            Some("TAF KAAA 010000Z 0100/0124 00000KT P6SM SCT020\nBECMG 0102/0104 BKN030\nFM010600 22008KT P6SM SCT050")
        );
    }

    #[test]
    fn weather_detail_formats_compact_age_labels_in_core() {
        let metar = MetarRecord {
            raw_text: "METAR KAAA 010000Z 00000KT 10SM SCT020 10/08 A3000".to_string(),
            observed_at_utc: Some("2026-05-03T00:00:00Z".to_string()),
            station_id: "KAAA".to_string(),
            flight_category: Some("VFR".to_string()),
            clouds: None,
            longitude: -122.0,
            latitude: 47.0,
        };
        let taf = TafRecord {
            raw_text: "TAF KAAA 010058Z 0101/0124 00000KT P6SM SCT020".to_string(),
            issued_at_utc: Some("2026-05-03T00:58:00Z".to_string()),
            station_id: "KAAA".to_string(),
            longitude: -122.0,
            latitude: 47.0,
        };
        let detail = weather_detail_from_records(
            "KAAA",
            Some(&metar),
            Some(&taf),
            Vec::new(),
            crate::freshness::parse_utc_instant("2026-05-03T01:12:00Z"),
        )
        .expect("weather detail");

        assert_eq!(detail.metar_age_label.as_deref(), Some("1.2h old"));
        assert!(detail.metar_age_warning);
        assert_eq!(detail.taf_age_label.as_deref(), Some("14m old"));
        assert!(!detail.taf_age_warning);
    }

    #[test]
    fn weather_station_airport_aliases_preserve_source_and_canonicalize_airport_ui() {
        let aliases = WeatherStationAirportAliases::from_station_to_airport([(
            "K1S5".to_string(),
            "1S5".to_string(),
            LatLon {
                lat: 46.327,
                lon: -119.970,
            },
        )]);
        let metar = MetarRecord {
            raw_text: "METAR K1S5 260415Z 00000KT 10SM CLR 20/10 A3000".to_string(),
            observed_at_utc: Some("2026-07-26T04:15:00Z".to_string()),
            station_id: "K1S5".to_string(),
            flight_category: Some("VFR".to_string()),
            clouds: None,
            longitude: -119.970,
            latitude: 46.327,
        };
        let payload = MetarProductPayload {
            schema_version: 1,
            version_label: "test".to_string(),
            generated_at_utc: None,
            observed_at_utc: None,
            metar_count: Some(1),
            metars_by_station: HashMap::from([("K1S5".to_string(), metar.clone())]),
        };

        let detail = weather_detail_for_airport("1S5", &aliases, Some(&payload), None, None, None)
            .expect("1S5 should find its K1S5 weather station");
        assert_eq!(detail.station_id, "1S5");
        assert_eq!(detail.metar_text.as_deref(), Some(metar.raw_text.as_str()));

        let item = selection_item_for_metar(
            &metar,
            None,
            VisibleMetarFeature {
                station_id: "K1S5".to_string(),
                screen_x: 10.0,
                screen_y: 20.0,
                flight_category: "vfr".to_string(),
                ceiling_amount: "unlimited".to_string(),
            },
            None,
            &aliases,
            None,
        );
        assert_eq!(item.label, "1S5");
        assert_eq!(item.nav_ref, Some(NavRef::Airport("1S5".to_string())));
        assert_eq!(
            item.metar_feature
                .as_ref()
                .map(|feature| feature.station_id.as_str()),
            Some("K1S5")
        );

        let mut distant_metar = metar.clone();
        distant_metar.latitude = 40.0;
        distant_metar.longitude = -100.0;
        let distant_item = selection_item_for_metar(
            &distant_metar,
            None,
            item.metar_feature.expect("METAR feature"),
            None,
            &aliases,
            None,
        );
        assert_eq!(distant_item.label, "K1S5");
        assert_eq!(distant_item.nav_ref, None);
    }

    #[test]
    fn unassociated_weather_station_keeps_its_source_identifier() {
        let metar = MetarRecord {
            raw_text: "METAR KSMP 260415Z 00000KT 10SM CLR 10/05 A3000".to_string(),
            observed_at_utc: Some("2026-07-26T04:15:00Z".to_string()),
            station_id: "KSMP".to_string(),
            flight_category: Some("VFR".to_string()),
            clouds: None,
            longitude: -121.338,
            latitude: 47.286,
        };
        let item = selection_item_for_metar(
            &metar,
            None,
            VisibleMetarFeature {
                station_id: "KSMP".to_string(),
                screen_x: 10.0,
                screen_y: 20.0,
                flight_category: "vfr".to_string(),
                ceiling_amount: "unlimited".to_string(),
            },
            None,
            &WeatherStationAirportAliases::default(),
            None,
        );

        assert_eq!(item.label, "KSMP");
        assert_eq!(item.nav_ref, None);
        assert_eq!(
            item.actions[0]
                .weather_detail
                .as_ref()
                .map(|detail| detail.station_id.as_str()),
            Some("KSMP")
        );
    }

    #[test]
    fn weather_detail_includes_only_matching_airport_notams() {
        let payload = NotamProductPayload {
            schema_version: NOTAM_LIVE_FEED_CONTRACT_VERSION,
            version_label: "v1".to_string(),
            notam_count: Some(3),
            notams_by_id: HashMap::from([
                (
                    "airport".to_string(),
                    NotamRecord {
                        id: "airport".to_string(),
                        airport_id: Some("AAA".to_string()),
                        airport_effects: BTreeSet::from([AirportNotamEffect::RunwayClosed]),
                        procedure_rendezvous_keys: BTreeSet::new(),
                        notam_keyword: Some("RWY".to_string()),
                        effective_start_utc: None,
                        effective_end_utc: None,
                        text: Some("RWY 18 CLSD".to_string()),
                        local_text: None,
                        icao_text: None,
                    },
                ),
                (
                    "other-airport".to_string(),
                    NotamRecord {
                        id: "other-airport".to_string(),
                        airport_id: Some("KBBB".to_string()),
                        airport_effects: BTreeSet::from([AirportNotamEffect::TaxiwayClosed]),
                        procedure_rendezvous_keys: BTreeSet::new(),
                        notam_keyword: Some("TWY".to_string()),
                        effective_start_utc: None,
                        effective_end_utc: None,
                        text: Some("TWY A CLSD".to_string()),
                        local_text: None,
                        icao_text: None,
                    },
                ),
                (
                    "not-airport".to_string(),
                    NotamRecord {
                        id: "not-airport".to_string(),
                        airport_id: None,
                        airport_effects: BTreeSet::new(),
                        procedure_rendezvous_keys: BTreeSet::new(),
                        notam_keyword: Some("NAV".to_string()),
                        effective_start_utc: None,
                        effective_end_utc: None,
                        text: Some("VOR U/S".to_string()),
                        local_text: None,
                        icao_text: None,
                    },
                ),
            ]),
        };

        let index = NotamDisplayIndex::from_payload(payload).expect("supported NOTAM fixture");
        let detail = weather_detail_for_station(
            "KAAA",
            &WeatherStationAirportAliases::default(),
            None,
            None,
            Some(&index),
            None,
        )
        .expect("airport NOTAM should enable detail");

        assert_eq!(detail.notams.len(), 1);
        assert_eq!(detail.notams[0].label, "RWY");
        assert_eq!(detail.notams[0].text, "RWY 18 CLSD");
        assert_eq!(detail.metar_text, None);
        assert_eq!(detail.taf_text, None);
        assert_eq!(detail.advisory_text, WEATHER_DETAIL_ADVISORY_TEXT);
    }

    #[test]
    fn weather_detail_sorts_airport_notams_by_highest_semantic_priority() {
        let record = |id: &str, keyword: &str, text: &str, effects: &[AirportNotamEffect]| {
            (
                id.to_string(),
                NotamRecord {
                    id: id.to_string(),
                    airport_id: Some("KAAA".to_string()),
                    airport_effects: effects.iter().copied().collect(),
                    procedure_rendezvous_keys: BTreeSet::new(),
                    notam_keyword: Some(keyword.to_string()),
                    effective_start_utc: None,
                    effective_end_utc: None,
                    text: Some(text.to_string()),
                    local_text: None,
                    icao_text: None,
                },
            )
        };
        let index = NotamDisplayIndex::from_payload(NotamProductPayload {
            schema_version: NOTAM_LIVE_FEED_CONTRACT_VERSION,
            version_label: "v1".to_string(),
            notam_count: Some(4),
            notams_by_id: HashMap::from([
                record(
                    "mowing",
                    "AD",
                    "AD AP ALL SFC WIP MOWING",
                    &[AirportNotamEffect::WorkInProgress],
                ),
                record(
                    "taxiway",
                    "TWY",
                    "TWY A CLSD",
                    &[AirportNotamEffect::TaxiwayClosed],
                ),
                record(
                    "runway",
                    "RWY",
                    "RWY 18 CLSD EXC XNG",
                    &[
                        AirportNotamEffect::RunwayClosed,
                        AirportNotamEffect::RunwayRestricted,
                    ],
                ),
                record(
                    "airport",
                    "AD",
                    "AD AP CLSD",
                    &[AirportNotamEffect::AirportClosed],
                ),
            ]),
        })
        .expect("supported NOTAM fixture");

        let detail = weather_detail_for_station(
            "KAAA",
            &WeatherStationAirportAliases::default(),
            None,
            None,
            Some(&index),
            None,
        )
        .expect("airport NOTAMs should enable detail");
        assert_eq!(
            detail
                .notams
                .iter()
                .map(|notam| notam.id.as_str())
                .collect::<Vec<_>>(),
            vec!["airport", "runway", "taxiway", "mowing"]
        );
    }

    #[test]
    fn notam_display_delta_matches_reprojected_canonical_state() {
        let record = |id: &str, airport_id: Option<&str>, text: &str| NotamRecord {
            id: id.to_string(),
            airport_id: airport_id.map(str::to_string),
            airport_effects: BTreeSet::from([AirportNotamEffect::RoutineAdvisory]),
            procedure_rendezvous_keys: BTreeSet::new(),
            notam_keyword: Some("AD".to_string()),
            effective_start_utc: None,
            effective_end_utc: None,
            text: Some(text.to_string()),
            local_text: None,
            icao_text: None,
        };
        let mut state = NotamState::empty();
        for source in [
            record("A", Some("KSEA"), "moves to another airport"),
            record("B", None, "becomes airport-associated"),
            record("C", Some("KPAE"), "loses airport association"),
            record("D", None, "stays outside the projection"),
        ] {
            state
                .apply_mutation(
                    NotamMutation::Upsert { record: source },
                    &mut notam_state::NotamApplyWork::default(),
                )
                .unwrap();
        }
        let mut index =
            NotamDisplayIndex::from_projection_checkpoint(notam_display_checkpoint(&state))
                .unwrap();
        let from_state_id = state.state_id().to_string();
        let mutations = vec![
            NotamMutation::Upsert {
                record: record("A", Some("KBFI"), "moved"),
            },
            NotamMutation::Upsert {
                record: record("B", Some("KSEA"), "now relevant"),
            },
            NotamMutation::Upsert {
                record: record("C", None, "no longer relevant"),
            },
            NotamMutation::Upsert {
                record: record("D", None, "still irrelevant"),
            },
        ];
        let mut expected_state = NotamState::from_checkpoint(
            state.checkpoint(),
            &mut notam_state::NotamApplyWork::default(),
        )
        .unwrap();
        for mutation in mutations.iter().cloned() {
            expected_state
                .apply_mutation(mutation, &mut notam_state::NotamApplyWork::default())
                .unwrap();
        }
        let delta = NotamDelta::new(
            from_state_id,
            expected_state.state_id().to_string(),
            expected_state.counters(),
            mutations,
        );

        let projection_delta = notam_display_delta(&state, &delta).unwrap();
        assert_eq!(projection_delta.mutations.len(), 3);
        state
            .apply_delta(delta, &mut notam_state::NotamApplyWork::default())
            .unwrap();
        index.apply_projection_delta(projection_delta).unwrap();

        let expected =
            NotamDisplayIndex::from_projection_checkpoint(notam_display_checkpoint(&state))
                .unwrap();
        assert_eq!(index, expected);
        assert_eq!(
            index
                .airport_records("KBFI")
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
            vec!["A"]
        );
        assert_eq!(
            index
                .airport_records("KSEA")
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
            vec!["B"]
        );
        assert!(index.airport_records("KPAE").is_empty());
    }

    #[test]
    fn airport_notam_index_rejects_old_product_contract() {
        let error = NotamDisplayIndex::from_payload(NotamProductPayload {
            schema_version: NOTAM_LIVE_FEED_CONTRACT_VERSION - 1,
            version_label: "old".to_string(),
            notam_count: Some(0),
            notams_by_id: HashMap::new(),
        })
        .expect_err("old NOTAM products lack semantic effects");

        assert!(error.contains("unsupported NOTAM live-feed schema"));
    }

    #[test]
    fn procedure_notam_index_moves_records_between_exact_keys() {
        let first = ProcedureRendezvousKey::shared_arrival("CHINS5").unwrap();
        let second = ProcedureRendezvousKey::shared_arrival("GLASR3").unwrap();
        let record = |key: Option<ProcedureRendezvousKey>| NotamRecord {
            id: "STAR-NOTAM".to_string(),
            airport_id: None,
            airport_effects: BTreeSet::new(),
            procedure_rendezvous_keys: key.into_iter().collect(),
            notam_keyword: Some("STAR".to_string()),
            effective_start_utc: None,
            effective_end_utc: None,
            text: Some("STAR PROCEDURE CHANGED".to_string()),
            local_text: None,
            icao_text: None,
        };
        let mut state = NotamState::empty();
        state
            .apply_mutation(
                NotamMutation::Upsert {
                    record: record(Some(first.clone())),
                },
                &mut notam_state::NotamApplyWork::default(),
            )
            .unwrap();
        let mut index =
            NotamDisplayIndex::from_projection_checkpoint(notam_display_checkpoint(&state))
                .unwrap();
        assert_eq!(
            index
                .procedure_records(&BTreeSet::from([first.clone()]))
                .len(),
            1
        );

        let mut next = NotamState::from_checkpoint(
            state.checkpoint(),
            &mut notam_state::NotamApplyWork::default(),
        )
        .unwrap();
        let mutation = NotamMutation::Upsert {
            record: record(Some(second.clone())),
        };
        next.apply_mutation(
            mutation.clone(),
            &mut notam_state::NotamApplyWork::default(),
        )
        .unwrap();
        let delta = NotamDelta::new(
            state.state_id().to_string(),
            next.state_id().to_string(),
            next.counters(),
            vec![mutation],
        );
        let display_delta = notam_display_delta(&state, &delta).unwrap();
        state
            .apply_delta(delta, &mut notam_state::NotamApplyWork::default())
            .unwrap();
        index.apply_projection_delta(display_delta).unwrap();

        assert!(index.procedure_records(&BTreeSet::from([first])).is_empty());
        assert_eq!(index.procedure_records(&BTreeSet::from([second])).len(), 1);
    }

    #[test]
    fn weather_detail_marks_age_warnings_in_core() {
        let reference = crate::freshness::parse_utc_instant("2026-05-03T12:00:00Z");

        assert!(
            !weather_age_status(
                Some("2026-05-03T11:00:00Z"),
                reference,
                METAR_AGE_WARNING_MS,
            )
            .1
        );
        assert!(
            weather_age_status(
                Some("2026-05-03T10:59:59Z"),
                reference,
                METAR_AGE_WARNING_MS,
            )
            .1
        );
        assert!(
            !weather_age_status(Some("2026-05-03T06:00:00Z"), reference, TAF_AGE_WARNING_MS,).1
        );
        assert!(weather_age_status(Some("2026-05-03T05:59:59Z"), reference, TAF_AGE_WARNING_MS,).1);
    }

    #[test]
    fn flight_plan_weather_badge_uses_aliases_and_expires_after_ninety_minutes() {
        let aliases = WeatherStationAirportAliases::from_station_to_airport([(
            "K1S5".to_string(),
            "1S5".to_string(),
            LatLon {
                lat: 46.327,
                lon: -119.970,
            },
        )]);
        let payload = MetarProductPayload {
            schema_version: 3,
            version_label: "test".to_string(),
            generated_at_utc: None,
            observed_at_utc: None,
            metar_count: Some(1),
            metars_by_station: HashMap::from([(
                "K1S5".to_string(),
                MetarRecord {
                    raw_text: "METAR K1S5 261500Z 00000KT 10SM SCT020 10/08 A3000".to_string(),
                    observed_at_utc: Some("2026-08-15T15:00:00Z".to_string()),
                    station_id: "K1S5".to_string(),
                    flight_category: Some("VFR".to_string()),
                    clouds: Some(MetarClouds {
                        symbol: Some("SCT".to_string()),
                    }),
                    longitude: -119.970,
                    latitude: 46.327,
                },
            )]),
        };

        let fresh = flight_plan_weather_badge_for_airport(
            "1S5",
            &aliases,
            Some(&payload),
            crate::freshness::parse_utc_instant("2026-08-15T16:30:00Z"),
        )
        .expect("90-minute-old METAR remains eligible");
        assert_eq!(fresh.flight_category, "vfr");
        assert_eq!(fresh.ceiling_amount, "sct");

        assert!(flight_plan_weather_badge_for_airport(
            "1S5",
            &aliases,
            Some(&payload),
            crate::freshness::parse_utc_instant("2026-08-15T16:30:00.001Z"),
        )
        .is_none());
    }

    #[test]
    fn map_selection_hits_weather_in_repeated_world_copy() {
        let viewport = MapViewport {
            center: LatLon { lat: 0.0, lon: 0.0 },
            zoom: 5.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let width_px = 20_000.0;
        let config = test_map_overlay_config();
        let metar_zoom = nearest_available_layer_zoom(
            config.metar_layer.as_ref().expect("metar layer"),
            viewport.zoom.floor() as u32,
        );
        let display_tile =
            visible_layer_display_tile_window("metars", metar_zoom, &viewport, width_px, 256.0)
                .into_iter()
                .find(|tile| tile.world_x_offset > 0.0)
                .expect("expected repeated metar world copy");
        let mut metar_tile_cache = HashMap::new();
        metar_tile_cache.insert(
            tile_key(
                &display_tile.request.layer,
                display_tile.request.z,
                display_tile.request.x,
                display_tile.request.y,
            ),
            MetarTilePayload {
                schema_version: 1,
                layer: "metars".to_string(),
                z: display_tile.request.z,
                x: display_tile.request.x,
                y: display_tile.request.y,
                records: vec![MetarTileRecord {
                    kind: "metar".to_string(),
                    id: "KAAA".to_string(),
                }],
            },
        );
        let mut metars_by_station = HashMap::new();
        metars_by_station.insert(
            "KAAA".to_string(),
            MetarRecord {
                raw_text: "METAR KAAA 010000Z 00000KT 10SM CLR 10/08 A3000".to_string(),
                observed_at_utc: Some("2026-05-03T00:00:00.000Z".to_string()),
                station_id: "KAAA".to_string(),
                flight_category: Some("VFR".to_string()),
                clouds: None,
                longitude: 0.0,
                latitude: 0.0,
            },
        );
        let metars = MetarProductPayload {
            schema_version: 3,
            version_label: "test".to_string(),
            generated_at_utc: None,
            observed_at_utc: None,
            metar_count: Some(1),
            metars_by_station,
        };
        let vector_tiles = HashMap::new();
        let airspaces = HashMap::new();
        let aliases = WeatherStationAirportAliases::default();
        let mut availability = |_: &str| AirportPlateAvailability::default();
        let result = query_map_selection(
            &viewport,
            width_px,
            256.0,
            MapSelectionQuery {
                metar_payload: Some(&metars),
                ..MapSelectionQuery::new(
                    &config,
                    LatLon {
                        lat: 0.0,
                        lon: 360.0,
                    },
                    &vector_tiles,
                    &metar_tile_cache,
                    &airspaces,
                    &aliases,
                    &mut availability,
                )
            },
        );
        let weather = result
            .categories
            .iter()
            .find(|category| category.id == "weather")
            .expect("weather category");

        assert_eq!(
            weather.items.first().map(|item| item.label.as_str()),
            Some("KAAA")
        );
    }

    #[test]
    fn map_selection_hits_offline_region_polygon() {
        let region = OfflineRegionRecord {
            id: "chart:nw".to_string(),
            kind: "chart".to_string(),
            region_id: "nw".to_string(),
            label: "NW Charts".to_string(),
            color_key: "ifr_low_blue".to_string(),
            summary: vec![OfflineRegionSummaryEntry {
                action: "available".to_string(),
                cycle: "2604".to_string(),
                count: 2,
            }],
            polygons: vec![vec![
                LatLon {
                    lat: -1.0,
                    lon: -1.0,
                },
                LatLon {
                    lat: -1.0,
                    lon: 1.0,
                },
                LatLon { lat: 1.0, lon: 1.0 },
                LatLon {
                    lat: 1.0,
                    lon: -1.0,
                },
            ]],
            label_position: LatLon { lat: 0.0, lon: 0.0 },
        };
        let mut nw_plate_region = region.clone();
        nw_plate_region.id = "plate:nw".to_string();
        nw_plate_region.kind = "plate".to_string();
        nw_plate_region.label = "NW Plates".to_string();
        let mut sw_plate_region = nw_plate_region.clone();
        sw_plate_region.id = "plate:sw".to_string();
        sw_plate_region.region_id = "sw".to_string();
        sw_plate_region.label = "SW Plates".to_string();
        let regions = vec![region, nw_plate_region, sw_plate_region];
        let config = test_map_overlay_config();
        let vector_tiles = HashMap::new();
        let metar_tiles = HashMap::new();
        let airspaces = HashMap::new();
        let aliases = WeatherStationAirportAliases::default();
        let mut availability = |_: &str| AirportPlateAvailability::default();
        let result = query_map_selection(
            &MapViewport {
                center: LatLon { lat: 0.0, lon: 0.0 },
                zoom: 4.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            1024.0,
            768.0,
            MapSelectionQuery {
                offline_region_records: &regions,
                ..MapSelectionQuery::new(
                    &config,
                    LatLon { lat: 0.0, lon: 0.0 },
                    &vector_tiles,
                    &metar_tiles,
                    &airspaces,
                    &aliases,
                    &mut availability,
                )
            },
        );
        let offline = result
            .categories
            .iter()
            .find(|category| category.id == "offline")
            .expect("offline category");
        assert_eq!(
            offline
                .items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["NW", "SW"]
        );
        let item = offline.items.first().expect("offline item");

        assert_eq!(item.label, "NW");
        assert_eq!(item.sublabel, "NW Charts, NW Plates");
        assert_eq!(
            item.detail_text.as_deref(),
            Some("NW Charts\navailable 2604 (2)\n\nNW Plates\navailable 2604 (2)")
        );
        assert!(matches!(
            item.highlight,
            MapSelectionHighlight::OfflineRegion { ref id } if id == "chart:nw"
        ));
        assert!(item
            .actions
            .iter()
            .any(|action| action.id == "offline_region_mode"));
        assert!(item
            .actions
            .iter()
            .any(|action| action.id == "offline_packages"));
    }

    #[test]
    fn airspace_selection_uses_descriptive_name_as_label() {
        let feature = AirspaceFeaturePayload {
            schema_version: 1,
            id: "airspace:sua:nhanford".to_string(),
            kind: "airspace".to_string(),
            name: "HANFORD NSA, WA".to_string(),
            ident: "NHANFORD".to_string(),
            airspace_class: "NSA".to_string(),
            style_hint: "national_security".to_string(),
            vertical: test_airspace_vertical("UNL", "SFC"),
            bbox: [-120.0, 46.0, -119.0, 47.0],
            paths: vec![test_airspace_path(
                true,
                None,
                vec![[-120.0, 46.0], [-119.0, 46.0], [-119.0, 47.0]],
            )],
        };

        let item = selection_item_for_airspace(&feature);

        assert_eq!(item.label, "HANFORD NSA, WA");
        assert_eq!(item.sublabel, "NHANFORD");
        assert_eq!(item.description, None);
        assert_eq!(
            item.actions
                .iter()
                .find(|action| action.id == "limits")
                .and_then(|action| action.airspace_limit.as_ref()),
            Some(&AirspaceLimitGlyph {
                upper: "UNL".to_string(),
                lower: "SFC".to_string(),
                style_key: "national_security".to_string(),
                color_key: "class_c_magenta".to_string(),
            })
        );
    }

    #[test]
    fn controlled_airspace_selection_uses_ident_and_class_as_compact_label() {
        let feature = AirspaceFeaturePayload {
            schema_version: 1,
            id: "airspace:data_2608:d:bfi:class_d:1".to_string(),
            kind: "airspace".to_string(),
            name: "SEATTLE, BOEING FIELD/KING COUNTY INT. AIRPORT CLASS D".to_string(),
            ident: "BFI".to_string(),
            airspace_class: "D".to_string(),
            style_hint: "class_d".to_string(),
            vertical: test_airspace_vertical("25", "SFC"),
            bbox: [-122.4, 47.4, -122.2, 47.6],
            paths: vec![],
        };

        let item = selection_item_for_airspace(&feature);

        assert_eq!(item.label, "BFI D");
        assert_eq!(item.sublabel, "BFI");
        assert_eq!(
            item.description.as_deref(),
            Some("SEATTLE, BOEING FIELD/KING COUNTY INT. AIRPORT CLASS D")
        );
    }

    #[test]
    fn airspace_selection_hits_repeated_world_copy() {
        let viewport = MapViewport {
            center: LatLon { lat: 0.0, lon: 0.0 },
            zoom: 6.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let feature = AirspaceFeaturePayload {
            schema_version: 1,
            id: "airspace:test:wrapped".to_string(),
            kind: "airspace".to_string(),
            name: "WRAPPED AREA".to_string(),
            ident: "WRAP".to_string(),
            airspace_class: "MOA".to_string(),
            style_hint: "moa".to_string(),
            vertical: test_airspace_vertical("100", "SFC"),
            bbox: [-1.0, -1.0, 1.0, 1.0],
            paths: vec![test_airspace_path(
                true,
                None,
                vec![[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]],
            )],
        };
        let mut vector_cache = HashMap::new();
        for tile in visible_layer_tile_window(
            "airspace",
            airspace_reference_zoom(viewport.zoom, &test_map_overlay_config()),
            &viewport,
            1024.0,
            256.0,
        ) {
            vector_cache.insert(
                aggregate_vector_tile_cache_key(tile.z, tile.x, tile.y),
                VectorAggregateTilePayload {
                    airspace_refs: vec![feature.id.clone()],
                    ..empty_test_vector_tile(tile.z, tile.x, tile.y)
                },
            );
        }
        let config = test_map_overlay_config();
        let metar_tiles = HashMap::new();
        let airspaces = HashMap::from([(feature.id.clone(), feature)]);
        let aliases = WeatherStationAirportAliases::default();
        let mut availability = |_: &str| AirportPlateAvailability::default();
        let result = query_map_selection(
            &viewport,
            1024.0,
            256.0,
            MapSelectionQuery::new(
                &config,
                LatLon {
                    lat: 0.0,
                    lon: 360.0,
                },
                &vector_cache,
                &metar_tiles,
                &airspaces,
                &aliases,
                &mut availability,
            ),
        );
        let airspace = result
            .categories
            .iter()
            .find(|category| category.id == "airspace")
            .expect("airspace category");

        assert_eq!(
            airspace.items.first().map(|item| item.label.as_str()),
            Some("WRAPPED AREA")
        );
    }

    #[test]
    fn airspace_selection_ignores_cached_detail_when_viewport_refs_outline() {
        let viewport = MapViewport {
            center: LatLon { lat: 0.0, lon: 0.0 },
            zoom: 6.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let detail = AirspaceFeaturePayload {
            schema_version: 1,
            id: "airspace:test:orlando:bravo:shelf".to_string(),
            kind: "airspace".to_string(),
            name: "ORLANDO CLASS B SHELF".to_string(),
            ident: "MCO".to_string(),
            airspace_class: "B".to_string(),
            style_hint: "class_b".to_string(),
            vertical: test_airspace_vertical("100", "SFC"),
            bbox: [-1.0, -1.0, 1.0, 1.0],
            paths: vec![test_airspace_path(
                true,
                None,
                vec![[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]],
            )],
        };
        let outline = AirspaceFeaturePayload {
            id: "airspace:test:outline:b:mco:class_b".to_string(),
            name: "ORLANDO CLASS B OUTLINE".to_string(),
            ..detail.clone()
        };
        let mut vector_cache = HashMap::new();
        for tile in visible_layer_tile_window(
            "airspace",
            airspace_reference_zoom(viewport.zoom, &test_map_overlay_config()),
            &viewport,
            1024.0,
            256.0,
        ) {
            vector_cache.insert(
                aggregate_vector_tile_cache_key(tile.z, tile.x, tile.y),
                VectorAggregateTilePayload {
                    airspace_refs: vec![outline.id.clone()],
                    ..empty_test_vector_tile(tile.z, tile.x, tile.y)
                },
            );
        }
        let config = test_map_overlay_config();
        let metar_tiles = HashMap::new();
        let airspaces = HashMap::from([(detail.id.clone(), detail), (outline.id.clone(), outline)]);
        let aliases = WeatherStationAirportAliases::default();
        let mut availability = |_: &str| AirportPlateAvailability::default();
        let result = query_map_selection(
            &viewport,
            1024.0,
            256.0,
            MapSelectionQuery::new(
                &config,
                LatLon { lat: 0.0, lon: 0.0 },
                &vector_cache,
                &metar_tiles,
                &airspaces,
                &aliases,
                &mut availability,
            ),
        );

        let airspace = result
            .categories
            .iter()
            .find(|category| category.id == "airspace")
            .expect("airspace category");
        assert!(
            airspace.items.is_empty(),
            "low-zoom outline refs must not make stale cached shelf details selectable"
        );
    }

    #[test]
    fn moa_paths_generate_feather_decorations_and_cap_warning() {
        let viewport = MapViewport {
            center: LatLon { lat: 0.0, lon: 0.0 },
            zoom: 8.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let mut ref_cache = HashMap::new();
        ref_cache.insert(
            airspace_ref_tile_key(8, 128, 128),
            AirspaceReferenceTilePayload {
                schema_version: 1,
                layer: "airspace".to_string(),
                z: 8,
                x: 128,
                y: 128,
                refs: vec!["airspace:test:moa".to_string()],
            },
        );
        let mut feature_cache = HashMap::new();
        feature_cache.insert(
            "airspace:test:moa".to_string(),
            AirspaceFeaturePayload {
                schema_version: 1,
                id: "airspace:test:moa".to_string(),
                kind: "airspace".to_string(),
                name: "TEST MOA".to_string(),
                ident: "TEST".to_string(),
                airspace_class: "MOA".to_string(),
                style_hint: "moa".to_string(),
                vertical: test_airspace_vertical("100", "50"),
                bbox: [-0.1, -0.1, 0.1, 0.1],
                paths: vec![test_airspace_path(
                    true,
                    Some("left".to_string()),
                    vec![[-0.1, -0.1], [0.1, -0.1], [0.1, 0.1], [-0.1, 0.1]],
                )],
            },
        );

        let result = query_map_overlay(
            &viewport,
            1200.0,
            900.0,
            &HashMap::new(),
            &ref_cache,
            &feature_cache,
            &HashMap::new(),
        );
        assert_eq!(result.airspace_paths.len(), 1);
        assert_eq!(result.airspace_paths[0].style.strokes.len(), 1);
        assert_eq!(
            result.airspace_paths[0].style.strokes[0].color_key,
            "class_c_magenta"
        );
        assert_eq!(result.airspace_paths[0].style.strokes[0].width_px, 1.4);
        assert!(result.airspace_paths[0].style.strokes[0].dash_px.is_empty());
        assert!(
            !result.airspace_paths[0].decorations.is_empty(),
            "MOA should include feather decorations"
        );
        assert!(
            !result.airspace_paths[0].decorations[0].segments.is_empty(),
            "feathers should be encoded as compact screen segments"
        );
        assert!(result.airspace_paths[0].decorations[0].paths.is_empty());
        assert_eq!(
            result.airspace_paths[0].decorations[0].color_key,
            "class_c_magenta"
        );
    }

    #[test]
    fn feathered_airspace_missing_interior_side_warns_and_skips_feathers() {
        let viewport = MapViewport {
            center: LatLon { lat: 0.0, lon: 0.0 },
            zoom: 8.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let mut ref_cache = HashMap::new();
        ref_cache.insert(
            airspace_ref_tile_key(8, 128, 128),
            AirspaceReferenceTilePayload {
                schema_version: 1,
                layer: "airspace".to_string(),
                z: 8,
                x: 128,
                y: 128,
                refs: vec!["airspace:test:moa".to_string()],
            },
        );
        let mut feature_cache = HashMap::new();
        feature_cache.insert(
            "airspace:test:moa".to_string(),
            AirspaceFeaturePayload {
                schema_version: 1,
                id: "airspace:test:moa".to_string(),
                kind: "airspace".to_string(),
                name: "TEST MOA".to_string(),
                ident: "TEST".to_string(),
                airspace_class: "MOA".to_string(),
                style_hint: "moa".to_string(),
                vertical: test_airspace_vertical("100", "50"),
                bbox: [-0.1, -0.1, 0.1, 0.1],
                paths: vec![test_airspace_path(
                    true,
                    None,
                    vec![[-0.1, -0.1], [0.1, -0.1], [0.1, 0.1], [-0.1, 0.1]],
                )],
            },
        );

        let result = query_map_overlay(
            &viewport,
            1200.0,
            900.0,
            &HashMap::new(),
            &ref_cache,
            &feature_cache,
            &HashMap::new(),
        );

        assert!(result.airspace_paths[0].decorations.is_empty());
        assert!(result
            .data_status_records
            .iter()
            .any(|record| record.id == "map_overlay:airspace_interior_side_contract"));
    }

    #[test]
    fn feathers_accumulate_distance_across_short_segments() {
        let mut points = Vec::new();
        let radius = 40.0;
        for index in 0..64 {
            let angle = (index as f64 / 64.0) * std::f64::consts::TAU;
            points.push(AirspaceScreenPoint {
                x: 100.0 + radius * angle.cos(),
                y: 100.0 + radius * angle.sin(),
            });
        }
        let path = AirspaceDisplaySubpath {
            closed: true,
            interior_side: Some("left".to_string()),
            points,
        };
        let mut budget = AirspaceDecorationBudget::default();
        let feathers =
            airspace_feathers_for_path(&path, AirspaceInteriorSide::Left, &mut budget, None);

        assert!(
            feathers.len() > 20,
            "short segment arcs should still receive regularly-spaced feathers"
        );
    }

    #[test]
    fn feather_direction_uses_declared_interior_side() {
        let path = AirspaceDisplaySubpath {
            closed: true,
            interior_side: Some("left".to_string()),
            points: vec![
                AirspaceScreenPoint { x: 40.0, y: 40.0 },
                AirspaceScreenPoint { x: 60.0, y: 40.0 },
                AirspaceScreenPoint { x: 60.0, y: 60.0 },
                AirspaceScreenPoint { x: 40.0, y: 60.0 },
            ],
        };

        let mut left_budget = AirspaceDecorationBudget::default();
        let left =
            airspace_feathers_for_path(&path, AirspaceInteriorSide::Left, &mut left_budget, None);
        let mut right_budget = AirspaceDecorationBudget::default();
        let right =
            airspace_feathers_for_path(&path, AirspaceInteriorSide::Right, &mut right_budget, None);

        assert!(!left.is_empty());
        assert_eq!(left.len(), right.len());
        assert_eq!(left[0][0], right[0][0]);
        assert_eq!(left[0][1], right[0][1]);
        assert!(
            (left[0][3] - left[0][1]) * (right[0][3] - right[0][1]) < 0.0,
            "right-side feathers should point opposite left-side feathers"
        );
    }

    #[test]
    fn national_security_uses_heavy_dashed_magenta_style() {
        let style = airspace_display_style("national_security");

        assert_eq!(style.fill_color_key, "class_c_magenta");
        assert_eq!(style.strokes.len(), 1);
        assert_eq!(style.strokes[0].color_key, "class_c_magenta");
        assert_eq!(style.strokes[0].width_px, 3.6);
        assert_eq!(style.strokes[0].dash_px, vec![6.0, 4.0]);
        assert_eq!(style.strokes[0].line_cap, "butt");
        assert!(airspace_feather_style("national_security").is_none());
    }

    #[test]
    fn warning_areas_use_blue_feathered_sua_style() {
        let style = airspace_display_style("warning");

        assert_eq!(style.fill_color_key, "class_b_d_blue");
        assert_eq!(style.strokes.len(), 1);
        assert_eq!(style.strokes[0].color_key, "class_b_d_blue");
        assert_eq!(style.strokes[0].width_px, 1.4);
        assert!(style.strokes[0].dash_px.is_empty());
        assert_eq!(style.strokes[0].line_cap, "butt");
        assert_eq!(
            airspace_feather_style("warning"),
            Some(("class_b_d_blue".to_string(), 1.4))
        );
    }

    #[test]
    fn tfr_overlay_emits_fraction_label_at_polygon_centroid() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.0,
                lon: -122.0,
            },
            zoom: 10.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let width_px = 1200.0;
        let height_px = 900.0;
        let metrics = MapSurfaceMetrics::new(viewport, width_px, height_px, 1.0);
        let payload = TfrProductPayload {
            schema_version: 1,
            version_label: "test".to_string(),
            generated_at_utc: None,
            notam_count: 1,
            area_group_count: 1,
            areas: vec![TfrAreaPayload {
                notam_id: "1/2345".to_string(),
                area_index: 0,
                schedule_fragments: Vec::new(),
                upper_limit: TfrAltitudeLimit {
                    value_text: "180".to_string(),
                    unit: "FL".to_string(),
                },
                lower_limit: TfrAltitudeLimit {
                    value_text: "0".to_string(),
                    unit: "FT".to_string(),
                },
                polygon: vec![
                    TfrLatLonPoint {
                        lat: 47.08,
                        lon: -122.08,
                    },
                    TfrLatLonPoint {
                        lat: 47.08,
                        lon: -121.92,
                    },
                    TfrLatLonPoint {
                        lat: 46.92,
                        lon: -121.92,
                    },
                    TfrLatLonPoint {
                        lat: 46.92,
                        lon: -122.08,
                    },
                ],
                summary_text: String::new(),
                notam: None,
            }],
        };

        let result = query_tfr_overlay(
            &MapProjectionContext::new(&metrics),
            TfrOverlayInput {
                payload: Some(&payload),
                point_features: &[],
                protected_point_features: &[],
                reference_utc: crate::freshness::parse_utc_instant("2026-07-11T12:00:00Z"),
            },
        );

        assert_eq!(result.paths.len(), 1);
        assert_eq!(result.labels.len(), 1);
        assert_eq!(result.paths[0].style_key, TFR_ACTIVE_STYLE_KEY);
        assert_eq!(result.labels[0].glyph.style_key, TFR_ACTIVE_STYLE_KEY);
        assert_eq!(result.labels[0].glyph.upper, "FL180");
        assert_eq!(result.labels[0].glyph.lower, "SFC");
        assert!((result.labels[0].screen_x - width_px / 2.0).abs() < 1.0);
        assert!((result.labels[0].screen_y - height_px / 2.0).abs() < 1.0);
    }

    #[test]
    fn tfr_overlay_offsets_fraction_label_from_centered_point_symbol() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.0,
                lon: -122.0,
            },
            zoom: 10.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let width_px = 1200.0;
        let height_px = 900.0;
        let metrics = MapSurfaceMetrics::new(viewport, width_px, height_px, 1.0);
        let payload = TfrProductPayload {
            schema_version: 1,
            version_label: "test".to_string(),
            generated_at_utc: None,
            notam_count: 1,
            area_group_count: 1,
            areas: vec![TfrAreaPayload {
                notam_id: "1/2345".to_string(),
                area_index: 0,
                schedule_fragments: Vec::new(),
                upper_limit: TfrAltitudeLimit {
                    value_text: "180".to_string(),
                    unit: "FL".to_string(),
                },
                lower_limit: TfrAltitudeLimit {
                    value_text: "0".to_string(),
                    unit: "FT".to_string(),
                },
                polygon: vec![
                    TfrLatLonPoint {
                        lat: 47.08,
                        lon: -122.08,
                    },
                    TfrLatLonPoint {
                        lat: 47.08,
                        lon: -121.92,
                    },
                    TfrLatLonPoint {
                        lat: 46.92,
                        lon: -121.92,
                    },
                    TfrLatLonPoint {
                        lat: 46.92,
                        lon: -122.08,
                    },
                ],
                summary_text: String::new(),
                notam: None,
            }],
        };
        let airport = test_visible_feature(
            "airports:KPWT",
            "airport",
            "airport",
            "PWT",
            width_px / 2.0,
            height_px / 2.0,
        );
        let obstacle_rect = point_feature_symbol_rect(&airport, 1.0)
            .expect("airport symbol rect")
            .padded(LABEL_COLLISION_PADDING_PX);
        let centroid_label = AirspaceDisplayLabel {
            feature_id: "tfr:1/2345:0".to_string(),
            glyph: AirspaceLimitGlyph {
                upper: "FL180".to_string(),
                lower: "SFC".to_string(),
                style_key: TFR_ACTIVE_STYLE_KEY.to_string(),
                color_key: TFR_ACTIVE_STYLE_KEY.to_string(),
            },
            screen_x: width_px / 2.0,
            screen_y: height_px / 2.0,
        };
        assert!(
            airspace_label_rect(&centroid_label, 1.0)
                .expect("centroid label rect")
                .padded(LABEL_COLLISION_PADDING_PX)
                .overlaps(obstacle_rect),
            "centroid TFR label should reproduce the airport-symbol overlap"
        );

        let result = query_tfr_overlay(
            &MapProjectionContext::new(&metrics),
            TfrOverlayInput {
                payload: Some(&payload),
                point_features: std::slice::from_ref(&airport),
                protected_point_features: &[],
                reference_utc: crate::freshness::parse_utc_instant("2026-07-11T12:00:00Z"),
            },
        );

        assert_eq!(result.paths.len(), 1);
        assert_eq!(result.labels.len(), 1);
        assert!(
            (result.labels[0].screen_x - width_px / 2.0).abs() > 1.0
                || (result.labels[0].screen_y - height_px / 2.0).abs() > 1.0,
            "TFR label should be moved away from the airport symbol at polygon center"
        );
        assert!(
            !airspace_label_rect(&result.labels[0], 1.0)
                .expect("shifted label rect")
                .padded(LABEL_COLLISION_PADDING_PX)
                .overlaps(obstacle_rect),
            "shifted TFR label should not cover the airport symbol"
        );

        let mut visible_features = vec![airport];
        let mut airspace_labels = result.labels.clone();
        suppress_overlapping_vector_labels(&mut visible_features, &mut airspace_labels, &[], 1.0);
        assert_eq!(
            airspace_labels.len(),
            1,
            "shifted TFR label should survive the shared label collision pass"
        );
    }

    #[test]
    fn tfr_overlay_marks_future_tfrs_as_upcoming() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.0,
                lon: -122.0,
            },
            zoom: 10.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let width_px = 1200.0;
        let height_px = 900.0;
        let metrics = MapSurfaceMetrics::new(viewport, width_px, height_px, 1.0);
        let payload = TfrProductPayload {
            schema_version: 1,
            version_label: "test".to_string(),
            generated_at_utc: None,
            notam_count: 1,
            area_group_count: 1,
            areas: vec![TfrAreaPayload {
                notam_id: "1/2345".to_string(),
                area_index: 0,
                schedule_fragments: Vec::new(),
                upper_limit: TfrAltitudeLimit {
                    value_text: "18000".to_string(),
                    unit: "FT MSL".to_string(),
                },
                lower_limit: TfrAltitudeLimit {
                    value_text: "SFC".to_string(),
                    unit: "FT MSL".to_string(),
                },
                polygon: vec![
                    TfrLatLonPoint {
                        lat: 47.08,
                        lon: -122.08,
                    },
                    TfrLatLonPoint {
                        lat: 47.08,
                        lon: -121.92,
                    },
                    TfrLatLonPoint {
                        lat: 46.92,
                        lon: -121.92,
                    },
                    TfrLatLonPoint {
                        lat: 46.92,
                        lon: -122.08,
                    },
                ],
                summary_text: String::new(),
                notam: Some(TfrNotamMetadata {
                    record_id: "fdc:1/2345".to_string(),
                    source_type: Some("FDC".to_string()),
                    status: Some("PUBLISHED".to_string()),
                    function: None,
                    keyword: Some("AIRSPACE".to_string()),
                    facility: Some("ZLC".to_string()),
                    issued_utc: None,
                    effective_start_utc: Some("2026-07-11T15:30:00Z".to_string()),
                    effective_end_utc: None,
                    text: None,
                    local_text: None,
                    icao_text: None,
                }),
            }],
        };

        let result = query_tfr_overlay(
            &MapProjectionContext::new(&metrics),
            TfrOverlayInput {
                payload: Some(&payload),
                point_features: &[],
                protected_point_features: &[],
                reference_utc: crate::freshness::parse_utc_instant("2026-07-11T12:00:00Z"),
            },
        );

        assert_eq!(result.paths.len(), 1);
        assert_eq!(result.labels.len(), 1);
        assert_eq!(result.paths[0].style_key, TFR_UPCOMING_STYLE_KEY);
        assert_eq!(result.paths[0].style.fill_color_key, "tfr_upcoming");
        assert_eq!(result.labels[0].glyph.style_key, TFR_UPCOMING_STYLE_KEY);
        assert_eq!(result.labels[0].glyph.color_key, "tfr_upcoming");
    }

    #[test]
    fn tfr_selection_exposes_text_as_modal_action_not_inline_detail() {
        let area = TfrAreaPayload {
            notam_id: "1/2345".to_string(),
            area_index: 0,
            schedule_fragments: Vec::new(),
            upper_limit: TfrAltitudeLimit {
                value_text: "12000".to_string(),
                unit: "FT MSL".to_string(),
            },
            lower_limit: TfrAltitudeLimit {
                value_text: "SFC".to_string(),
                unit: "FT MSL".to_string(),
            },
            polygon: vec![
                TfrLatLonPoint {
                    lat: 47.08,
                    lon: -122.08,
                },
                TfrLatLonPoint {
                    lat: 47.08,
                    lon: -121.92,
                },
                TfrLatLonPoint {
                    lat: 46.92,
                    lon: -121.92,
                },
            ],
            summary_text: String::new(),
            notam: Some(TfrNotamMetadata {
                record_id: "fdc:1/2345".to_string(),
                source_type: Some("FDC".to_string()),
                status: Some("PUBLISHED".to_string()),
                function: None,
                keyword: Some("AIRSPACE".to_string()),
                facility: Some("ZLC".to_string()),
                issued_utc: None,
                effective_start_utc: Some("2026-07-11T12:00:00Z".to_string()),
                effective_end_utc: Some("2026-07-27T12:30:00Z".to_string()),
                text: Some("FDC 1/2345 LONG TFR TEXT".to_string()),
                local_text: None,
                icao_text: None,
            }),
        };

        let item = selection_item_for_tfr(
            &area,
            crate::freshness::parse_utc_instant("2026-07-11T12:30:00Z"),
            chrono_tz::America::Los_Angeles,
            crate::TimeDisplayMode::Local,
        );

        assert_eq!(item.detail_text, None);
        let text_action = item
            .actions
            .iter()
            .find(|action| action.id == "tfr_text")
            .expect("TFR text action");
        assert_eq!(text_action.label, "Text");
        assert_eq!(text_action.detail_title.as_deref(), Some("TFR"));
        assert!(text_action.enabled);
        assert_eq!(
            text_action.detail_text.as_deref(),
            Some("FDC 1/2345 LONG TFR TEXT")
        );
        assert_eq!(
            text_action.detail_status.as_ref(),
            Some(&MapSelectionDetailStatus {
                text: "Active now; ends in 16d (Mon Jul 27 5:30am PDT)".to_string(),
                color_key: TFR_ACTIVE_STYLE_KEY.to_string(),
                action_id: Some(crate::TOGGLE_TIME_DISPLAY_MODE_ACTION_ID.to_string()),
            })
        );
    }

    #[test]
    fn tfr_modal_timing_uses_same_one_hour_color_threshold_as_map_rendering() {
        let area = TfrAreaPayload {
            notam_id: "1/2345".to_string(),
            area_index: 0,
            schedule_fragments: Vec::new(),
            upper_limit: TfrAltitudeLimit {
                value_text: "12000".to_string(),
                unit: "FT MSL".to_string(),
            },
            lower_limit: TfrAltitudeLimit {
                value_text: "SFC".to_string(),
                unit: "FT MSL".to_string(),
            },
            polygon: Vec::new(),
            summary_text: String::new(),
            notam: Some(TfrNotamMetadata {
                record_id: "fdc:1/2345".to_string(),
                source_type: Some("FDC".to_string()),
                status: Some("PUBLISHED".to_string()),
                function: None,
                keyword: Some("AIRSPACE".to_string()),
                facility: Some("ZLC".to_string()),
                issued_utc: None,
                effective_start_utc: Some("2026-07-27T19:00:00Z".to_string()),
                effective_end_utc: Some("2026-07-28T19:00:00Z".to_string()),
                text: Some("TFR text".to_string()),
                local_text: None,
                icao_text: None,
            }),
        };
        let reference = crate::freshness::parse_utc_instant("2026-07-24T19:00:00Z");
        assert_eq!(
            tfr_timing_detail_status(
                &area,
                reference,
                chrono_tz::America::Los_Angeles,
                crate::TimeDisplayMode::Local,
            ),
            Some(MapSelectionDetailStatus {
                text: "Starts in 3d (Mon Jul 27 12:00pm PDT)".to_string(),
                color_key: TFR_UPCOMING_STYLE_KEY.to_string(),
                action_id: Some(crate::TOGGLE_TIME_DISPLAY_MODE_ACTION_ID.to_string()),
            })
        );

        let within_one_hour = crate::freshness::parse_utc_instant("2026-07-27T18:30:00Z");
        assert_eq!(
            tfr_timing_detail_status(
                &area,
                within_one_hour,
                chrono_tz::America::Los_Angeles,
                crate::TimeDisplayMode::Local,
            ),
            Some(MapSelectionDetailStatus {
                text: "Starts in 30m (Mon Jul 27 12:00pm PDT)".to_string(),
                color_key: TFR_ACTIVE_STYLE_KEY.to_string(),
                action_id: Some(crate::TOGGLE_TIME_DISPLAY_MODE_ACTION_ID.to_string()),
            })
        );
    }

    #[test]
    fn tfr_overlay_elides_fraction_label_when_polygon_is_too_small() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.0,
                lon: -122.0,
            },
            zoom: 8.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let width_px = 1200.0;
        let height_px = 900.0;
        let metrics = MapSurfaceMetrics::new(viewport, width_px, height_px, 1.0);
        let payload = TfrProductPayload {
            schema_version: 1,
            version_label: "test".to_string(),
            generated_at_utc: None,
            notam_count: 1,
            area_group_count: 1,
            areas: vec![TfrAreaPayload {
                notam_id: "1/2345".to_string(),
                area_index: 0,
                schedule_fragments: Vec::new(),
                upper_limit: TfrAltitudeLimit {
                    value_text: "18000".to_string(),
                    unit: "FT MSL".to_string(),
                },
                lower_limit: TfrAltitudeLimit {
                    value_text: "SFC".to_string(),
                    unit: "FT MSL".to_string(),
                },
                polygon: vec![
                    TfrLatLonPoint {
                        lat: 47.001,
                        lon: -122.001,
                    },
                    TfrLatLonPoint {
                        lat: 47.001,
                        lon: -121.999,
                    },
                    TfrLatLonPoint {
                        lat: 46.999,
                        lon: -121.999,
                    },
                    TfrLatLonPoint {
                        lat: 46.999,
                        lon: -122.001,
                    },
                ],
                summary_text: String::new(),
                notam: None,
            }],
        };

        let result = query_tfr_overlay(
            &MapProjectionContext::new(&metrics),
            TfrOverlayInput {
                payload: Some(&payload),
                point_features: &[],
                protected_point_features: &[],
                reference_utc: crate::freshness::parse_utc_instant("2026-07-11T12:00:00Z"),
            },
        );

        assert_eq!(result.paths.len(), 1);
        assert!(result.labels.is_empty());
    }

    #[test]
    fn omits_over_budget_fix_bucket_as_unit() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.36,
                lon: -121.98,
            },
            zoom: 10.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let window =
            visible_point_tile_window(&test_map_overlay_config(), &viewport, 1200.0, 900.0);
        let first = window
            .iter()
            .find(|tile| tile.layer == "fix")
            .expect("expected visible tile");
        let mut cache = HashMap::new();
        cache.insert(
            tile_key(&first.layer, first.z, first.x, first.y),
            PointTilePayload {
                schema_version: 1,
                layer: first.layer.clone(),
                z: first.z,
                x: first.x,
                y: first.y,
                records: (0..(VECTOR_DISPLAY_FEATURE_LIMIT + 5))
                    .map(|index| test_point_record(format!("fix:{index}"), "yrep-pt", "fix"))
                    .collect(),
            },
        );
        let result = query_map_overlay(
            &viewport,
            1200.0,
            900.0,
            &cache,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );
        assert!(result.visible_features.is_empty());
        assert!(result.data_status_records.iter().any(|record| {
            record.id == "map_overlay:vector_display_feature_limit"
                && record.drives_caution
                && record.severity == UiStatusSeverity::Warning
                && record
                    .detail
                    .contains("omitted lower-priority features: fix=505")
        }));
    }

    #[test]
    fn prioritizes_nav_over_over_budget_fixes() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.36,
                lon: -121.98,
            },
            zoom: 10.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let window =
            visible_point_tile_window(&test_map_overlay_config(), &viewport, 1200.0, 900.0);
        let fix_tile = window
            .iter()
            .find(|tile| tile.layer == "fix")
            .expect("expected visible fix tile");
        let nav_tile = window
            .iter()
            .find(|tile| tile.layer == "nav")
            .expect("expected visible nav tile");
        let mut cache = HashMap::new();
        cache.insert(
            tile_key(&fix_tile.layer, fix_tile.z, fix_tile.x, fix_tile.y),
            PointTilePayload {
                schema_version: 1,
                layer: fix_tile.layer.clone(),
                z: fix_tile.z,
                x: fix_tile.x,
                y: fix_tile.y,
                records: (0..VECTOR_DISPLAY_FEATURE_LIMIT)
                    .map(|index| test_point_record(format!("fix:{index}"), "yrep-pt", "fix"))
                    .collect(),
            },
        );
        cache.insert(
            tile_key(&nav_tile.layer, nav_tile.z, nav_tile.x, nav_tile.y),
            PointTilePayload {
                schema_version: 1,
                layer: nav_tile.layer.clone(),
                z: nav_tile.z,
                x: nav_tile.x,
                y: nav_tile.y,
                records: (0..3)
                    .map(|index| test_point_record(format!("nav:{index}:VOR"), "VOR", "nav"))
                    .collect(),
            },
        );

        let result = query_map_overlay(
            &viewport,
            1200.0,
            900.0,
            &cache,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );

        assert_eq!(result.visible_features.len(), 3);
        assert!(result
            .visible_features
            .iter()
            .all(|feature| feature.style_class == "nav"));
        assert!(result.data_status_records.iter().any(|record| {
            record.id == "map_overlay:vector_display_feature_limit"
                && record.detail.contains("fix=500")
        }));
    }

    #[test]
    fn suppresses_lower_drawn_overlapping_point_labels() {
        let mut features = vec![
            test_visible_feature("airports:KABC", "airport", "airport", "KABC", 100.0, 100.0),
            test_visible_feature("nav:ABC:VOR", "VORTAC", "nav", "ABC", 100.0, 100.0),
        ];
        let mut airspace_labels = Vec::new();

        suppress_overlapping_vector_labels(&mut features, &mut airspace_labels, &[], 1.0);

        assert_eq!(features[0].label, "");
        assert_eq!(features[1].label, "ABC");
    }

    #[test]
    fn airport_labels_suppress_colocated_fix_labels() {
        let mut features = vec![
            test_visible_feature("airports:KPAE", "airport", "airport", "KPAE", 100.0, 100.0),
            test_visible_feature("fix:KONAH", "fix", "fix", "KONAH", 100.0, 100.0),
        ];
        let mut airspace_labels = Vec::new();

        suppress_overlapping_vector_labels(&mut features, &mut airspace_labels, &[], 1.0);

        assert_eq!(features[0].label, "KPAE");
        assert_eq!(features[1].label, "");
    }

    #[test]
    fn flight_plan_labels_protect_against_vector_labels() {
        let mut features = vec![test_visible_feature(
            "nav:PAE:VOR",
            "VOR/DME",
            "nav",
            "PAE",
            100.0,
            100.0,
        )];
        let mut flight_plan_feature = test_visible_feature(
            "flight-plan:airport:KPAE",
            "airport",
            "airport",
            "KPAE",
            100.0,
            100.0,
        );
        flight_plan_feature.label_style = VectorIdentLabelStyle::FlightPlan;
        let protected = vec![flight_plan_feature];
        let mut airspace_labels = Vec::new();

        suppress_overlapping_vector_labels(&mut features, &mut airspace_labels, &protected, 1.0);

        assert_eq!(features[0].label, "");
        assert_eq!(protected[0].label, "KPAE");
    }

    #[test]
    fn suppresses_airspace_label_under_point_label() {
        let mut features = vec![test_visible_feature(
            "nav:ABC:VOR",
            "VORTAC",
            "nav",
            "ABC",
            100.0,
            124.0,
        )];
        let mut airspace_labels = vec![AirspaceDisplayLabel {
            feature_id: "airspace:a".to_string(),
            glyph: AirspaceLimitGlyph {
                upper: "100".to_string(),
                lower: "50".to_string(),
                style_key: "class_b".to_string(),
                color_key: "class_b_d_blue".to_string(),
            },
            screen_x: 100.0,
            screen_y: 100.0,
        }];

        suppress_overlapping_vector_labels(&mut features, &mut airspace_labels, &[], 1.0);

        assert!(airspace_labels.is_empty());
        assert_eq!(features[0].label, "ABC");
    }

    #[test]
    fn scaled_point_label_rects_match_android_drawn_density() {
        let mut features = vec![
            test_visible_feature("fix:LONGA", "fix", "fix", "LONGA", 100.0, 100.0),
            test_visible_feature("fix:LONGB", "fix", "fix", "LONGB", 100.0, 130.0),
        ];
        let mut airspace_labels = Vec::new();

        suppress_overlapping_vector_labels(&mut features, &mut airspace_labels, &[], 1.0);
        assert_eq!(features[0].label, "LONGA");
        assert_eq!(features[1].label, "LONGB");

        let mut features = vec![
            test_visible_feature("fix:LONGA", "fix", "fix", "LONGA", 100.0, 100.0),
            test_visible_feature("fix:LONGB", "fix", "fix", "LONGB", 100.0, 130.0),
        ];
        suppress_overlapping_vector_labels(&mut features, &mut airspace_labels, &[], 3.0);

        assert_eq!(features[0].label, "");
        assert_eq!(features[1].label, "LONGB");
    }

    #[test]
    fn scaled_airspace_label_rects_match_android_drawn_density() {
        let features = Vec::new();
        let airspace_label = AirspaceDisplayLabel {
            feature_id: "airspace:a".to_string(),
            glyph: AirspaceLimitGlyph {
                upper: "100".to_string(),
                lower: "50".to_string(),
                style_key: "class_b".to_string(),
                color_key: "class_b_d_blue".to_string(),
            },
            screen_x: 100.0,
            screen_y: 100.0,
        };
        let mut airspace_labels = vec![
            airspace_label.clone(),
            AirspaceDisplayLabel {
                feature_id: "airspace:b".to_string(),
                screen_y: 140.0,
                ..airspace_label
            },
        ];
        let mut unscaled_features = features.clone();
        suppress_overlapping_vector_labels(&mut unscaled_features, &mut airspace_labels, &[], 1.0);
        assert_eq!(airspace_labels.len(), 2);

        let mut airspace_labels = vec![
            AirspaceDisplayLabel {
                feature_id: "airspace:a".to_string(),
                glyph: AirspaceLimitGlyph {
                    upper: "100".to_string(),
                    lower: "50".to_string(),
                    style_key: "class_b".to_string(),
                    color_key: "class_b_d_blue".to_string(),
                },
                screen_x: 100.0,
                screen_y: 100.0,
            },
            AirspaceDisplayLabel {
                feature_id: "airspace:b".to_string(),
                glyph: AirspaceLimitGlyph {
                    upper: "100".to_string(),
                    lower: "50".to_string(),
                    style_key: "class_b".to_string(),
                    color_key: "class_b_d_blue".to_string(),
                },
                screen_x: 100.0,
                screen_y: 140.0,
            },
        ];
        let mut scaled_features = features;
        suppress_overlapping_vector_labels(&mut scaled_features, &mut airspace_labels, &[], 3.0);

        assert_eq!(airspace_labels.len(), 1);
        assert_eq!(airspace_labels[0].feature_id, "airspace:b");
    }

    #[test]
    fn map_selection_returns_point_and_spot_categories() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.36,
                lon: -121.98,
            },
            zoom: 10.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let airport_tile =
            visible_point_tile_window(&test_map_overlay_config(), &viewport, 1200.0, 900.0)
                .into_iter()
                .find(|tile| tile.layer == "airport")
                .expect("expected airport tile");
        let nearest_airport = PointVectorRecord {
            id: "airports:KSEA".to_string(),
            kind: "airport".to_string(),
            lat: 47.36,
            lon: -121.98,
            label: "SEATTLE".to_string(),
            location_label: Some("Seattle, WA".to_string()),
            style_class: "airport".to_string(),
            towered: Some(true),
            fuel_available: Some(true),
            public_use: Some(true),
            private_use: Some(false),
            has_paved_runway: Some(true),
            heliport: Some(false),
            has_water_runway: Some(false),
            longest_runway_length_ft: Some(10000.0),
            longest_runway_heading_true_deg: Some(160.0),
            elevation_msl_ft: Some(433.0),
            obstacle: None,
            weather_camera: None,
        };
        let mut farther_airport = nearest_airport.clone();
        farther_airport.id = "airports:KBFI".to_string();
        farther_airport.label = "BOEING FIELD".to_string();
        farther_airport.lon += 0.01;
        let mut cache = HashMap::new();
        cache.insert(
            tile_key(
                &airport_tile.layer,
                airport_tile.z,
                airport_tile.x,
                airport_tile.y,
            ),
            PointTilePayload {
                schema_version: 1,
                layer: airport_tile.layer.clone(),
                z: airport_tile.z,
                x: airport_tile.x,
                y: airport_tile.y,
                records: vec![farther_airport, nearest_airport],
            },
        );
        let plan = FlightPlan {
            id: "plan".to_string(),
            name: "Plan".to_string(),
            route_components: Vec::new(),
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let vector_cache = aggregate_test_vector_tiles(&cache, &HashMap::new(), &HashMap::new());
        let config = test_map_overlay_config();
        let metar_tiles = HashMap::new();
        let airspaces = HashMap::new();
        let aliases = WeatherStationAirportAliases::default();
        let mut availability = |_: &str| AirportPlateAvailability {
            plates: true,
            csup: true,
        };
        let result = query_map_selection(
            &viewport,
            1200.0,
            900.0,
            MapSelectionQuery {
                plan: Some(&plan),
                ..MapSelectionQuery::new(
                    &config,
                    viewport.center,
                    &vector_cache,
                    &metar_tiles,
                    &airspaces,
                    &aliases,
                    &mut availability,
                )
            },
        );

        assert_eq!(result.categories[0].id, "airport");
        assert_eq!(result.categories[0].items[0].label, "KSEA");
        assert_eq!(
            result.categories[0].items[0].description.as_deref(),
            Some("SEATTLE")
        );
        assert_eq!(
            result.categories[0].items[0]
                .secondary_description
                .as_deref(),
            Some("Seattle, WA")
        );
        assert_eq!(result.categories[0].items[0].elevation_msl_ft, Some(433.0));
        assert_eq!(
            result.initial_selected_item_id.as_deref(),
            Some(result.categories[0].items[0].id.as_str())
        );
        assert!(!result.categories[0].items[0]
            .actions
            .iter()
            .any(|action| action.id == "elevation"));
        let airport_info = result.categories[0].items[0]
            .actions
            .iter()
            .find(|action| action.id == "airport_info")
            .expect("airport info action");
        assert!(airport_info.enabled);
        assert_eq!(
            airport_info.airport_info_airport_id.as_deref(),
            Some("KSEA")
        );
        assert_eq!(result.categories[1].id, "navaid");
        let spot = result.categories[1]
            .items
            .iter()
            .find(|item| item.id.starts_with("spot:"))
            .expect("spot selection item");
        let spot_coordinates = format!("{:.4}, {:.4}", viewport.center.lat, viewport.center.lon);
        assert_eq!(spot.description, None);
        assert_eq!(
            spot.secondary_description.as_deref(),
            Some(spot_coordinates.as_str())
        );
        assert_eq!(spot.detail_text, None);
        let spot_nav_ref = NavRef::Spot(viewport.center);
        assert_eq!(spot.nav_ref, Some(spot_nav_ref.clone()));
        let direct_to = spot
            .actions
            .iter()
            .find(|action| action.id == "direct_to")
            .expect("direct-to action");
        assert!(direct_to.enabled);
        assert_eq!(
            serde_json::from_str::<MapSelectionSessionAction>(
                direct_to
                    .session_action
                    .as_deref()
                    .expect("direct-to session action"),
            )
            .expect("session action decodes"),
            MapSelectionSessionAction::ActivateDirectToNavRef {
                nav_ref: spot_nav_ref.clone(),
            }
        );
        let insert = spot
            .actions
            .iter()
            .find(|action| action.id == "insert")
            .expect("insert action");
        assert!(insert.enabled);
        assert_eq!(
            serde_json::from_str::<MapSelectionSessionAction>(
                insert
                    .session_action
                    .as_deref()
                    .expect("insert session action"),
            )
            .expect("session action decodes"),
            MapSelectionSessionAction::InsertWaypointBestPosition {
                nav_ref: spot_nav_ref,
            }
        );
        let overlay_plan = FlightPlan {
            guidance: Some(crate::GuidanceState {
                active_leg_index: 0,
                active_detail_index: None,
                display_split_leg_id: None,
                sequencing_mode: crate::SequencingMode::DirectTo,
                direct_to: Some(crate::DirectToState {
                    start: NavRef::LatLon(viewport.center),
                    target: NavRef::Spot(LatLon {
                        lat: viewport.center.lat + 0.1,
                        lon: viewport.center.lon,
                    }),
                    target_row: crate::DirectToTargetRow::Temporary {
                        row_id: crate::FlightPlanRowId("flight-plan-row:test-spot".to_string()),
                    },
                    resume_row_id: None,
                }),
                suspend_reason: None,
            }),
            ..plan
        };
        let overlay_spot = spot_selection_item(viewport.center, Some(&overlay_plan));
        let overlay_insert = overlay_spot
            .actions
            .iter()
            .find(|action| action.id == "insert")
            .expect("insert action");
        assert!(!overlay_insert.enabled);
        assert_eq!(
            overlay_insert.disabled_reason.as_deref(),
            Some("Restore FP before editing the flight plan.")
        );
        assert!(overlay_insert.session_action.is_none());
        assert_eq!(result.categories[3].id, "weather");
    }

    #[test]
    fn map_selection_exposes_weather_camera_as_weather_with_core_owned_open_action() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.2831,
                lon: -121.3372,
            },
            zoom: 10.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let airport_tile =
            visible_point_tile_window(&test_map_overlay_config(), &viewport, 1200.0, 900.0)
                .into_iter()
                .find(|tile| tile.layer == "airport")
                .expect("expected airport tile");
        let camera = PointVectorRecord {
            id: "weather-camera:150".to_string(),
            kind: "weather camera".to_string(),
            lat: viewport.center.lat,
            lon: viewport.center.lon,
            label: "SAC 150".to_string(),
            location_label: Some("WA".to_string()),
            style_class: "weather_camera".to_string(),
            towered: None,
            fuel_available: None,
            public_use: None,
            private_use: None,
            has_paved_runway: None,
            heliport: None,
            has_water_runway: None,
            longest_runway_length_ft: None,
            longest_runway_heading_true_deg: None,
            elevation_msl_ft: Some(3800.0),
            obstacle: None,
            weather_camera: Some(WeatherCameraPointSemantics {
                site_id: "150".to_string(),
                site_name: "Stampede Pass".to_string(),
                site_identifier: Some("SAC 150".to_string()),
                icao: None,
                page_url: "https://weathercams.faa.gov/cameras/cameraSite/150/summary".to_string(),
                operated_by: Some("FAA".to_string()),
                attribution: None,
                active: Some(true),
                in_maintenance: Some(false),
                third_party: Some(false),
            }),
        };
        let mut point_tiles = HashMap::new();
        point_tiles.insert(
            tile_key(
                &airport_tile.layer,
                airport_tile.z,
                airport_tile.x,
                airport_tile.y,
            ),
            PointTilePayload {
                schema_version: 1,
                layer: airport_tile.layer,
                z: airport_tile.z,
                x: airport_tile.x,
                y: airport_tile.y,
                records: vec![camera],
            },
        );
        let vector_tiles =
            aggregate_test_vector_tiles(&point_tiles, &HashMap::new(), &HashMap::new());
        let config = test_map_overlay_config();
        let metar_tiles = HashMap::new();
        let airspaces = HashMap::new();
        let aliases = WeatherStationAirportAliases::default();
        let mut availability = |_: &str| AirportPlateAvailability::default();

        let selection = query_map_selection(
            &viewport,
            1200.0,
            900.0,
            MapSelectionQuery::new(
                &config,
                viewport.center,
                &vector_tiles,
                &metar_tiles,
                &airspaces,
                &aliases,
                &mut availability,
            ),
        );

        let weather = selection
            .categories
            .iter()
            .find(|category| category.id == "weather")
            .expect("weather category");
        let item = weather.items.first().expect("camera selection item");
        assert_eq!(item.label, "Stampede Pass");
        assert_eq!(item.sublabel, "SAC 150");
        assert_eq!(
            item.symbol_feature.as_ref().unwrap().symbol_kind,
            "weather_camera"
        );
        assert_eq!(
            selection.initial_selected_item_id.as_deref(),
            Some(item.id.as_str())
        );
        let action = item.actions.first().expect("open camera action");
        assert_eq!(action.id, "open_weather_camera");
        assert_eq!(
            action.external_url.as_deref(),
            Some("https://weathercams.faa.gov/cameras/cameraSite/150/summary")
        );
        assert!(serde_json::to_value(action)
            .unwrap()
            .get("external_url")
            .is_none());
    }

    #[test]
    fn map_selection_offers_remove_for_top_level_waypoint_already_in_plan() {
        let record = PointVectorRecord {
            id: "airports:KSEA".to_string(),
            kind: "airport".to_string(),
            lat: 47.36,
            lon: -121.98,
            label: "SEATTLE".to_string(),
            location_label: None,
            style_class: "airport".to_string(),
            towered: Some(true),
            fuel_available: Some(true),
            public_use: Some(true),
            private_use: Some(false),
            has_paved_runway: Some(true),
            heliport: Some(false),
            has_water_runway: Some(false),
            longest_runway_length_ft: Some(10000.0),
            longest_runway_heading_true_deg: Some(160.0),
            elevation_msl_ft: Some(433.0),
            obstacle: None,
            weather_camera: None,
        };
        let symbol = point_vector_record_to_symbol_feature(&record, None).unwrap();
        let plan = FlightPlan {
            id: "plan".to_string(),
            name: "Plan".to_string(),
            route_components: vec![RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KSEA".to_string()),
            }],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let item = selection_item_for_point(
            &record,
            &symbol,
            Some(&plan),
            AirportPlateAvailability {
                plates: true,
                csup: true,
            },
            Some(WeatherDetailUiView {
                station_id: "KSEA".to_string(),
                title: "WX KSEA".to_string(),
                advisory_text: WEATHER_DETAIL_ADVISORY_TEXT.to_string(),
                sections: Vec::new(),
                metar_text: None,
                metar_age_label: None,
                metar_age_warning: false,
                taf_text: Some(
                    "TAF KSEA 010000Z 0100/0124 00000KT P6SM SCT020\nBECMG 0102/0104 BKN030\nFM010600 22008KT P6SM SCT050"
                        .to_string(),
                ),
                taf_age_label: None,
                taf_age_warning: false,
                notams: Vec::new(),
            }),
        );
        let remove = item
            .actions
            .iter()
            .find(|action| action.id == "remove_from_flight_plan")
            .expect("remove action");

        assert_eq!(item.nav_ref, Some(NavRef::Airport("KSEA".to_string())));
        assert_eq!(remove.label, "Remove");
        assert!(remove.enabled);
        assert!(!remove.display_only);
        assert!(remove.flight_plan_row_action.is_some());
        let direct_to = item
            .actions
            .iter()
            .find(|action| action.id == "direct_to")
            .expect("direct-to action");
        assert!(direct_to.enabled);
        assert!(direct_to.flight_plan_row_action.is_some());
        assert!(item
            .actions
            .iter()
            .any(|action| action.id == "plates" && action.enabled));
        assert!(item
            .actions
            .iter()
            .any(|action| action.id == "csup" && action.enabled));
        let wx = item
            .actions
            .iter()
            .find(|action| action.id == "wx")
            .expect("WX action");
        assert!(wx.enabled);
        assert_eq!(
            wx.weather_detail
                .as_ref()
                .and_then(|detail| detail.taf_text.as_deref()),
            Some("TAF KSEA 010000Z 0100/0124 00000KT P6SM SCT020\nBECMG 0102/0104 BKN030\nFM010600 22008KT P6SM SCT050")
        );

        let duplicate_plan = FlightPlan {
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KSEA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KSEA".to_string()),
                },
            ],
            ..plan
        };
        let duplicate_item = selection_item_for_point(
            &record,
            &symbol,
            Some(&duplicate_plan),
            AirportPlateAvailability::default(),
            None,
        );
        let duplicate_remove = duplicate_item
            .actions
            .iter()
            .find(|action| action.id == "remove_from_flight_plan")
            .expect("remove action");
        assert!(!duplicate_remove.enabled);
        assert_eq!(duplicate_remove.label, "Remove");
        assert!(duplicate_remove.flight_plan_row_action.is_none());
        let duplicate_direct_to = duplicate_item
            .actions
            .iter()
            .find(|action| action.id == "direct_to")
            .expect("direct-to action");
        assert!(duplicate_direct_to.enabled);
        assert_eq!(duplicate_direct_to.label, "Direct");
        assert!(duplicate_direct_to.flight_plan_row_action.is_none());

        let off_plan_item = selection_item_for_point(
            &record,
            &symbol,
            Some(&FlightPlan {
                route_components: Vec::new(),
                route_component_uids: Vec::new(),
                route_component_uid_counter: 0,
                ..duplicate_plan
            }),
            AirportPlateAvailability::default(),
            None,
        );
        let insert = off_plan_item
            .actions
            .iter()
            .find(|action| action.id == "insert")
            .expect("insert action");
        assert!(insert.enabled);
        assert_eq!(insert.label, "Insert");
        assert!(insert.flight_plan_row_action.is_none());
        assert_eq!(
            serde_json::from_str::<MapSelectionSessionAction>(
                insert.session_action.as_deref().expect("session action"),
            )
            .expect("session action decodes"),
            MapSelectionSessionAction::InsertWaypointBestPosition {
                nav_ref: NavRef::Airport("KSEA".to_string()),
            }
        );
    }

    #[test]
    fn map_selection_offers_direct_to_and_insert_for_fix_points() {
        let record = PointVectorRecord {
            id: "fix:VAMPS".to_string(),
            kind: "fix".to_string(),
            lat: 47.0,
            lon: -122.0,
            label: "VAMPS".to_string(),
            location_label: None,
            style_class: "fix".to_string(),
            towered: None,
            fuel_available: None,
            public_use: None,
            private_use: None,
            has_paved_runway: None,
            heliport: None,
            has_water_runway: None,
            longest_runway_length_ft: None,
            longest_runway_heading_true_deg: None,
            elevation_msl_ft: None,
            obstacle: None,
            weather_camera: None,
        };
        let symbol = point_vector_record_to_symbol_feature(&record, None).unwrap();
        let plan = FlightPlan {
            id: "plan".to_string(),
            name: "Plan".to_string(),
            route_components: Vec::new(),
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let item = selection_item_for_point(
            &record,
            &symbol,
            Some(&plan),
            AirportPlateAvailability::default(),
            None,
        );

        let nav_ref = NavRef::Fix("VAMPS".to_string());
        assert_eq!(item.nav_ref, Some(nav_ref.clone()));
        let direct_to = item
            .actions
            .iter()
            .find(|action| action.id == "direct_to")
            .expect("direct-to action");
        assert!(direct_to.enabled);
        assert_eq!(
            serde_json::from_str::<MapSelectionSessionAction>(
                direct_to
                    .session_action
                    .as_deref()
                    .expect("direct-to session action"),
            )
            .expect("session action decodes"),
            MapSelectionSessionAction::ActivateDirectToNavRef {
                nav_ref: nav_ref.clone(),
            }
        );

        let insert = item
            .actions
            .iter()
            .find(|action| action.id == "insert")
            .expect("insert action");
        assert!(insert.enabled);
        assert_eq!(
            serde_json::from_str::<MapSelectionSessionAction>(
                insert
                    .session_action
                    .as_deref()
                    .expect("insert session action"),
            )
            .expect("session action decodes"),
            MapSelectionSessionAction::InsertWaypointBestPosition { nav_ref }
        );
    }

    #[test]
    fn map_selection_hits_flight_plan_only_fix_point() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.0,
                lon: -122.0,
            },
            zoom: 9.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let nav_ref = NavRef::Fix("WIBAT".to_string());
        let supplemental_nav_ref_points = [NavRefSelectionPoint {
            feature_id: format!(
                "flight-plan:{}",
                serde_json::to_string(&nav_ref).expect("serialize NavRef")
            ),
            nav_ref: nav_ref.clone(),
            position: viewport.center,
            symbol: NavSymbolFeature {
                kind: "fix".to_string(),
                label: "WIBAT".to_string(),
                symbol_kind: "fix".to_string(),
                style_class: "fix".to_string(),
                obstacle_variant: None,
                obstacle_tone: None,
                towered: false,
                fuel_available: false,
                has_paved_runway: None,
                heliport: None,
                has_water_runway: None,
                runway_length_ratio: 0.0,
                longest_runway_heading_true_deg: None,
                elevation_msl_ft: None,
            },
        }];
        let config = test_map_overlay_config();
        let vector_tiles = HashMap::new();
        let metar_tiles = HashMap::new();
        let airspaces = HashMap::new();
        let aliases = WeatherStationAirportAliases::default();
        let mut availability = |_: &str| AirportPlateAvailability::default();
        let selection = query_map_selection(
            &viewport,
            800.0,
            600.0,
            MapSelectionQuery {
                supplemental_nav_ref_points: &supplemental_nav_ref_points,
                ..MapSelectionQuery::new(
                    &config,
                    viewport.center,
                    &vector_tiles,
                    &metar_tiles,
                    &airspaces,
                    &aliases,
                    &mut availability,
                )
            },
        );

        let navaids = selection
            .categories
            .iter()
            .find(|category| category.id == "navaid")
            .expect("navaid category");
        let wibat = navaids
            .items
            .iter()
            .find(|item| item.label == "WIBAT")
            .expect("flight-plan-only fix should be selectable");
        assert_eq!(wibat.nav_ref, Some(nav_ref.clone()));
        assert!(matches!(
            wibat.highlight,
            MapSelectionHighlight::FeatureRef { ref id }
                if id.starts_with("flight-plan:")
        ));
        let direct_to = wibat
            .actions
            .iter()
            .find(|action| action.id == "direct_to")
            .expect("direct-to action");
        assert!(direct_to.enabled);
        assert_eq!(
            serde_json::from_str::<MapSelectionSessionAction>(
                direct_to.session_action.as_deref().expect("session action")
            )
            .expect("session action decodes"),
            MapSelectionSessionAction::ActivateDirectToNavRef { nav_ref }
        );
    }

    #[test]
    fn vor_symbol_labels_omit_frequency() {
        let record = PointVectorRecord {
            id: "nav:ELN:VOR".to_string(),
            kind: "VORTAC".to_string(),
            lat: 47.024,
            lon: -120.459,
            label: "ELLENSBURG 117.9".to_string(),
            location_label: None,
            style_class: "nav".to_string(),
            towered: None,
            fuel_available: None,
            public_use: None,
            private_use: None,
            has_paved_runway: None,
            heliport: None,
            has_water_runway: None,
            longest_runway_length_ft: None,
            longest_runway_heading_true_deg: None,
            elevation_msl_ft: None,
            obstacle: None,
            weather_camera: None,
        };
        let feature = point_vector_record_to_symbol_feature(&record, None)
            .expect("VORTAC should be displayed");

        assert_eq!(feature.label, "ELN");
    }

    #[test]
    fn nav_ref_chart_ident_labels_use_vector_symbol_rules() {
        let symbol = NavSymbolFeature {
            kind: "VOR/DME".to_string(),
            label: "SEA 116.80".to_string(),
            symbol_kind: "nav".to_string(),
            style_class: "nav".to_string(),
            obstacle_variant: None,
            obstacle_tone: None,
            towered: false,
            fuel_available: false,
            has_paved_runway: None,
            heliport: None,
            has_water_runway: None,
            runway_length_ratio: 0.0,
            longest_runway_heading_true_deg: None,
            elevation_msl_ft: None,
        };

        assert_eq!(
            chart_ident_label_for_nav_ref_symbol(&NavRef::Navaid("SEA".to_string()), &symbol),
            "SEA"
        );

        let airport_symbol = NavSymbolFeature {
            kind: "airport".to_string(),
            label: "wrong".to_string(),
            symbol_kind: "airport".to_string(),
            style_class: "airport".to_string(),
            obstacle_variant: None,
            obstacle_tone: None,
            towered: false,
            fuel_available: false,
            has_paved_runway: None,
            heliport: None,
            has_water_runway: None,
            runway_length_ratio: 0.0,
            longest_runway_heading_true_deg: None,
            elevation_msl_ft: None,
        };
        assert_eq!(
            chart_ident_label_for_nav_ref_symbol(
                &NavRef::Airport("KPAE".to_string()),
                &airport_symbol
            ),
            "KPAE"
        );
        assert_eq!(
            chart_ident_label_for_nav_ref_symbol(
                &NavRef::Airport("1S5".to_string()),
                &airport_symbol
            ),
            "1S5"
        );
    }

    #[test]
    fn vor_selection_uses_frequency_as_description() {
        let record = PointVectorRecord {
            id: "nav:SEA:VOR".to_string(),
            kind: "VOR/DME".to_string(),
            lat: 47.435,
            lon: -122.309,
            label: "SEATTLE 118.8".to_string(),
            location_label: None,
            style_class: "nav".to_string(),
            towered: None,
            fuel_available: None,
            public_use: None,
            private_use: None,
            has_paved_runway: None,
            heliport: None,
            has_water_runway: None,
            longest_runway_length_ft: None,
            longest_runway_heading_true_deg: None,
            elevation_msl_ft: None,
            obstacle: None,
            weather_camera: None,
        };
        let symbol = point_vector_record_to_symbol_feature(&record, None).unwrap();
        let item = selection_item_for_point(
            &record,
            &symbol,
            None,
            AirportPlateAvailability {
                plates: false,
                csup: false,
            },
            None,
        );

        assert_eq!(item.label, "SEA");
        assert_eq!(item.description.as_deref(), Some("118.8"));
        assert!(!item.actions.iter().any(|action| action.id == "frequency"));
    }

    #[test]
    fn private_airport_symbols_are_available_when_not_chart_filtered() {
        let record = PointVectorRecord {
            id: "airports:WN08".to_string(),
            kind: "airport".to_string(),
            lat: 47.0,
            lon: -122.0,
            label: "PRIVATE STRIP".to_string(),
            location_label: None,
            style_class: "airport".to_string(),
            towered: Some(false),
            fuel_available: Some(false),
            public_use: Some(false),
            private_use: Some(true),
            has_paved_runway: Some(true),
            heliport: Some(false),
            has_water_runway: Some(false),
            longest_runway_length_ft: Some(1_900.0),
            longest_runway_heading_true_deg: Some(120.0),
            elevation_msl_ft: Some(82.0),
            obstacle: None,
            weather_camera: None,
        };

        assert!(
            point_vector_record_to_symbol_feature(&record, None).is_none(),
            "private airports remain hidden from the chart overlay"
        );
        let feature = point_vector_record_to_symbol_feature_unfiltered(&record, None)
            .expect("unfiltered feature should be present");
        assert_eq!(feature.style_class, "airport");
        assert_eq!(feature.label, "WN08");
        assert_eq!(feature.longest_runway_heading_true_deg, Some(120.0));
    }

    #[test]
    fn obstacle_symbol_variant_comes_from_structured_semantics() {
        let feature = point_vector_record_to_symbol_feature_unfiltered(
            &PointVectorRecord {
                id: "obs:51.679306:-108.690833:3451".to_string(),
                kind: "obs".to_string(),
                lat: 51.679_305_555_555_55,
                lon: -108.690_833_333_333_33,
                label: "3451".to_string(),
                location_label: None,
                style_class: "obstacle".to_string(),
                towered: None,
                fuel_available: None,
                public_use: None,
                private_use: None,
                has_paved_runway: None,
                heliport: None,
                has_water_runway: None,
                longest_runway_length_ft: None,
                longest_runway_heading_true_deg: None,
                elevation_msl_ft: None,
                obstacle: Some(ObstaclePointSemantics {
                    height_agl_ft: 1_076.0,
                    elevation_msl_ft: 2_375.0,
                    top_msl_ft: 3_451.0,
                    is_tall: true,
                }),
                weather_camera: None,
            },
            None,
        )
        .expect("obstacle should be present");

        assert_eq!(feature.style_class, "obstacle-caution");
        assert_eq!(feature.obstacle_variant.as_deref(), Some("tall"));
        assert!(feature.label.is_empty());
    }

    #[test]
    fn filters_private_water_and_heliport_airports_in_core() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.36,
                lon: -121.98,
            },
            zoom: 9.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let airport_tile =
            visible_point_tile_window(&test_map_overlay_config(), &viewport, 1200.0, 900.0)
                .into_iter()
                .find(|tile| tile.layer == "airport")
                .expect("expected airport tile");
        let mut cache = HashMap::new();
        cache.insert(
            tile_key(
                &airport_tile.layer,
                airport_tile.z,
                airport_tile.x,
                airport_tile.y,
            ),
            PointTilePayload {
                schema_version: 1,
                layer: airport_tile.layer.clone(),
                z: airport_tile.z,
                x: airport_tile.x,
                y: airport_tile.y,
                records: vec![
                    PointVectorRecord {
                        id: "airports:KSEA".to_string(),
                        kind: "airport".to_string(),
                        lat: 47.361,
                        lon: -121.981,
                        label: "SEATTLE".to_string(),
                        location_label: None,
                        style_class: "airport".to_string(),
                        towered: Some(true),
                        fuel_available: Some(true),
                        public_use: Some(true),
                        private_use: Some(false),
                        has_paved_runway: Some(true),
                        heliport: Some(false),
                        has_water_runway: Some(false),
                        longest_runway_length_ft: Some(10000.0),
                        longest_runway_heading_true_deg: Some(160.0),
                        elevation_msl_ft: Some(433.0),
                        obstacle: None,
                        weather_camera: None,
                    },
                    PointVectorRecord {
                        id: "airports:WN50".to_string(),
                        kind: "airport".to_string(),
                        lat: 47.3605,
                        lon: -121.9805,
                        label: "PRIVATE".to_string(),
                        location_label: None,
                        style_class: "airport".to_string(),
                        towered: Some(false),
                        fuel_available: Some(false),
                        public_use: Some(false),
                        private_use: Some(true),
                        has_paved_runway: Some(true),
                        heliport: Some(false),
                        has_water_runway: Some(false),
                        longest_runway_length_ft: Some(2500.0),
                        longest_runway_heading_true_deg: Some(90.0),
                        elevation_msl_ft: Some(120.0),
                        obstacle: None,
                        weather_camera: None,
                    },
                    PointVectorRecord {
                        id: "airports:W57".to_string(),
                        kind: "airport".to_string(),
                        lat: 47.36,
                        lon: -121.98,
                        label: "WATER".to_string(),
                        location_label: None,
                        style_class: "airport".to_string(),
                        towered: Some(false),
                        fuel_available: Some(false),
                        public_use: Some(true),
                        private_use: Some(false),
                        has_paved_runway: Some(false),
                        heliport: Some(false),
                        has_water_runway: Some(true),
                        longest_runway_length_ft: Some(3000.0),
                        longest_runway_heading_true_deg: Some(45.0),
                        elevation_msl_ft: Some(10.0),
                        obstacle: None,
                        weather_camera: None,
                    },
                    PointVectorRecord {
                        id: "airports:H1".to_string(),
                        kind: "heliport".to_string(),
                        lat: 47.362,
                        lon: -121.982,
                        label: "HELI".to_string(),
                        location_label: None,
                        style_class: "airport".to_string(),
                        towered: Some(false),
                        fuel_available: Some(false),
                        public_use: Some(true),
                        private_use: Some(false),
                        has_paved_runway: Some(false),
                        heliport: Some(true),
                        has_water_runway: Some(false),
                        longest_runway_length_ft: Some(80.0),
                        longest_runway_heading_true_deg: Some(0.0),
                        elevation_msl_ft: Some(200.0),
                        obstacle: None,
                        weather_camera: None,
                    },
                ],
            },
        );

        let result = query_map_overlay(
            &viewport,
            1200.0,
            900.0,
            &cache,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(result.visible_features.len(), 1);
        assert_eq!(result.visible_features[0].id, "airports:KSEA");

        let vector_cache = aggregate_test_vector_tiles(&cache, &HashMap::new(), &HashMap::new());
        let config = test_map_overlay_config();
        let metar_tiles = HashMap::new();
        let airspaces = HashMap::new();
        let aliases = WeatherStationAirportAliases::default();
        let mut availability = |_: &str| AirportPlateAvailability::default();
        let selection = query_map_selection(
            &viewport,
            1200.0,
            900.0,
            MapSelectionQuery::new(
                &config,
                viewport.center,
                &vector_cache,
                &metar_tiles,
                &airspaces,
                &aliases,
                &mut availability,
            ),
        );
        let airport_ids = selection.categories[0]
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            airport_ids,
            vec![
                "airports:KSEA",
                "airports:W57",
                "airports:WN50",
                "airports:H1"
            ]
        );
        assert_eq!(
            selection.initial_selected_item_id.as_deref(),
            Some("airports:KSEA"),
            "a rendered airport must outrank a closer airport hidden by vector display policy",
        );
        let private_airport = selection.categories[0]
            .items
            .iter()
            .find(|item| item.id == "airports:WN50")
            .expect("private airport selection");
        assert!(private_airport
            .actions
            .iter()
            .any(|action| action.id == "plates" && !action.enabled));
        assert!(private_airport
            .actions
            .iter()
            .any(|action| action.id == "csup" && !action.enabled));
    }

    fn test_visible_feature(
        id: &str,
        kind: &str,
        style_class: &str,
        label: &str,
        screen_x: f64,
        screen_y: f64,
    ) -> VisibleMapFeature {
        VisibleMapFeature {
            id: id.to_string(),
            kind: kind.to_string(),
            label: label.to_string(),
            symbol_kind: point_symbol_kind(style_class, kind),
            style_class: style_class.to_string(),
            obstacle_variant: None,
            obstacle_tone: None,
            screen_x,
            screen_y,
            towered: false,
            fuel_available: false,
            has_paved_runway: None,
            heliport: None,
            has_water_runway: None,
            runway_length_ratio: 0.0,
            longest_runway_heading_true_deg: None,
            label_style: VectorIdentLabelStyle::Default,
        }
    }
}
