use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{
    geometry::LatLon, great_circle_distance_nm, AppError, AppErrorKind, AppResult, FlightPlan,
    MapViewport, NavRef,
};

pub const VECTOR_DISPLAY_FEATURE_LIMIT: usize = 500;
pub const AIRSPACE_DISPLAY_FEATURE_LIMIT: usize = 700;
pub const AIRSPACE_FEATHER_LIMIT: usize = 5_000;
const LABEL_COLLISION_PADDING_PX: f64 = 3.0;
const POINT_TILE_ZOOM: u32 = 9;
const AIRSPACE_MIN_DISPLAY_ZOOM: f64 = 6.0;
const AIRPORT_MIN_DISPLAY_ZOOM: f64 = 8.0;
const FIX_MIN_DISPLAY_ZOOM: f64 = 9.0;
const NAV_MIN_DISPLAY_ZOOM: f64 = 7.0;
const OBSTACLE_MIN_DISPLAY_ZOOM: f64 = 8.0;
const OBSTACLE_LOOKAHEAD_MINUTES: f64 = 5.0;
const OBSTACLE_LOOKAHEAD_DEFAULT_DIAMETER_NM: f64 = 5.0;
const OBSTACLE_LOOKAHEAD_CENTER_OFFSET_DIAMETER_RATIO: f64 = 0.3;
const OBSTACLE_BELOW_OWNERSHIP_HIDE_FT: f64 = 1000.0;
const OBSTACLE_CAUTION_LOWER_FT: f64 = 800.0;
const OBSTACLE_DANGER_LOWER_FT: f64 = 200.0;
const METAR_DISPLAY_FEATURE_LIMIT: usize = 1_000;
const WORLD_SIZE: f64 = 256.0;
const MAX_LATITUDE: f64 = 85.051_128_78;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorTileRequest {
    pub layer: String,
    pub z: u32,
    pub x: u32,
    pub y: u32,
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
pub struct PointVectorRecord {
    pub id: String,
    pub kind: String,
    pub lat: f64,
    pub lon: f64,
    pub label: String,
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
    pub taf_count: Option<u32>,
    pub tafs_by_station: HashMap<String, TafRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetarProductPayload {
    pub schema_version: u32,
    pub version_label: String,
    #[serde(default)]
    pub metar_count: Option<u32>,
    pub metars_by_station: HashMap<String, MetarRecord>,
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
    pub points: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TfrProductPayload {
    pub schema_version: u32,
    pub version_label: String,
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
    pub avare_text: String,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisibleMapFeature {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub style_class: String,
    #[serde(default)]
    pub obstacle_variant: Option<String>,
    pub screen_x: f64,
    pub screen_y: f64,
    pub towered: bool,
    pub fuel_available: bool,
    pub has_paved_runway: Option<bool>,
    pub heliport: Option<bool>,
    pub has_water_runway: Option<bool>,
    pub runway_length_ratio: f64,
    pub longest_runway_heading_true_deg: Option<f64>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceScreenPoint {
    pub x: f64,
    pub y: f64,
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
    pub paths: Vec<AirspaceDisplaySubpath>,
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
pub struct NavSymbolFeature {
    pub kind: String,
    pub label: String,
    pub style_class: String,
    #[serde(default)]
    pub obstacle_variant: Option<String>,
    pub towered: bool,
    pub fuel_available: bool,
    #[serde(default)]
    pub has_paved_runway: Option<bool>,
    #[serde(default)]
    pub heliport: Option<bool>,
    #[serde(default)]
    pub has_water_runway: Option<bool>,
    pub runway_length_ratio: f64,
    pub longest_runway_heading_true_deg: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapOverlayWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapOverlayQueryResult {
    pub needed_point_tiles: Vec<VectorTileRequest>,
    pub needed_metar_tiles: Vec<VectorTileRequest>,
    pub needed_airspace_ref_tiles: Vec<VectorTileRequest>,
    pub needed_airspace_features: Vec<AirspaceFeatureRequest>,
    pub needed_airspace_label_tiles: Vec<VectorTileRequest>,
    pub needed_metars: bool,
    pub needed_tfrs: bool,
    pub visible_features: Vec<VisibleMapFeature>,
    pub visible_metars: Vec<VisibleMetarFeature>,
    pub airspace_paths: Vec<AirspaceDisplayPath>,
    pub tfr_paths: Vec<AirspaceDisplayPath>,
    pub airspace_labels: Vec<AirspaceDisplayLabel>,
    pub warnings: Vec<MapOverlayWarning>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapSelectionQueryResult {
    pub click_lat: f64,
    pub click_lon: f64,
    pub categories: Vec<MapSelectionCategory>,
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
    pub detail_text: Option<String>,
    pub highlight: MapSelectionHighlight,
    #[serde(default)]
    pub nav_ref: Option<NavRef>,
    #[serde(default)]
    pub symbol_feature: Option<NavSymbolFeature>,
    #[serde(default)]
    pub metar_feature: Option<VisibleMetarFeature>,
    #[serde(default)]
    pub airspace_icon: Option<AirspaceDisplayPath>,
    pub actions: Vec<MapSelectionAction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MapSelectionHighlight {
    FeatureRef { id: String },
    Metar { station_id: String },
    Spot { lat: f64, lon: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapSelectionAction {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub display_only: bool,
    #[serde(default)]
    pub detail_text: Option<String>,
    #[serde(default)]
    pub airspace_limit: Option<AirspaceLimitGlyph>,
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
    pub obstacle_layer: Option<ObstacleLayerConfig>,
    pub metar_layer: Option<PointTileLayerConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointTileLayerConfig {
    min_zoom: u32,
    max_zoom: u32,
    available_zooms: Vec<u32>,
}

#[derive(Debug, Deserialize)]
struct VectorOverlayManifest {
    #[serde(default)]
    point_layers: HashMap<String, VectorPointLayerManifest>,
    airspace: VectorAirspaceManifest,
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
    let obstacle_layer = manifest
        .point_layers
        .get("obstacle")
        .map(obstacle_layer_config_from_manifest)
        .transpose()?;
    let metar_layer = manifest
        .point_layers
        .get("metars")
        .map(|layer| point_tile_layer_config_from_manifest("metars", layer))
        .transpose()?;
    Ok(MapOverlayConfig {
        airspace_reference_tile_min_zoom: manifest.airspace.reference_tile_min_zoom,
        airspace_reference_tile_max_zoom: manifest.airspace.reference_tile_max_zoom,
        airspace_label_tile_min_zoom: manifest.airspace.label_tile_min_zoom,
        airspace_label_tile_max_zoom: manifest.airspace.label_tile_max_zoom,
        obstacle_layer,
        metar_layer,
    })
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
    obstacle_context: Option<&ObstacleOverlayContext>,
) -> Vec<VectorTileRequest> {
    if width_px <= 0.0 || height_px <= 0.0 {
        return Vec::new();
    }
    let mut tiles = Vec::new();
    if viewport.zoom >= AIRPORT_MIN_DISPLAY_ZOOM {
        tiles.extend(visible_layer_tile_window(
            "airport",
            POINT_TILE_ZOOM,
            viewport,
            width_px,
            height_px,
        ));
    }
    if viewport.zoom >= FIX_MIN_DISPLAY_ZOOM {
        tiles.extend(visible_layer_tile_window(
            "fix",
            POINT_TILE_ZOOM,
            viewport,
            width_px,
            height_px,
        ));
    }
    if viewport.zoom >= NAV_MIN_DISPLAY_ZOOM {
        tiles.extend(visible_layer_tile_window(
            "nav",
            POINT_TILE_ZOOM,
            viewport,
            width_px,
            height_px,
        ));
    }
    if let Some(obstacle_layer) = config.obstacle_layer.as_ref() {
        tiles.extend(visible_obstacle_tile_window(
            obstacle_layer,
            viewport,
            width_px,
            height_px,
            obstacle_context,
        ));
    }
    tiles
}

fn visible_obstacle_tile_window(
    config: &ObstacleLayerConfig,
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    obstacle_context: Option<&ObstacleOverlayContext>,
) -> Vec<VectorTileRequest> {
    if viewport.zoom < OBSTACLE_MIN_DISPLAY_ZOOM {
        return Vec::new();
    }
    let display_zoom = nearest_available_zoom(config, viewport.zoom.floor() as u32);
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

fn nearest_available_zoom(config: &ObstacleLayerConfig, desired_zoom: u32) -> u32 {
    nearest_available_zoom_in(
        config.min_zoom,
        config.max_zoom,
        &config.available_zooms,
        desired_zoom,
    )
}

fn nearest_available_layer_zoom(config: &PointTileLayerConfig, desired_zoom: u32) -> u32 {
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
    let center_world = lat_lon_to_world(viewport.center);
    let scale = 2.0_f64.powf(viewport.zoom);
    let min_world_x = center_world.x - width_px / 2.0 / scale;
    let max_world_x = center_world.x + width_px / 2.0 / scale;
    let min_world_y = center_world.y - height_px / 2.0 / scale;
    let max_world_y = center_world.y + height_px / 2.0 / scale;
    let tile_world_size = WORLD_SIZE / (2_u32.pow(zoom) as f64);
    let max_index = (2_u32.pow(zoom) - 1) as i32;
    let x_start = (min_world_x / tile_world_size).floor() as i32;
    let x_end = (max_world_x / tile_world_size).floor() as i32;
    let y_start = (min_world_y / tile_world_size).floor() as i32;
    let y_end = (max_world_y / tile_world_size).floor() as i32;
    let mut tiles = Vec::new();

    for y in y_start.max(0)..=y_end.min(max_index) {
        for x in x_start.max(0)..=x_end.min(max_index) {
            tiles.push(VectorTileRequest {
                layer: layer.to_string(),
                z: zoom,
                x: x as u32,
                y: y as u32,
            });
        }
    }

    tiles
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
    let max_index = (2_u32.pow(zoom) - 1) as i32;
    let min_world_x = center_world.x - world_radius;
    let max_world_x = center_world.x + world_radius;
    let min_world_y = center_world.y - world_radius;
    let max_world_y = center_world.y + world_radius;
    let x_start = (min_world_x / tile_world_size).floor() as i32;
    let x_end = (max_world_x / tile_world_size).floor() as i32;
    let y_start = (min_world_y / tile_world_size).floor() as i32;
    let y_end = (max_world_y / tile_world_size).floor() as i32;
    let mut tiles = Vec::new();

    for y in y_start.max(0)..=y_end.min(max_index) {
        for x in x_start.max(0)..=x_end.min(max_index) {
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

pub fn query_map_overlay(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    config: &MapOverlayConfig,
    display_vectors: bool,
    display_metars: bool,
    obstacle_context: Option<&ObstacleOverlayContext>,
    point_tile_cache: &HashMap<String, PointTilePayload>,
    metar_tile_cache: &HashMap<String, MetarTilePayload>,
    metar_payload: Option<&MetarProductPayload>,
    airspace_ref_tile_cache: &HashMap<String, AirspaceReferenceTilePayload>,
    airspace_feature_cache: &HashMap<String, AirspaceFeaturePayload>,
    airspace_label_tile_cache: &HashMap<String, AirspaceLabelTilePayload>,
    tfr_payload: Option<&TfrProductPayload>,
) -> MapOverlayQueryResult {
    let mut needed_point_tiles = Vec::new();
    let mut visible_features = Vec::new();
    let mut limit_hit = false;
    let center_world = lat_lon_to_world(viewport.center);
    let scale = 2.0_f64.powf(viewport.zoom);

    if display_vectors {
        let tile_window =
            visible_point_tile_window(config, viewport, width_px, height_px, obstacle_context);
        for tile in tile_window {
            let key = tile_key(&tile.layer, tile.z, tile.x, tile.y);
            let Some(payload) = point_tile_cache.get(&key) else {
                needed_point_tiles.push(tile);
                continue;
            };
            for record in &payload.records {
                if visible_features.len() >= VECTOR_DISPLAY_FEATURE_LIMIT {
                    limit_hit = true;
                    break;
                }
                if !should_display_record(record) {
                    continue;
                }
                let point = world_to_screen(
                    center_world,
                    scale,
                    width_px,
                    height_px,
                    LatLon {
                        lat: record.lat,
                        lon: record.lon,
                    },
                );
                let Some(symbol) = point_vector_record_to_symbol_feature(
                    record,
                    obstacle_context.and_then(|context| context.altitude_ft),
                ) else {
                    continue;
                };
                visible_features.push(VisibleMapFeature {
                    id: record.id.clone(),
                    kind: symbol.kind,
                    label: symbol.label,
                    style_class: symbol.style_class,
                    obstacle_variant: symbol.obstacle_variant,
                    screen_x: point.x,
                    screen_y: point.y,
                    towered: symbol.towered,
                    fuel_available: symbol.fuel_available,
                    has_paved_runway: symbol.has_paved_runway,
                    heliport: symbol.heliport,
                    has_water_runway: symbol.has_water_runway,
                    runway_length_ratio: symbol.runway_length_ratio,
                    longest_runway_heading_true_deg: symbol.longest_runway_heading_true_deg,
                });
            }
            if limit_hit {
                break;
            }
        }
    }

    let warnings = if limit_hit {
        vec![MapOverlayWarning {
            code: "vector_display_feature_limit".to_string(),
            message: format!(
                "display capped at {} visible vector features",
                VECTOR_DISPLAY_FEATURE_LIMIT
            ),
        }]
    } else {
        Vec::new()
    };

    let airspace = if display_vectors {
        query_airspace_overlay(
            viewport,
            width_px,
            height_px,
            config,
            center_world,
            scale,
            airspace_ref_tile_cache,
            airspace_feature_cache,
            airspace_label_tile_cache,
        )
    } else {
        AirspaceOverlayProjection {
            needed_ref_tiles: Vec::new(),
            needed_features: Vec::new(),
            needed_label_tiles: Vec::new(),
            paths: Vec::new(),
            labels: Vec::new(),
            warnings: Vec::new(),
        }
    };
    let mut warnings = warnings;
    warnings.extend(airspace.warnings);
    let tfrs = if display_vectors {
        query_tfr_overlay(
            viewport,
            width_px,
            height_px,
            center_world,
            scale,
            tfr_payload,
        )
    } else {
        TfrOverlayProjection {
            needed_tfrs: false,
            paths: Vec::new(),
            labels: Vec::new(),
        }
    };
    let metars = if display_metars {
        query_metar_overlay(
            viewport,
            width_px,
            height_px,
            config,
            center_world,
            scale,
            metar_tile_cache,
            metar_payload,
        )
    } else {
        MetarOverlayProjection {
            needed_tiles: Vec::new(),
            needed_metars: false,
            visible_metars: Vec::new(),
            warnings: Vec::new(),
        }
    };
    warnings.extend(metars.warnings);

    let mut airspace_labels = {
        let mut labels = airspace.labels;
        labels.extend(tfrs.labels);
        labels
    };
    suppress_overlapping_vector_labels(&mut visible_features, &mut airspace_labels);

    MapOverlayQueryResult {
        needed_point_tiles,
        needed_metar_tiles: metars.needed_tiles,
        needed_airspace_ref_tiles: airspace.needed_ref_tiles,
        needed_airspace_features: airspace.needed_features,
        needed_airspace_label_tiles: airspace.needed_label_tiles,
        needed_metars: metars.needed_metars,
        needed_tfrs: tfrs.needed_tfrs,
        visible_features,
        visible_metars: metars.visible_metars,
        airspace_paths: airspace.paths,
        tfr_paths: tfrs.paths,
        airspace_labels,
        warnings,
    }
}

struct MetarOverlayProjection {
    needed_tiles: Vec<VectorTileRequest>,
    needed_metars: bool,
    visible_metars: Vec<VisibleMetarFeature>,
    warnings: Vec<MapOverlayWarning>,
}

fn query_metar_overlay(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    config: &MapOverlayConfig,
    center_world: WorldPoint,
    scale: f64,
    metar_tile_cache: &HashMap<String, MetarTilePayload>,
    metar_payload: Option<&MetarProductPayload>,
) -> MetarOverlayProjection {
    let Some(metar_layer) = config.metar_layer.as_ref() else {
        return MetarOverlayProjection {
            needed_tiles: Vec::new(),
            needed_metars: false,
            visible_metars: Vec::new(),
            warnings: Vec::new(),
        };
    };
    let needed_metars = metar_payload.is_none();
    let mut needed_tiles = Vec::new();
    let mut visible_metars = Vec::new();
    let mut limit_hit = false;
    let metar_zoom = nearest_available_layer_zoom(metar_layer, viewport.zoom.floor() as u32);
    for tile in visible_layer_tile_window("metars", metar_zoom, viewport, width_px, height_px) {
        let key = tile_key(&tile.layer, tile.z, tile.x, tile.y);
        let Some(tile_payload) = metar_tile_cache.get(&key) else {
            needed_tiles.push(tile);
            continue;
        };
        let Some(metars) = metar_payload else {
            continue;
        };
        for record_ref in &tile_payload.records {
            if record_ref.kind != "metar" {
                continue;
            }
            if visible_metars.len() >= METAR_DISPLAY_FEATURE_LIMIT {
                limit_hit = true;
                break;
            }
            let Some(record) = metars.metars_by_station.get(&record_ref.id) else {
                continue;
            };
            let feature = visible_metar_feature(record, center_world, scale, width_px, height_px);
            if feature.screen_x < -32.0
                || feature.screen_x > width_px + 32.0
                || feature.screen_y < -32.0
                || feature.screen_y > height_px + 32.0
            {
                continue;
            }
            visible_metars.push(feature);
        }
        if limit_hit {
            break;
        }
    }
    let warnings = if limit_hit {
        vec![MapOverlayWarning {
            code: "metar_display_feature_limit".to_string(),
            message: format!(
                "display capped at {} visible METAR features",
                METAR_DISPLAY_FEATURE_LIMIT
            ),
        }]
    } else {
        Vec::new()
    };
    MetarOverlayProjection {
        needed_tiles,
        needed_metars,
        visible_metars,
        warnings,
    }
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
) -> VisibleMetarFeature {
    let point = world_to_screen(
        center_world,
        scale,
        width_px,
        height_px,
        LatLon {
            lat: record.latitude,
            lon: record.longitude,
        },
    );
    VisibleMetarFeature {
        station_id: record.station_id.clone(),
        screen_x: point.x,
        screen_y: point.y,
        flight_category: normalized_metar_flight_category(record),
        ceiling_amount: normalized_metar_ceiling_amount(record),
    }
}

pub fn query_map_selection(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    config: &MapOverlayConfig,
    plan: Option<&FlightPlan>,
    click: LatLon,
    hit_radius_px: f64,
    point_tile_cache: &HashMap<String, PointTilePayload>,
    metar_tile_cache: &HashMap<String, MetarTilePayload>,
    metar_payload: Option<&MetarProductPayload>,
    taf_payload: Option<&TafProductPayload>,
    airspace_feature_cache: &HashMap<String, AirspaceFeaturePayload>,
    tfr_payload: Option<&TfrProductPayload>,
    airport_plate_availability: &mut dyn FnMut(&str) -> AirportPlateAvailability,
) -> MapSelectionQueryResult {
    let center_world = lat_lon_to_world(viewport.center);
    let scale = 2.0_f64.powf(viewport.zoom);
    let click_screen = world_to_screen(center_world, scale, width_px, height_px, click);
    let mut airports = Vec::new();
    let mut navaids = Vec::new();
    let mut weather = Vec::new();
    let mut airspaces = Vec::new();

    for tile in visible_point_tile_window(config, viewport, width_px, height_px, None) {
        let Some(payload) = point_tile_cache.get(&tile_key(&tile.layer, tile.z, tile.x, tile.y))
        else {
            continue;
        };
        for record in &payload.records {
            let is_airport = selection_record_is_airport(record);
            if !is_airport && !should_display_record(record) {
                continue;
            }
            let point = world_to_screen(
                center_world,
                scale,
                width_px,
                height_px,
                LatLon {
                    lat: record.lat,
                    lon: record.lon,
                },
            );
            let distance_px =
                ((point.x - click_screen.x).powi(2) + (point.y - click_screen.y).powi(2)).sqrt();
            if distance_px > hit_radius_px {
                continue;
            }
            let Some(symbol) = selection_symbol_for_point(record, is_airport) else {
                continue;
            };
            if is_airport {
                let availability = selection_nav_ref(record, true)
                    .and_then(|nav_ref| match nav_ref {
                        NavRef::Airport(airport_id) => {
                            Some(airport_plate_availability(&airport_id))
                        }
                        _ => None,
                    })
                    .unwrap_or_default();
                let airport_id =
                    selection_nav_ref(record, true).and_then(|nav_ref| match nav_ref {
                        NavRef::Airport(airport_id) => Some(airport_id),
                        _ => None,
                    });
                let taf = airport_id.as_deref().and_then(|airport_id| {
                    taf_payload.and_then(|payload| payload.tafs_by_station.get(airport_id))
                });
                let item = selection_item_for_point(record, &symbol, plan, availability, taf);
                airports.push(MapSelectionPointMatch { item, distance_px });
            } else if record.style_class == "fix" || record.style_class == "nav" {
                let item = selection_item_for_point(
                    record,
                    &symbol,
                    plan,
                    AirportPlateAvailability::default(),
                    None,
                );
                navaids.push(MapSelectionPointMatch { item, distance_px });
            }
        }
    }

    for feature in airspace_feature_cache.values() {
        if selectable_airspace_feature(feature) && airspace_feature_contains(feature, click) {
            airspaces.push(selection_item_for_airspace(feature));
        }
    }
    if let Some(tfr_payload) = tfr_payload {
        for area in &tfr_payload.areas {
            if tfr_area_contains(area, click) {
                airspaces.push(selection_item_for_tfr(area));
            }
        }
    }

    if let Some(metar_payload) = metar_payload {
        weather.extend(query_metar_selection_matches(
            viewport,
            width_px,
            height_px,
            config,
            center_world,
            scale,
            click_screen,
            hit_radius_px,
            metar_tile_cache,
            metar_payload,
            taf_payload,
        ));
    }

    airports.sort_by(compare_map_selection_point_matches);
    navaids.sort_by(compare_map_selection_point_matches);
    weather.sort_by(compare_map_selection_point_matches);
    airspaces.sort_by(|left, right| {
        left.label
            .cmp(&right.label)
            .then_with(|| left.id.cmp(&right.id))
    });
    navaids.push(MapSelectionPointMatch {
        item: spot_selection_item(click),
        distance_px: f64::INFINITY,
    });

    MapSelectionQueryResult {
        click_lat: click.lat,
        click_lon: click.lon,
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
        ],
    }
}

fn query_metar_selection_matches(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    config: &MapOverlayConfig,
    center_world: WorldPoint,
    scale: f64,
    click_screen: WorldPoint,
    hit_radius_px: f64,
    metar_tile_cache: &HashMap<String, MetarTilePayload>,
    metar_payload: &MetarProductPayload,
    taf_payload: Option<&TafProductPayload>,
) -> Vec<MapSelectionPointMatch> {
    let Some(metar_layer) = config.metar_layer.as_ref() else {
        return Vec::new();
    };
    let mut matches = Vec::new();
    let metar_zoom = nearest_available_layer_zoom(metar_layer, viewport.zoom.floor() as u32);
    for tile in visible_layer_tile_window("metars", metar_zoom, viewport, width_px, height_px) {
        let Some(tile_payload) =
            metar_tile_cache.get(&tile_key(&tile.layer, tile.z, tile.x, tile.y))
        else {
            continue;
        };
        for record_ref in &tile_payload.records {
            if record_ref.kind != "metar" {
                continue;
            }
            let Some(record) = metar_payload.metars_by_station.get(&record_ref.id) else {
                continue;
            };
            let feature = visible_metar_feature(record, center_world, scale, width_px, height_px);
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
                    ),
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
    taf: Option<&TafRecord>,
) -> MapSelectionItem {
    let is_airport = record.style_class == "airport"
        || record.kind.eq_ignore_ascii_case("airport")
        || record.id.starts_with("airports:");
    let label = if is_airport {
        airport_icao_label(record).unwrap_or_else(|| display_label(record))
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
    let insert_action = match &nav_ref {
        Some(nav_ref) if selection_plan_has_top_level_waypoint(plan, nav_ref) => {
            enabled_action("remove_from_flight_plan", "Remove from flight plan")
        }
        Some(nav_ref) if !selection_plan_contains_nav_ref(plan, nav_ref) => {
            enabled_action("insert", "Insert in flight plan")
        }
        Some(_) => disabled_action("insert", "In grouped route"),
        None => disabled_action("insert", "Insert unavailable"),
    };
    let mut actions = if is_airport {
        vec![
            action_for_availability("direct_to", "Direct-to", nav_ref.is_some()),
            insert_action,
            action_for_availability("plates", "Plates", airport_plate_availability.plates),
            action_for_availability("csup", "Chart Supp", airport_plate_availability.csup),
            taf.map(|record| detail_action("taf", "TAF", taf_detail_text(record)))
                .unwrap_or_else(|| disabled_action("taf", "TAF")),
            disabled_action("runways", "Runways"),
        ]
    } else {
        vec![
            action_for_availability("direct_to", "Direct-to", nav_ref.is_some()),
            insert_action,
        ]
    };
    MapSelectionItem {
        id: record.id.clone(),
        label,
        sublabel: record.kind.trim().to_ascii_uppercase(),
        description: selection_item_description(record, is_airport),
        detail_text: None,
        highlight: MapSelectionHighlight::FeatureRef {
            id: record.id.clone(),
        },
        nav_ref,
        symbol_feature: Some(symbol_feature),
        metar_feature: None,
        airspace_icon: None,
        actions: {
            actions.shrink_to_fit();
            actions
        },
    }
}

fn action_for_availability(id: &str, label: &str, available: bool) -> MapSelectionAction {
    if available {
        enabled_action(id, label)
    } else {
        disabled_action(id, label)
    }
}

fn spot_selection_item(click: LatLon) -> MapSelectionItem {
    MapSelectionItem {
        id: format!("spot:{:.6}:{:.6}", click.lat, click.lon),
        label: "SPOT".to_string(),
        sublabel: format!("{:.4}, {:.4}", click.lat, click.lon),
        description: None,
        detail_text: None,
        highlight: MapSelectionHighlight::Spot {
            lat: click.lat,
            lon: click.lon,
        },
        nav_ref: None,
        symbol_feature: None,
        metar_feature: None,
        airspace_icon: None,
        actions: vec![
            display_action("terrain", "Terrain --"),
            disabled_action("direct_to", "Direct-to"),
            disabled_action("insert", "Insert in flight plan"),
        ],
    }
}

fn selection_item_for_metar(
    record: &MetarRecord,
    taf: Option<&TafRecord>,
    feature: VisibleMetarFeature,
) -> MapSelectionItem {
    MapSelectionItem {
        id: format!("metar:{}", record.station_id.trim()),
        label: record.station_id.trim().to_ascii_uppercase(),
        sublabel: normalized_metar_flight_category(record).to_ascii_uppercase(),
        description: record.observed_at_utc.clone(),
        detail_text: Some(record.raw_text.clone()),
        highlight: MapSelectionHighlight::Metar {
            station_id: record.station_id.clone(),
        },
        nav_ref: None,
        symbol_feature: None,
        metar_feature: Some(feature),
        airspace_icon: None,
        actions: vec![
            display_action("metar", "METAR"),
            taf.map(|record| detail_action("taf", "TAF", taf_detail_text(record)))
                .unwrap_or_else(|| disabled_action("taf", "TAF")),
        ],
    }
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
    MapSelectionItem {
        id: feature.id.clone(),
        label: airspace_selection_label(feature),
        sublabel: feature.ident.trim().to_string(),
        description: None,
        detail_text: None,
        highlight: MapSelectionHighlight::FeatureRef {
            id: feature.id.clone(),
        },
        nav_ref: None,
        symbol_feature: None,
        metar_feature: None,
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

fn selectable_airspace_feature(feature: &AirspaceFeaturePayload) -> bool {
    !feature.id.contains(":outline:")
}

fn airspace_selection_icon(feature: &AirspaceFeaturePayload) -> Option<AirspaceDisplayPath> {
    airspace_icon_paths_from_lon_lat_paths(
        feature.paths.iter().map(|path| AirspaceIconSourcePath {
            closed: path.closed,
            interior_side: path.interior_side.clone(),
            points: path.points.clone(),
        }),
        &feature.id,
        &feature.name,
        &airspace_style_key(&feature.style_hint),
        None,
    )
}

fn tfr_selection_icon(area: &TfrAreaPayload) -> Option<AirspaceDisplayPath> {
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
        "tfr",
        Some(tfr_display_style()),
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
        decorations: airspace_decorations(style_key, &paths, &mut decoration_budget),
        paths,
    })
}

fn selection_item_for_tfr(area: &TfrAreaPayload) -> MapSelectionItem {
    MapSelectionItem {
        id: format!("tfr:{}:{}", area.notam_id.trim(), area.area_index),
        label: "TFR".to_string(),
        sublabel: area.notam_id.trim().to_string(),
        description: None,
        detail_text: None,
        highlight: MapSelectionHighlight::FeatureRef {
            id: format!("tfr:{}:{}", area.notam_id.trim(), area.area_index),
        },
        nav_ref: None,
        symbol_feature: None,
        metar_feature: None,
        airspace_icon: tfr_selection_icon(area),
        actions: vec![airspace_limit_action_from_parts(
            "limits",
            tfr_limit_label(&area.upper_limit),
            tfr_limit_label(&area.lower_limit),
            "tfr",
        )],
    }
}

fn display_action(id: &str, label: &str) -> MapSelectionAction {
    MapSelectionAction {
        id: id.to_string(),
        label: label.to_string(),
        enabled: false,
        display_only: true,
        detail_text: None,
        airspace_limit: None,
    }
}

fn detail_action(id: &str, label: &str, detail_text: String) -> MapSelectionAction {
    MapSelectionAction {
        id: id.to_string(),
        label: label.to_string(),
        enabled: true,
        display_only: false,
        detail_text: Some(detail_text),
        airspace_limit: None,
    }
}

fn enabled_action(id: &str, label: &str) -> MapSelectionAction {
    MapSelectionAction {
        id: id.to_string(),
        label: label.to_string(),
        enabled: true,
        display_only: false,
        detail_text: None,
        airspace_limit: None,
    }
}

fn disabled_action(id: &str, label: &str) -> MapSelectionAction {
    MapSelectionAction {
        id: id.to_string(),
        label: label.to_string(),
        enabled: false,
        display_only: false,
        detail_text: None,
        airspace_limit: None,
    }
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
        detail_text: None,
        airspace_limit: Some(AirspaceLimitGlyph {
            upper,
            lower,
            style_key: style_key.to_string(),
            color_key: airspace_label_color_key(style_key).to_string(),
        }),
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

fn airport_icao_label(record: &PointVectorRecord) -> Option<String> {
    record
        .id
        .strip_prefix("airports:")
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(|id| id.to_ascii_uppercase())
}

fn airport_selection_description(elevation_msl_ft: Option<f64>) -> Option<String> {
    elevation_msl_ft
        .filter(|value| value.is_finite())
        .map(|value| format!("Elev {}", value.round() as i64))
}

fn selection_item_description(record: &PointVectorRecord, is_airport: bool) -> Option<String> {
    if is_airport {
        return airport_selection_description(record.elevation_msl_ft);
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
    None
}

fn selection_plan_contains_nav_ref(plan: Option<&FlightPlan>, nav_ref: &NavRef) -> bool {
    plan.map(|plan| crate::flight_plan_contains_nav_ref(plan, nav_ref))
        .unwrap_or(false)
}

fn selection_plan_has_top_level_waypoint(plan: Option<&FlightPlan>, nav_ref: &NavRef) -> bool {
    plan.and_then(|plan| crate::top_level_waypoint_component_index(plan, nav_ref))
        .is_some()
}

fn airspace_feature_contains(feature: &AirspaceFeaturePayload, point: LatLon) -> bool {
    if point.lon < feature.bbox[0]
        || point.lat < feature.bbox[1]
        || point.lon > feature.bbox[2]
        || point.lat > feature.bbox[3]
    {
        return false;
    }
    feature.paths.iter().any(|path| {
        path.closed
            && path.points.len() >= 3
            && lon_lat_polygon_contains(&path.points, point.lon, point.lat)
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
    lon_lat_polygon_contains(&polygon, point.lon, point.lat)
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
}

#[derive(Debug, Clone, Copy)]
enum LabelRef {
    Airspace(usize),
    Point(usize),
}

fn suppress_overlapping_vector_labels(
    visible_features: &mut [VisibleMapFeature],
    airspace_labels: &mut Vec<AirspaceDisplayLabel>,
) {
    let mut candidates = Vec::<(LabelRef, LabelRect)>::new();
    for (index, label) in airspace_labels.iter().enumerate() {
        if let Some(rect) = airspace_label_rect(label) {
            candidates.push((
                LabelRef::Airspace(index),
                rect.padded(LABEL_COLLISION_PADDING_PX),
            ));
        }
    }
    for (index, feature) in visible_features.iter().enumerate() {
        if let Some(rect) = point_feature_label_rect(feature) {
            candidates.push((
                LabelRef::Point(index),
                rect.padded(LABEL_COLLISION_PADDING_PX),
            ));
        }
    }

    let mut occupied = Vec::<LabelRect>::new();
    let mut keep_airspace = vec![true; airspace_labels.len()];
    let mut keep_point = vec![true; visible_features.len()];

    for (label_ref, rect) in candidates.into_iter().rev() {
        if occupied.iter().any(|kept| rect.overlaps(*kept)) {
            match label_ref {
                LabelRef::Airspace(index) => keep_airspace[index] = false,
                LabelRef::Point(index) => keep_point[index] = false,
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

fn airspace_label_rect(label: &AirspaceDisplayLabel) -> Option<LabelRect> {
    if !label.screen_x.is_finite() || !label.screen_y.is_finite() {
        return None;
    }
    let width = label
        .glyph
        .upper
        .chars()
        .count()
        .max(label.glyph.lower.chars().count()) as f64
        * 8.2
        + 10.0;
    let height = 30.0;
    Some(centered_rect(label.screen_x, label.screen_y, width, height))
}

fn point_feature_label_rect(feature: &VisibleMapFeature) -> Option<LabelRect> {
    let text = feature.label.trim();
    if text.is_empty() || !feature.screen_x.is_finite() || !feature.screen_y.is_finite() {
        return None;
    }
    let style = feature.style_class.to_ascii_lowercase();
    let kind = feature.kind.to_ascii_lowercase();
    let label_y = if style == "airport" || kind == "airport" {
        -24.0
    } else if style == "nav" || kind.contains("vor") {
        -24.0
    } else if style.starts_with("obstacle") || kind == "obs" || kind == "obstacle" {
        -14.0
    } else {
        -15.0
    };
    let font_px = if style.starts_with("obstacle") {
        12.0
    } else {
        14.0
    };
    let width = text.chars().count() as f64 * font_px * 0.64 + 8.0;
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

fn query_tfr_overlay(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    center_world: WorldPoint,
    scale: f64,
    tfr_payload: Option<&TfrProductPayload>,
) -> TfrOverlayProjection {
    if width_px <= 0.0 || height_px <= 0.0 || viewport.zoom < AIRSPACE_MIN_DISPLAY_ZOOM {
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
        if let Some(label_point) = tfr_label_screen_point(
            area,
            &projected_points,
            center_world,
            scale,
            width_px,
            height_px,
        ) {
            labels.push(AirspaceDisplayLabel {
                feature_id: format!("tfr:{}:{}", area.notam_id.trim(), area.area_index),
                glyph: airspace_limit_glyph(
                    tfr_limit_label(&area.upper_limit),
                    tfr_limit_label(&area.lower_limit),
                    "tfr",
                ),
                screen_x: label_point.x,
                screen_y: label_point.y,
            });
        }
        paths.push(AirspaceDisplayPath {
            id: format!("tfr:{}:{}", area.notam_id.trim(), area.area_index),
            name: area.notam_id.trim().to_string(),
            style_key: "tfr".to_string(),
            style: tfr_display_style(),
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
    center_world: WorldPoint,
    scale: f64,
    width_px: f64,
    height_px: f64,
) -> Option<AirspaceScreenPoint> {
    if !tfr_polygon_can_fit_label(area, projected_points) {
        return None;
    }
    let centroid = tfr_polygon_centroid(area)?;
    let point = world_to_screen(center_world, scale, width_px, height_px, centroid);
    if point.x < 0.0 || point.x > width_px || point.y < 0.0 || point.y > height_px {
        return None;
    }
    Some(AirspaceScreenPoint {
        x: point.x,
        y: point.y,
    })
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
) -> bool {
    let Some((bbox_width, bbox_height)) = projected_bbox_size(projected_points) else {
        return false;
    };
    let label_width = tfr_fraction_label_width_px(area);
    let label_height = 22.0;
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
    needed_ref_tiles: Vec<VectorTileRequest>,
    needed_features: Vec<AirspaceFeatureRequest>,
    needed_label_tiles: Vec<VectorTileRequest>,
    paths: Vec<AirspaceDisplayPath>,
    labels: Vec<AirspaceDisplayLabel>,
    warnings: Vec<MapOverlayWarning>,
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

fn query_airspace_overlay(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    config: &MapOverlayConfig,
    center_world: WorldPoint,
    scale: f64,
    ref_tile_cache: &HashMap<String, AirspaceReferenceTilePayload>,
    feature_cache: &HashMap<String, AirspaceFeaturePayload>,
    label_tile_cache: &HashMap<String, AirspaceLabelTilePayload>,
) -> AirspaceOverlayProjection {
    if viewport.zoom < AIRSPACE_MIN_DISPLAY_ZOOM || width_px <= 0.0 || height_px <= 0.0 {
        return AirspaceOverlayProjection {
            needed_ref_tiles: Vec::new(),
            needed_features: Vec::new(),
            needed_label_tiles: Vec::new(),
            paths: Vec::new(),
            labels: Vec::new(),
            warnings: Vec::new(),
        };
    }

    let ref_zoom = airspace_reference_zoom(viewport.zoom, config);
    let ref_tiles = visible_layer_tile_window("airspace", ref_zoom, viewport, width_px, height_px);
    let mut needed_ref_tiles = Vec::new();
    let mut feature_ids = BTreeSet::new();
    for tile in ref_tiles {
        let key = tile_key(&tile.layer, tile.z, tile.x, tile.y);
        let Some(payload) = ref_tile_cache.get(&key) else {
            needed_ref_tiles.push(tile);
            continue;
        };
        feature_ids.extend(payload.refs.iter().cloned());
    }

    let mut needed_features = Vec::new();
    let mut paths = Vec::new();
    let mut limit_hit = false;
    let mut decoration_budget = AirspaceDecorationBudget::default();
    for feature_id in feature_ids {
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

    let label_zoom = airspace_label_zoom(viewport.zoom, config);
    let label_tiles =
        visible_layer_tile_window("airspace-labels", label_zoom, viewport, width_px, height_px);
    let mut needed_label_tiles = Vec::new();
    let mut label_by_feature = HashMap::<String, AirspaceLabelCandidate>::new();
    for tile in label_tiles {
        let key = tile_key(&tile.layer, tile.z, tile.x, tile.y);
        let Some(payload) = label_tile_cache.get(&key) else {
            needed_label_tiles.push(tile);
            continue;
        };
        for label in &payload.labels {
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
            let candidate = AirspaceLabelCandidate {
                rank: label.rank,
                label: {
                    let style_key = airspace_style_key(&label.style_hint);
                    let Some(glyph) =
                        airspace_limit_glyph_from_label(label.text.trim(), &style_key)
                    else {
                        continue;
                    };
                    AirspaceDisplayLabel {
                        feature_id: label.feature_id.clone(),
                        glyph,
                        screen_x: point.x,
                        screen_y: point.y,
                    }
                },
            };
            let entry = label_by_feature
                .entry(candidate.label.feature_id.clone())
                .or_insert_with(|| candidate.clone());
            if airspace_label_candidate_is_better(&candidate, entry) {
                *entry = candidate;
            }
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

    let mut warnings = Vec::new();
    if limit_hit {
        warnings.push(MapOverlayWarning {
            code: "airspace_display_feature_limit".to_string(),
            message: format!(
                "display capped at {} visible airspace features",
                AIRSPACE_DISPLAY_FEATURE_LIMIT
            ),
        });
    }
    if decoration_budget.limit_hit {
        warnings.push(MapOverlayWarning {
            code: "airspace_feather_limit".to_string(),
            message: format!(
                "display capped at {} airspace feather ticks",
                AIRSPACE_FEATHER_LIMIT
            ),
        });
    }
    if decoration_budget.missing_interior_side > 0 || decoration_budget.invalid_interior_side > 0 {
        warnings.push(MapOverlayWarning {
            code: "airspace_interior_side_contract".to_string(),
            message: format!(
                "feathered airspace paths require interior_side; {} missing, {} invalid",
                decoration_budget.missing_interior_side, decoration_budget.invalid_interior_side
            ),
        });
    }

    AirspaceOverlayProjection {
        needed_ref_tiles,
        needed_features,
        needed_label_tiles,
        paths,
        labels,
        warnings,
    }
}

fn tfr_display_style() -> AirspaceDisplayStyle {
    AirspaceDisplayStyle {
        fill_color_key: "tfr_red".to_string(),
        fill_opacity: 0.08,
        strokes: vec![AirspaceDisplayStroke {
            color_key: "tfr_red".to_string(),
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
            let points = path
                .points
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
        decorations: airspace_decorations(&style_key, &paths, decoration_budget),
        style_key,
        paths,
    }
}

fn airspace_decorations(
    style_key: &str,
    paths: &[AirspaceDisplaySubpath],
    budget: &mut AirspaceDecorationBudget,
) -> Vec<AirspaceDecorationPath> {
    let Some((color_key, width_px)) = airspace_feather_style(style_key) else {
        return Vec::new();
    };
    let mut feather_paths = Vec::new();
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
        feather_paths.extend(airspace_feathers_for_path(path, interior_side, budget));
        if budget.limit_hit {
            break;
        }
    }
    if feather_paths.is_empty() {
        return Vec::new();
    }
    vec![AirspaceDecorationPath {
        color_key,
        width_px,
        line_cap: "butt".to_string(),
        paths: feather_paths,
    }]
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
        "tfr" => "tfr_red",
        _ => "class_b_d_blue",
    }
}

fn airspace_feathers_for_path(
    path: &AirspaceDisplaySubpath,
    interior_side: AirspaceInteriorSide,
    budget: &mut AirspaceDecorationBudget,
) -> Vec<AirspaceDisplaySubpath> {
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
            feathers.push(AirspaceDisplaySubpath {
                closed: false,
                interior_side: None,
                points: vec![
                    AirspaceScreenPoint {
                        x: round_screen_coordinate(base_x),
                        y: round_screen_coordinate(base_y),
                    },
                    AirspaceScreenPoint {
                        x: round_screen_coordinate(base_x + nx * FEATHER_LENGTH_PX),
                        y: round_screen_coordinate(base_y + ny * FEATHER_LENGTH_PX),
                    },
                ],
            });
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
        let keep = simplified.last().map_or(true, |last| {
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
    let mut obstacle_variant = None;
    if record.style_class == "obstacle" {
        let obstacle = record.obstacle.as_ref()?;
        let altitude_ft = obstacle.top_msl_ft;
        if let Some(ownship_altitude_ft) = ownship_altitude_ft.filter(|value| value.is_finite()) {
            let delta_ft = altitude_ft - ownship_altitude_ft;
            if delta_ft < -OBSTACLE_BELOW_OWNERSHIP_HIDE_FT {
                return None;
            }
            style_class = if delta_ft >= -OBSTACLE_DANGER_LOWER_FT {
                "obstacle-danger".to_string()
            } else if delta_ft >= -OBSTACLE_CAUTION_LOWER_FT {
                "obstacle-caution".to_string()
            } else {
                "obstacle-muted".to_string()
            };
        } else {
            style_class = "obstacle-caution".to_string();
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
        style_class,
        obstacle_variant,
        towered: record.towered.unwrap_or(false),
        fuel_available: record.fuel_available.unwrap_or(false),
        has_paved_runway: record.has_paved_runway,
        heliport: record.heliport,
        has_water_runway: record.has_water_runway,
        runway_length_ratio: runway_length_ratio(record.longest_runway_length_ft),
        longest_runway_heading_true_deg: record.longest_runway_heading_true_deg,
    })
}

pub fn tile_key(layer: &str, z: u32, x: u32, y: u32) -> String {
    format!("{layer}:{z}/{x}/{y}")
}

fn display_label(record: &PointVectorRecord) -> String {
    if record.style_class == "airport" || record.kind.eq_ignore_ascii_case("airport") {
        if let Some(ident) = record
            .id
            .strip_prefix("airports:")
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let trimmed = if ident.len() == 4 && ident.starts_with('K') {
                &ident[1..]
            } else {
                ident
            };
            return trimmed.to_uppercase();
        }
    }
    if record.style_class == "nav" && is_vor_family_kind(&record.kind) {
        if let Some(ident) = record
            .id
            .strip_prefix("nav:")
            .map(|tail| tail.split(':').next().unwrap_or(tail).trim())
            .filter(|value| !value.is_empty())
        {
            return ident.to_uppercase();
        }
    }
    record.label.trim().to_uppercase()
}

fn is_vor_family_kind(kind: &str) -> bool {
    matches!(
        kind.to_ascii_lowercase().as_str(),
        "vor" | "vor/dme" | "vortac"
    )
}

fn should_display_record(record: &PointVectorRecord) -> bool {
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
    let world = lat_lon_to_world(position);
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

    fn test_map_overlay_config() -> MapOverlayConfig {
        MapOverlayConfig {
            airspace_reference_tile_min_zoom: 0,
            airspace_reference_tile_max_zoom: 12,
            airspace_label_tile_min_zoom: 0,
            airspace_label_tile_max_zoom: 12,
            obstacle_layer: None,
            metar_layer: Some(PointTileLayerConfig {
                min_zoom: 5,
                max_zoom: 7,
                available_zooms: vec![5, 6, 7],
            }),
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
        super::query_map_overlay(
            viewport,
            width_px,
            height_px,
            &test_map_overlay_config(),
            true,
            false,
            None,
            point_tile_cache,
            &HashMap::new(),
            None,
            airspace_ref_tile_cache,
            airspace_feature_cache,
            airspace_label_tile_cache,
            None,
        )
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
        let tiles =
            visible_point_tile_window(&test_map_overlay_config(), &viewport, 1200.0, 900.0, None);
        assert!(tiles.iter().any(|tile| tile.layer == "airport"));
        assert!(!tiles.iter().any(|tile| tile.layer == "fix"));
        assert!(tiles.iter().any(|tile| tile.layer == "nav"));
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
        assert!(result
            .needed_airspace_label_tiles
            .iter()
            .all(|tile| tile.z == 11));

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
            .needed_airspace_label_tiles
            .iter()
            .all(|tile| tile.z == test_map_overlay_config().airspace_label_tile_max_zoom));
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

        assert!(result
            .needed_airspace_ref_tiles
            .iter()
            .all(|tile| tile.z == 9));
    }

    #[test]
    fn vector_manifest_config_controls_airspace_tile_zoom_ranges() {
        let config = map_overlay_config_from_vector_manifest_json(
            r#"{"airspace":{"reference_tile_min_zoom":3,"reference_tile_max_zoom":11,"label_tile_min_zoom":2,"label_tile_max_zoom":10}}"#,
        )
        .expect("manifest should parse");

        assert_eq!(config.airspace_reference_tile_min_zoom, 3);
        assert_eq!(config.airspace_reference_tile_max_zoom, 11);
        assert_eq!(config.airspace_label_tile_min_zoom, 2);
        assert_eq!(config.airspace_label_tile_max_zoom, 10);
    }

    #[test]
    fn vector_manifest_config_controls_metar_tile_zoom_levels() {
        let config = map_overlay_config_from_vector_manifest_json(
            r#"{
                "point_layers": {
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
        let low_zoom = super::query_map_overlay(
            &viewport,
            240.0,
            240.0,
            &config,
            false,
            true,
            None,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
        );
        assert!(low_zoom.needed_metar_tiles.iter().all(|tile| tile.z == 5));

        let high_zoom = super::query_map_overlay(
            &MapViewport {
                zoom: 9.0,
                ..viewport
            },
            240.0,
            240.0,
            &config,
            false,
            true,
            None,
            &HashMap::new(),
            &HashMap::new(),
            None,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
        );
        assert!(high_zoom.needed_metar_tiles.iter().all(|tile| tile.z == 7));
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
            .needed_airspace_label_tiles
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
            zoom: 8.0,
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
            schema_version: 2,
            version_label: "test".to_string(),
            metar_count: Some(1),
            metars_by_station,
        };
        let result = super::query_map_overlay(
            &viewport,
            240.0,
            240.0,
            &test_map_overlay_config(),
            false,
            true,
            None,
            &HashMap::new(),
            &metar_tile_cache,
            Some(&metars),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
        );

        assert!(result.needed_metar_tiles.is_empty());
        assert!(!result.needed_metars);
        assert_eq!(result.visible_metars.len(), 1);
        assert_eq!(result.visible_metars[0].station_id, "KAAA");
        assert_eq!(result.visible_metars[0].flight_category, "mvfr");
        assert_eq!(result.visible_metars[0].ceiling_amount, "sct");
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
            schema_version: 2,
            version_label: "test".to_string(),
            metar_count: Some(1),
            metars_by_station,
        };
        let taf_raw_text = "TAF KAAA 010000Z 0100/0124 00000KT P6SM SCT020 BECMG 0102/0104 BKN030 FM010600 22008KT P6SM SCT050";
        let tafs = TafProductPayload {
            schema_version: 1,
            version_label: "test".to_string(),
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

        let result = query_map_selection(
            &viewport,
            240.0,
            240.0,
            &test_map_overlay_config(),
            None,
            viewport.center,
            32.0,
            &HashMap::new(),
            &metar_tile_cache,
            Some(&metars),
            Some(&tafs),
            &HashMap::new(),
            None,
            &mut |_| AirportPlateAvailability::default(),
        );
        let weather = result
            .categories
            .iter()
            .find(|category| category.id == "weather")
            .expect("weather category");
        let item = weather.items.first().expect("METAR selection item");

        assert_eq!(item.label, "KAAA");
        assert_eq!(item.detail_text.as_deref(), Some(raw_text));
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
        assert!(item.actions.iter().any(|action| action.id == "metar"));
        let taf_action = item
            .actions
            .iter()
            .find(|action| action.id == "taf")
            .expect("TAF action");
        assert!(taf_action.enabled);
        assert_eq!(
            taf_action.detail_text.as_deref(),
            Some("TAF KAAA 010000Z 0100/0124 00000KT P6SM SCT020\nBECMG 0102/0104 BKN030\nFM010600 22008KT P6SM SCT050")
        );
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
            paths: vec![AirspaceFeaturePath {
                role: "boundary".to_string(),
                closed: true,
                interior_side: None,
                points: vec![[-120.0, 46.0], [-119.0, 46.0], [-119.0, 47.0]],
            }],
        };

        let item = selection_item_for_airspace(&feature);

        assert_eq!(item.label, "HANFORD NSA, WA");
        assert_eq!(item.sublabel, "NHANFORD");
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
                paths: vec![AirspaceFeaturePath {
                    role: "boundary".to_string(),
                    closed: true,
                    interior_side: Some("left".to_string()),
                    points: vec![[-0.1, -0.1], [0.1, -0.1], [0.1, 0.1], [-0.1, 0.1]],
                }],
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
                paths: vec![AirspaceFeaturePath {
                    role: "boundary".to_string(),
                    closed: true,
                    interior_side: None,
                    points: vec![[-0.1, -0.1], [0.1, -0.1], [0.1, 0.1], [-0.1, 0.1]],
                }],
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
            .warnings
            .iter()
            .any(|warning| warning.code == "airspace_interior_side_contract"));
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
        let feathers = airspace_feathers_for_path(&path, AirspaceInteriorSide::Left, &mut budget);

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
        let left = airspace_feathers_for_path(&path, AirspaceInteriorSide::Left, &mut left_budget);
        let mut right_budget = AirspaceDecorationBudget::default();
        let right =
            airspace_feathers_for_path(&path, AirspaceInteriorSide::Right, &mut right_budget);

        assert!(!left.is_empty());
        assert_eq!(left.len(), right.len());
        assert_eq!(left[0].points[0], right[0].points[0]);
        assert!(
            (left[0].points[1].y - left[0].points[0].y)
                * (right[0].points[1].y - right[0].points[0].y)
                < 0.0,
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
        let scale = 2.0_f64.powf(viewport.zoom);
        let center_world = lat_lon_to_world(viewport.center);
        let payload = TfrProductPayload {
            schema_version: 1,
            version_label: "test".to_string(),
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
                avare_text: String::new(),
            }],
        };

        let result = query_tfr_overlay(
            &viewport,
            width_px,
            height_px,
            center_world,
            scale,
            Some(&payload),
        );

        assert_eq!(result.paths.len(), 1);
        assert_eq!(result.labels.len(), 1);
        assert_eq!(result.labels[0].glyph.style_key, "tfr");
        assert_eq!(result.labels[0].glyph.upper, "FL180");
        assert_eq!(result.labels[0].glyph.lower, "SFC");
        assert!((result.labels[0].screen_x - width_px / 2.0).abs() < 1.0);
        assert!((result.labels[0].screen_y - height_px / 2.0).abs() < 1.0);
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
        let scale = 2.0_f64.powf(viewport.zoom);
        let center_world = lat_lon_to_world(viewport.center);
        let payload = TfrProductPayload {
            schema_version: 1,
            version_label: "test".to_string(),
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
                avare_text: String::new(),
            }],
        };

        let result = query_tfr_overlay(
            &viewport,
            width_px,
            height_px,
            center_world,
            scale,
            Some(&payload),
        );

        assert_eq!(result.paths.len(), 1);
        assert!(result.labels.is_empty());
    }

    #[test]
    fn caps_visible_features_and_warns() {
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
            visible_point_tile_window(&test_map_overlay_config(), &viewport, 1200.0, 900.0, None);
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
                    .map(|index| PointVectorRecord {
                        id: format!("fix:{index}"),
                        kind: "yrep-pt".to_string(),
                        lat: 47.36,
                        lon: -121.98,
                        label: format!("FIX{index}"),
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
                    })
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
        assert_eq!(result.visible_features.len(), VECTOR_DISPLAY_FEATURE_LIMIT);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].code, "vector_display_feature_limit");
    }

    #[test]
    fn suppresses_lower_drawn_overlapping_point_labels() {
        let mut features = vec![
            test_visible_feature("airports:KABC", "airport", "airport", "KABC", 100.0, 100.0),
            test_visible_feature("nav:ABC:VOR", "VORTAC", "nav", "ABC", 100.0, 100.0),
        ];
        let mut airspace_labels = Vec::new();

        suppress_overlapping_vector_labels(&mut features, &mut airspace_labels);

        assert_eq!(features[0].label, "");
        assert_eq!(features[1].label, "ABC");
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

        suppress_overlapping_vector_labels(&mut features, &mut airspace_labels);

        assert!(airspace_labels.is_empty());
        assert_eq!(features[0].label, "ABC");
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
            visible_point_tile_window(&test_map_overlay_config(), &viewport, 1200.0, 900.0, None)
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
                records: vec![PointVectorRecord {
                    id: "airports:KSEA".to_string(),
                    kind: "airport".to_string(),
                    lat: 47.36,
                    lon: -121.98,
                    label: "SEATTLE".to_string(),
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
                }],
            },
        );

        let result = query_map_selection(
            &viewport,
            1200.0,
            900.0,
            &test_map_overlay_config(),
            None,
            viewport.center,
            32.0,
            &cache,
            &HashMap::new(),
            None,
            None,
            &HashMap::new(),
            None,
            &mut |_| AirportPlateAvailability {
                plates: true,
                csup: true,
            },
        );

        assert_eq!(result.categories[0].id, "airport");
        assert_eq!(result.categories[0].items[0].label, "KSEA");
        assert_eq!(
            result.categories[0].items[0].description.as_deref(),
            Some("Elev 433")
        );
        assert!(!result.categories[0].items[0]
            .actions
            .iter()
            .any(|action| action.id == "elevation"));
        assert_eq!(result.categories[1].id, "navaid");
        assert!(result.categories[1]
            .items
            .iter()
            .any(|item| item.id.starts_with("spot:")));
        assert_eq!(result.categories[3].id, "weather");
    }

    #[test]
    fn map_selection_offers_remove_for_top_level_waypoint_already_in_plan() {
        let record = PointVectorRecord {
            id: "airports:KSEA".to_string(),
            kind: "airport".to_string(),
            lat: 47.36,
            lon: -121.98,
            label: "SEATTLE".to_string(),
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
        };
        let symbol = point_vector_record_to_symbol_feature(&record, None).unwrap();
        let plan = FlightPlan {
            id: "plan".to_string(),
            name: "Plan".to_string(),
            legs: Vec::new(),
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
            cruise_altitude_ft: None,
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
            Some(&TafRecord {
                raw_text: "TAF KSEA 010000Z 0100/0124 00000KT P6SM SCT020 BECMG 0102/0104 BKN030 FM010600 22008KT P6SM SCT050"
                    .to_string(),
                issued_at_utc: Some("2026-05-03T00:00:00.000Z".to_string()),
                station_id: "KSEA".to_string(),
                longitude: record.lon,
                latitude: record.lat,
            }),
        );
        let remove = item
            .actions
            .iter()
            .find(|action| action.id == "remove_from_flight_plan")
            .expect("remove action");

        assert_eq!(item.nav_ref, Some(NavRef::Airport("KSEA".to_string())));
        assert_eq!(remove.label, "Remove from flight plan");
        assert!(remove.enabled);
        assert!(!remove.display_only);
        assert!(item
            .actions
            .iter()
            .any(|action| action.id == "plates" && action.enabled));
        assert!(item
            .actions
            .iter()
            .any(|action| action.id == "csup" && action.enabled));
        let taf = item
            .actions
            .iter()
            .find(|action| action.id == "taf")
            .expect("TAF action");
        assert!(taf.enabled);
        assert_eq!(
            taf.detail_text.as_deref(),
            Some("TAF KSEA 010000Z 0100/0124 00000KT P6SM SCT020\nBECMG 0102/0104 BKN030\nFM010600 22008KT P6SM SCT050")
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
        };
        let feature = point_vector_record_to_symbol_feature(&record, None)
            .expect("VORTAC should be displayed");

        assert_eq!(feature.label, "ELN");
    }

    #[test]
    fn vor_selection_uses_frequency_as_description() {
        let record = PointVectorRecord {
            id: "nav:SEA:VOR".to_string(),
            kind: "VOR/DME".to_string(),
            lat: 47.435,
            lon: -122.309,
            label: "SEATTLE 118.8".to_string(),
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
            visible_point_tile_window(&test_map_overlay_config(), &viewport, 1200.0, 900.0, None)
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
                        lat: 47.36,
                        lon: -121.98,
                        label: "SEATTLE".to_string(),
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
                    },
                    PointVectorRecord {
                        id: "airports:WN50".to_string(),
                        kind: "airport".to_string(),
                        lat: 47.3605,
                        lon: -121.9805,
                        label: "PRIVATE".to_string(),
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
                    },
                    PointVectorRecord {
                        id: "airports:W57".to_string(),
                        kind: "airport".to_string(),
                        lat: 47.361,
                        lon: -121.981,
                        label: "WATER".to_string(),
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
                    },
                    PointVectorRecord {
                        id: "airports:H1".to_string(),
                        kind: "heliport".to_string(),
                        lat: 47.362,
                        lon: -121.982,
                        label: "HELI".to_string(),
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

        let selection = query_map_selection(
            &viewport,
            1200.0,
            900.0,
            &test_map_overlay_config(),
            None,
            viewport.center,
            32.0,
            &cache,
            &HashMap::new(),
            None,
            None,
            &HashMap::new(),
            None,
            &mut |_| AirportPlateAvailability::default(),
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
                "airports:WN50",
                "airports:W57",
                "airports:H1"
            ]
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
            style_class: style_class.to_string(),
            obstacle_variant: None,
            screen_x,
            screen_y,
            towered: false,
            fuel_available: false,
            has_paved_runway: None,
            heliport: None,
            has_water_runway: None,
            runway_length_ratio: 0.0,
            longest_runway_heading_true_deg: None,
        }
    }
}
