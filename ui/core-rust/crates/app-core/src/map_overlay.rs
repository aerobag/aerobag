use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{
    geometry::LatLon, great_circle_distance_nm, AppError, AppErrorKind, AppResult, MapViewport,
};

pub const VECTOR_DISPLAY_FEATURE_LIMIT: usize = 500;
pub const AIRSPACE_DISPLAY_FEATURE_LIMIT: usize = 700;
pub const AIRSPACE_FEATHER_LIMIT: usize = 5_000;
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
    pub vertical_label: String,
    pub bbox: [f64; 4],
    pub paths: Vec<AirspaceFeaturePath>,
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
    pub label: String,
    pub style_key: String,
    pub style: AirspaceDisplayStyle,
    pub paths: Vec<AirspaceDisplaySubpath>,
    pub decorations: Vec<AirspaceDecorationPath>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceDisplayLabel {
    pub feature_id: String,
    pub text: String,
    pub style_key: String,
    pub screen_x: f64,
    pub screen_y: f64,
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
    pub needed_airspace_ref_tiles: Vec<VectorTileRequest>,
    pub needed_airspace_features: Vec<AirspaceFeatureRequest>,
    pub needed_airspace_label_tiles: Vec<VectorTileRequest>,
    pub needed_tfrs: bool,
    pub visible_features: Vec<VisibleMapFeature>,
    pub airspace_paths: Vec<AirspaceDisplayPath>,
    pub tfr_paths: Vec<AirspaceDisplayPath>,
    pub airspace_labels: Vec<AirspaceDisplayLabel>,
    pub warnings: Vec<MapOverlayWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapOverlayConfig {
    pub airspace_reference_tile_min_zoom: u32,
    pub airspace_reference_tile_max_zoom: u32,
    pub airspace_label_tile_min_zoom: u32,
    pub airspace_label_tile_max_zoom: u32,
    pub obstacle_layer: Option<ObstacleLayerConfig>,
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
    Ok(MapOverlayConfig {
        airspace_reference_tile_min_zoom: manifest.airspace.reference_tile_min_zoom,
        airspace_reference_tile_max_zoom: manifest.airspace.reference_tile_max_zoom,
        airspace_label_tile_min_zoom: manifest.airspace.label_tile_min_zoom,
        airspace_label_tile_max_zoom: manifest.airspace.label_tile_max_zoom,
        obstacle_layer,
    })
}

fn obstacle_layer_config_from_manifest(
    manifest: &VectorPointLayerManifest,
) -> AppResult<ObstacleLayerConfig> {
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
            message: "vector overlay manifest obstacle layer is missing available zooms"
                .to_string(),
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
            message: "vector overlay manifest has inverted obstacle zoom range".to_string(),
        });
    }
    let mut zoom_levels = HashMap::new();
    let mut high_detail_zoom = *available_zooms.last().unwrap();
    for level in &manifest.zoom_levels {
        zoom_levels.insert(level.zoom, level.clone());
        if !level.filtered {
            high_detail_zoom = high_detail_zoom.max(level.zoom);
        }
    }
    Ok(ObstacleLayerConfig {
        min_zoom,
        max_zoom,
        available_zooms,
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
    let clamped = desired_zoom.clamp(config.min_zoom, config.max_zoom);
    config
        .available_zooms
        .iter()
        .copied()
        .filter(|zoom| *zoom <= clamped)
        .max()
        .unwrap_or_else(|| *config.available_zooms.first().unwrap())
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
    obstacle_context: Option<&ObstacleOverlayContext>,
    point_tile_cache: &HashMap<String, PointTilePayload>,
    airspace_ref_tile_cache: &HashMap<String, AirspaceReferenceTilePayload>,
    airspace_feature_cache: &HashMap<String, AirspaceFeaturePayload>,
    airspace_label_tile_cache: &HashMap<String, AirspaceLabelTilePayload>,
    tfr_payload: Option<&TfrProductPayload>,
) -> MapOverlayQueryResult {
    let tile_window =
        visible_point_tile_window(config, viewport, width_px, height_px, obstacle_context);
    let mut needed_point_tiles = Vec::new();
    let mut visible_features = Vec::new();
    let mut limit_hit = false;
    let center_world = lat_lon_to_world(viewport.center);
    let scale = 2.0_f64.powf(viewport.zoom);

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

    let airspace = query_airspace_overlay(
        viewport,
        width_px,
        height_px,
        config,
        center_world,
        scale,
        airspace_ref_tile_cache,
        airspace_feature_cache,
        airspace_label_tile_cache,
    );
    let mut warnings = warnings;
    warnings.extend(airspace.warnings);
    let tfrs = query_tfr_overlay(
        viewport,
        width_px,
        height_px,
        center_world,
        scale,
        tfr_payload,
    );

    MapOverlayQueryResult {
        needed_point_tiles,
        needed_airspace_ref_tiles: airspace.needed_ref_tiles,
        needed_airspace_features: airspace.needed_features,
        needed_airspace_label_tiles: airspace.needed_label_tiles,
        needed_tfrs: tfrs.needed_tfrs,
        visible_features,
        airspace_paths: airspace.paths,
        tfr_paths: tfrs.paths,
        airspace_labels: {
            let mut labels = airspace.labels;
            labels.extend(tfrs.labels);
            labels
        },
        warnings,
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
                text: format!(
                    "{}/{}",
                    tfr_limit_label(&area.upper_limit),
                    tfr_limit_label(&area.lower_limit)
                ),
                style_key: "tfr".to_string(),
                screen_x: label_point.x,
                screen_y: label_point.y,
            });
        }
        paths.push(AirspaceDisplayPath {
            id: format!("tfr:{}:{}", area.notam_id.trim(), area.area_index),
            name: area.notam_id.trim().to_string(),
            label: area.notam_id.trim().to_string(),
            style_key: "tfr".to_string(),
            style: AirspaceDisplayStyle {
                fill_color_key: "tfr_red".to_string(),
                fill_opacity: 0.08,
                strokes: vec![AirspaceDisplayStroke {
                    color_key: "tfr_red".to_string(),
                    width_px: 2.0,
                    dash_px: Vec::new(),
                    line_cap: "round".to_string(),
                }],
            },
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
                label: AirspaceDisplayLabel {
                    feature_id: label.feature_id.clone(),
                    text: label.text.trim().to_string(),
                    style_key: airspace_style_key(&label.style_hint),
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
    }
    let mut labels = label_by_feature
        .into_values()
        .map(|candidate| candidate.label)
        .collect::<Vec<_>>();
    labels.sort_by(|left, right| {
        left.feature_id
            .cmp(&right.feature_id)
            .then_with(|| left.text.cmp(&right.text))
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
        label: feature.vertical_label.clone(),
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
    use app_fixtures::fixture_vector_tile_root as app_fixture_vector_tile_root;
    use std::fs;
    use std::sync::OnceLock;

    fn test_map_overlay_config() -> MapOverlayConfig {
        MapOverlayConfig {
            airspace_reference_tile_min_zoom: 0,
            airspace_reference_tile_max_zoom: 12,
            airspace_label_tile_min_zoom: 0,
            airspace_label_tile_max_zoom: 12,
            obstacle_layer: None,
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
            None,
            point_tile_cache,
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
                        text: "A-OFFSCREEN".to_string(),
                        lon: 10.0,
                        lat: 0.0,
                        rank: 0,
                        score: Some(1.0),
                        style_hint: "class_b".to_string(),
                    },
                    AirspaceLabelRecord {
                        feature_id: "feature-a".to_string(),
                        text: "A-RANK-2".to_string(),
                        lon: 0.0,
                        lat: 0.0,
                        rank: 2,
                        score: Some(0.2),
                        style_hint: "class_b".to_string(),
                    },
                    AirspaceLabelRecord {
                        feature_id: "feature-a".to_string(),
                        text: "A-RANK-1".to_string(),
                        lon: 0.1,
                        lat: 0.0,
                        rank: 1,
                        score: Some(0.1),
                        style_hint: "class_b".to_string(),
                    },
                    AirspaceLabelRecord {
                        feature_id: "feature-b".to_string(),
                        text: "B-RANK-1".to_string(),
                        lon: 0.0,
                        lat: 0.1,
                        rank: 1,
                        score: Some(0.9),
                        style_hint: "class_c".to_string(),
                    },
                    AirspaceLabelRecord {
                        feature_id: "feature-b".to_string(),
                        text: "B-RANK-0".to_string(),
                        lon: 0.1,
                        lat: 0.1,
                        rank: 0,
                        score: Some(0.1),
                        style_hint: "class_c".to_string(),
                    },
                ],
            },
        );

        let result = query_map_overlay(
            &viewport,
            100.0,
            100.0,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &label_cache,
        );

        assert_eq!(result.airspace_labels.len(), 2);
        assert_eq!(result.airspace_labels[0].feature_id, "feature-a");
        assert_eq!(result.airspace_labels[1].feature_id, "feature-b");
        assert_eq!(result.airspace_labels[0].text, "A-RANK-1");
        assert_eq!(result.airspace_labels[1].text, "B-RANK-0");
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
                vertical_label: "100/50".to_string(),
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
                vertical_label: "100/50".to_string(),
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
        assert_eq!(result.labels[0].style_key, "tfr");
        assert_eq!(result.labels[0].text, "FL180/SFC");
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
    fn vor_symbol_labels_omit_frequency() {
        let feature = point_vector_record_to_symbol_feature(
            &PointVectorRecord {
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
                obstacle: None,
            },
            None,
        )
        .expect("VORTAC should be displayed");

        assert_eq!(feature.label, "ELN");
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
    #[ignore = "requires generated vector fixtures under ui-target/web/generated-static/vectors"]
    fn real_vamps_viewport_returns_visible_fix_features() {
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.364_894_444_444_4,
                lon: -121.980_275,
            },
            zoom: 10.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let tile_root = fixture_vector_tile_root();
        let mut cache = HashMap::new();

        for tile in
            visible_point_tile_window(&test_map_overlay_config(), &viewport, 1200.0, 900.0, None)
        {
            if tile.layer != "fix" {
                continue;
            }
            let tile_path = tile_root
                .join(tile.x.to_string())
                .join(format!("{}.json", tile.y));
            let payload: PointTilePayload = serde_json::from_str(
                &fs::read_to_string(&tile_path)
                    .unwrap_or_else(|err| panic!("failed to read {}: {err}", tile_path.display())),
            )
            .unwrap_or_else(|err| panic!("failed to parse {}: {err}", tile_path.display()));
            cache.insert(tile_key(&tile.layer, tile.z, tile.x, tile.y), payload);
        }

        let result = query_map_overlay(
            &viewport,
            1200.0,
            900.0,
            &cache,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );
        assert!(
            !result.visible_features.is_empty(),
            "expected visible fix features for VAMPS viewport"
        );
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
    }

    fn fixture_vector_tile_root() -> &'static std::path::Path {
        static ROOT: OnceLock<std::path::PathBuf> = OnceLock::new();
        ROOT.get_or_init(|| app_fixture_vector_tile_root("fix", 9))
            .as_path()
    }
}
