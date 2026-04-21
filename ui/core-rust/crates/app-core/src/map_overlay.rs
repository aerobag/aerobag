use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::{geometry::LatLon, MapViewport};

pub const VECTOR_DISPLAY_FEATURE_LIMIT: usize = 300;
pub const AIRSPACE_DISPLAY_FEATURE_LIMIT: usize = 700;
pub const AIRSPACE_FEATHER_LIMIT: usize = 5_000;
const POINT_TILE_ZOOM: u32 = 9;
const AIRSPACE_MIN_DISPLAY_ZOOM: f64 = 6.0;
const AIRSPACE_REF_MIN_ZOOM: u32 = 0;
const AIRSPACE_REF_MAX_ZOOM: u32 = 8;
const AIRSPACE_LABEL_MIN_ZOOM: u32 = 0;
const AIRSPACE_LABEL_MAX_ZOOM: u32 = 12;
const AIRPORT_MIN_DISPLAY_ZOOM: f64 = 8.0;
const FIX_MIN_DISPLAY_ZOOM: f64 = 9.0;
const NAV_MIN_DISPLAY_ZOOM: f64 = 7.0;
const WORLD_SIZE: f64 = 256.0;
const MAX_LATITUDE: f64 = 85.051_128_78;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorTileRequest {
    pub layer: String,
    pub z: u32,
    pub x: u32,
    pub y: u32,
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
    pub points: Vec<[f64; 2]>,
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
    pub screen_x: f64,
    pub screen_y: f64,
    pub towered: bool,
    pub fuel_available: bool,
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
    pub towered: bool,
    pub fuel_available: bool,
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
    pub visible_features: Vec<VisibleMapFeature>,
    pub airspace_paths: Vec<AirspaceDisplayPath>,
    pub airspace_labels: Vec<AirspaceDisplayLabel>,
    pub warnings: Vec<MapOverlayWarning>,
}

pub fn visible_point_tile_window(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
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
    tiles
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

pub fn query_map_overlay(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    point_tile_cache: &HashMap<String, PointTilePayload>,
    airspace_ref_tile_cache: &HashMap<String, AirspaceReferenceTilePayload>,
    airspace_feature_cache: &HashMap<String, AirspaceFeaturePayload>,
    airspace_label_tile_cache: &HashMap<String, AirspaceLabelTilePayload>,
) -> MapOverlayQueryResult {
    let tile_window = visible_point_tile_window(viewport, width_px, height_px);
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
            let Some(symbol) = point_vector_record_to_symbol_feature(record) else {
                continue;
            };
            visible_features.push(VisibleMapFeature {
                id: record.id.clone(),
                kind: symbol.kind,
                label: symbol.label,
                style_class: symbol.style_class,
                screen_x: point.x,
                screen_y: point.y,
                towered: symbol.towered,
                fuel_available: symbol.fuel_available,
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
        center_world,
        scale,
        airspace_ref_tile_cache,
        airspace_feature_cache,
        airspace_label_tile_cache,
    );
    let mut warnings = warnings;
    warnings.extend(airspace.warnings);

    MapOverlayQueryResult {
        needed_point_tiles,
        needed_airspace_ref_tiles: airspace.needed_ref_tiles,
        needed_airspace_features: airspace.needed_features,
        needed_airspace_label_tiles: airspace.needed_label_tiles,
        visible_features,
        airspace_paths: airspace.paths,
        airspace_labels: airspace.labels,
        warnings,
    }
}

struct AirspaceOverlayProjection {
    needed_ref_tiles: Vec<VectorTileRequest>,
    needed_features: Vec<AirspaceFeatureRequest>,
    needed_label_tiles: Vec<VectorTileRequest>,
    paths: Vec<AirspaceDisplayPath>,
    labels: Vec<AirspaceDisplayLabel>,
    warnings: Vec<MapOverlayWarning>,
}

#[derive(Debug, Default)]
struct AirspaceDecorationBudget {
    used: usize,
    limit_hit: bool,
}

fn query_airspace_overlay(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
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

    let ref_zoom = airspace_reference_zoom(viewport.zoom);
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

    let label_zoom = airspace_label_zoom(viewport.zoom);
    let label_tiles =
        visible_layer_tile_window("airspace-labels", label_zoom, viewport, width_px, height_px);
    let mut needed_label_tiles = Vec::new();
    let mut labels = Vec::new();
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
            if point.x < -80.0
                || point.x > width_px + 80.0
                || point.y < -40.0
                || point.y > height_px + 40.0
            {
                continue;
            }
            labels.push(AirspaceDisplayLabel {
                feature_id: label.feature_id.clone(),
                text: label.text.trim().to_string(),
                style_key: airspace_style_key(&label.style_hint),
                screen_x: point.x,
                screen_y: point.y,
            });
        }
    }

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

    AirspaceOverlayProjection {
        needed_ref_tiles,
        needed_features,
        needed_label_tiles,
        paths,
        labels,
        warnings,
    }
}

