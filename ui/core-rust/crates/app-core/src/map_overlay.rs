use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{geometry::LatLon, MapViewport};

pub const VECTOR_DISPLAY_FEATURE_LIMIT: usize = 1000;
const FIX_TILE_ZOOM: u32 = 9;
const FIX_MIN_DISPLAY_ZOOM: f64 = 9.0;
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
pub struct VisibleMapFeature {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub style_class: String,
    pub screen_x: f64,
    pub screen_y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapOverlayWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapOverlayQueryResult {
    pub needed_fix_tiles: Vec<VectorTileRequest>,
    pub visible_features: Vec<VisibleMapFeature>,
    pub warnings: Vec<MapOverlayWarning>,
}

pub fn visible_fix_tile_window(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
) -> Vec<VectorTileRequest> {
    if viewport.zoom < FIX_MIN_DISPLAY_ZOOM || width_px <= 0.0 || height_px <= 0.0 {
        return Vec::new();
    }
    let center_world = lat_lon_to_world(viewport.center);
    let scale = 2.0_f64.powf(viewport.zoom);
    let min_world_x = center_world.x - width_px / 2.0 / scale;
    let max_world_x = center_world.x + width_px / 2.0 / scale;
    let min_world_y = center_world.y - height_px / 2.0 / scale;
    let max_world_y = center_world.y + height_px / 2.0 / scale;
    let tile_world_size = WORLD_SIZE / (2_u32.pow(FIX_TILE_ZOOM) as f64);
    let max_index = (2_u32.pow(FIX_TILE_ZOOM) - 1) as i32;
    let x_start = (min_world_x / tile_world_size).floor() as i32;
    let x_end = (max_world_x / tile_world_size).floor() as i32;
    let y_start = (min_world_y / tile_world_size).floor() as i32;
    let y_end = (max_world_y / tile_world_size).floor() as i32;
    let mut tiles = Vec::new();

    for y in y_start.max(0)..=y_end.min(max_index) {
        for x in x_start.max(0)..=x_end.min(max_index) {
            tiles.push(VectorTileRequest {
                layer: "fix".to_string(),
                z: FIX_TILE_ZOOM,
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
    fix_tile_cache: &HashMap<String, PointTilePayload>,
) -> MapOverlayQueryResult {
    let tile_window = visible_fix_tile_window(viewport, width_px, height_px);
    let mut needed_fix_tiles = Vec::new();
    let mut visible_features = Vec::new();
    let mut limit_hit = false;
    let center_world = lat_lon_to_world(viewport.center);
    let scale = 2.0_f64.powf(viewport.zoom);

    for tile in tile_window {
        let key = tile_key(tile.z, tile.x, tile.y);
        let Some(payload) = fix_tile_cache.get(&key) else {
            needed_fix_tiles.push(tile);
            continue;
        };
        for record in &payload.records {
            if visible_features.len() >= VECTOR_DISPLAY_FEATURE_LIMIT {
                limit_hit = true;
                break;
            }
            let point = world_to_screen(center_world, scale, width_px, height_px, LatLon { lat: record.lat, lon: record.lon });
            visible_features.push(VisibleMapFeature {
                id: record.id.clone(),
                kind: record.kind.clone(),
                label: record.label.trim().to_uppercase(),
                style_class: record.style_class.clone(),
                screen_x: point.x,
                screen_y: point.y,
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

    MapOverlayQueryResult {
        needed_fix_tiles,
        visible_features,
        warnings,
    }
}

pub fn tile_key(z: u32, x: u32, y: u32) -> String {
    format!("{z}/{x}/{y}")
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
    fn suppresses_fix_tiles_below_threshold_zoom() {
        let viewport = MapViewport {
            center: LatLon { lat: 47.36, lon: -121.98 },
            zoom: 8.9,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        assert!(visible_fix_tile_window(&viewport, 1200.0, 900.0).is_empty());
    }

    #[test]
    fn caps_visible_features_and_warns() {
        let viewport = MapViewport {
            center: LatLon { lat: 47.36, lon: -121.98 },
            zoom: 10.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let window = visible_fix_tile_window(&viewport, 1200.0, 900.0);
        let first = window.first().expect("expected visible tile");
        let mut cache = HashMap::new();
        cache.insert(
            tile_key(first.z, first.x, first.y),
            PointTilePayload {
                schema_version: 1,
                layer: "fix".to_string(),
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
                    })
                    .collect(),
            },
        );
        let result = query_map_overlay(&viewport, 1200.0, 900.0, &cache);
        assert_eq!(result.visible_features.len(), VECTOR_DISPLAY_FEATURE_LIMIT);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].code, "vector_display_feature_limit");
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

        for tile in visible_fix_tile_window(&viewport, 1200.0, 900.0) {
            let tile_path = tile_root.join(tile.x.to_string()).join(format!("{}.json", tile.y));
            let payload: PointTilePayload = serde_json::from_str(
                &fs::read_to_string(&tile_path)
                    .unwrap_or_else(|err| panic!("failed to read {}: {err}", tile_path.display())),
            )
            .unwrap_or_else(|err| panic!("failed to parse {}: {err}", tile_path.display()));
            cache.insert(tile_key(tile.z, tile.x, tile.y), payload);
        }

        let result = query_map_overlay(&viewport, 1200.0, 900.0, &cache);
        assert!(
            !result.visible_features.is_empty(),
            "expected visible fix features for VAMPS viewport"
        );
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
            let ui_dir = manifest_dir.join("../../..").canonicalize().expect("resolve ui dir");
            let repo_root = ui_dir.parent().expect("ui dir parent");
            let target_root_raw =
                fs::read_to_string(ui_dir.join("target-root.txt")).expect("read ui/target-root.txt");
            let target_root = repo_root.join(target_root_raw.trim()).canonicalize().expect("resolve ui target root");
            let path = target_root.join("web/generated-static/vectors/points/fix/9");
            if path.is_dir() {
                return path;
            }
            panic!("unable to locate vector tile fixture root");
        })
        .as_path()
    }
}