fn airspace_reference_zoom(display_zoom: f64) -> u32 {
    display_zoom
        .floor()
        .clamp(AIRSPACE_REF_MIN_ZOOM as f64, AIRSPACE_REF_MAX_ZOOM as f64) as u32
}

fn airspace_label_zoom(display_zoom: f64) -> u32 {
    display_zoom.floor().clamp(
        AIRSPACE_LABEL_MIN_ZOOM as f64,
        AIRSPACE_LABEL_MAX_ZOOM as f64,
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
        feather_paths.extend(airspace_feathers_for_path(path, budget));
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

fn airspace_feather_style(style_key: &str) -> Option<(String, f64)> {
    match style_key {
        "moa" | "alert" => Some(("class_c_magenta".to_string(), 1.4)),
        "restricted" | "prohibited" => Some(("class_b_d_blue".to_string(), 1.4)),
        _ => None,
    }
}

fn airspace_feathers_for_path(
    path: &AirspaceDisplaySubpath,
    budget: &mut AirspaceDecorationBudget,
) -> Vec<AirspaceDisplaySubpath> {
    const FEATHER_SPACING_PX: f64 = 8.0;
    const FEATHER_LENGTH_PX: f64 = 8.0;
    let signed_area = polygon_signed_area(&path.points);
    if signed_area.abs() < 1.0 {
        return Vec::new();
    }
    let inward_sign = if signed_area > 0.0 { 1.0 } else { -1.0 };
    let mut feathers = Vec::new();
    for index in 0..path.points.len() {
        let start = &path.points[index];
        let end = &path.points[(index + 1) % path.points.len()];
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length = (dx * dx + dy * dy).sqrt();
        if length < FEATHER_SPACING_PX {
            continue;
        }
        let nx = -dy / length * inward_sign;
        let ny = dx / length * inward_sign;
        let mut distance = FEATHER_SPACING_PX * 0.5;
        while distance < length {
            if budget.used >= AIRSPACE_FEATHER_LIMIT {
                budget.limit_hit = true;
                return feathers;
            }
            let t = distance / length;
            let base_x = start.x + dx * t;
            let base_y = start.y + dy * t;
            feathers.push(AirspaceDisplaySubpath {
                closed: false,
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
            distance += FEATHER_SPACING_PX;
        }
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
            fill_opacity: 0.018,
            strokes: vec![AirspaceDisplayStroke {
                color_key: "class_b_d_blue".to_string(),
                width_px: 3.6,
                dash_px: vec![6.0, 4.0],
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
) -> Option<NavSymbolFeature> {
    should_display_record(record).then(|| NavSymbolFeature {
        kind: record.kind.clone(),
        label: display_label(record),
        style_class: record.style_class.clone(),
        towered: record.towered.unwrap_or(false),
        fuel_available: record.fuel_available.unwrap_or(false),
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

fn lat_lon_to_world(position: LatLon) -> WorldPoint {
    let clamped_lat = position.lat.clamp(-MAX_LATITUDE, MAX_LATITUDE);
    WorldPoint {
        x: ((position.lon + 180.0) / 360.0) * WORLD_SIZE,
        y: ((1.0 - clamped_lat.to_radians().tan().asinh() / std::f64::consts::PI) / 2.0)
            * WORLD_SIZE,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::OnceLock;
    use std::{fs, path::PathBuf};

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
        let tiles = visible_point_tile_window(&viewport, 1200.0, 900.0);
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
            .all(|tile| tile.z == AIRSPACE_LABEL_MAX_ZOOM));
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
        let window = visible_point_tile_window(&viewport, 1200.0, 900.0);
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
        let feature = point_vector_record_to_symbol_feature(&PointVectorRecord {
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
        })
        .expect("VORTAC should be displayed");

        assert_eq!(feature.label, "ELN");
    }

    #[test]
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

        for tile in visible_point_tile_window(&viewport, 1200.0, 900.0) {
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
        let airport_tile = visible_point_tile_window(&viewport, 1200.0, 900.0)
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

    fn fixture_vector_tile_root() -> &'static Path {
        static ROOT: OnceLock<PathBuf> = OnceLock::new();
        ROOT.get_or_init(|| {
            if let Some(value) = std::env::var_os("AEROBAG_FIXTURE_VECTOR_ROOT") {
                let path = PathBuf::from(value);
                if path.is_dir() {
                    return path;
                }
            }
            let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let ui_dir = manifest_dir
                .join("../../..")
                .canonicalize()
                .expect("resolve ui dir");
            let repo_root = ui_dir.parent().expect("ui dir parent");
            let target_root_raw = fs::read_to_string(ui_dir.join("target-root.txt"))
                .expect("read ui/target-root.txt");
            let target_root = repo_root
                .join(target_root_raw.trim())
                .canonicalize()
                .expect("resolve ui target root");
            let path = target_root.join("web/generated-static/vectors/points/fix/9");
            if path.is_dir() {
                return path;
            }
            panic!("unable to locate vector tile fixture root");
        })
        .as_path()
    }
}
