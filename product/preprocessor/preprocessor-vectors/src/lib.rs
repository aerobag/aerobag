use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use rusqlite::Connection;
use serde::Serialize;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

const POINT_LAYER_ZOOM_POLICY: &[(&str, u8)] = &[
    ("airport", 9),
    ("fix", 9),
    ("nav", 9),
    ("awos", 9),
    ("obstacle", 12),
];

#[derive(Debug, Clone)]
pub struct BuildVectorsRequest {
    pub main_db: PathBuf,
    pub output_dir: PathBuf,
    pub version_label: String,
}

#[derive(Debug, Clone)]
pub struct BuildVectorsResult {
    pub manifest_path: PathBuf,
    pub stats_path: PathBuf,
    pub zip_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct VectorManifest {
    schema_version: u32,
    version_label: String,
    point_layers: BTreeMap<String, PointLayerManifest>,
    files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
struct VectorStats {
    schema_version: u32,
    version_label: String,
    points: PointStats,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PointStats {
    total_points: usize,
    layer_counts: BTreeMap<String, usize>,
    layers: BTreeMap<String, PointLayerStats>,
}

#[derive(Debug, Clone, Serialize)]
struct PointLayerManifest {
    zoom: u8,
    tile_path_template: String,
}

#[derive(Debug, Clone, Serialize)]
struct PointLayerStats {
    zoom: u8,
    tile_count: usize,
    max_points_in_tile: usize,
}

#[derive(Debug, Clone, Serialize)]
struct PointTileFile {
    schema_version: u32,
    layer: String,
    z: u8,
    x: u32,
    y: u32,
    records: Vec<PointRecord>,
}

#[derive(Debug, Clone, Serialize)]
struct PointTileRecord {
    z: u8,
    x: u32,
    y: u32,
    records: Vec<PointRecord>,
}

#[derive(Debug, Clone, Serialize)]
struct PointRecord {
    id: String,
    kind: String,
    lat: f64,
    lon: f64,
    label: String,
    style_class: String,
}

pub fn build_vectors_dataset(request: &BuildVectorsRequest) -> anyhow::Result<BuildVectorsResult> {
    if request.output_dir.exists() {
        fs::remove_dir_all(&request.output_dir)
            .with_context(|| format!("failed to clear {}", request.output_dir.display()))?;
    }
    fs::create_dir_all(&request.output_dir)
        .with_context(|| format!("failed to create {}", request.output_dir.display()))?;

    let conn = Connection::open(&request.main_db)
        .with_context(|| format!("failed to open {}", request.main_db.display()))?;
    let points = load_points(&conn)?;
    let stats_path = request.output_dir.join("stats.json");
    let manifest_path = request
        .output_dir
        .join(format!("vectors_{}", request.version_label));
    let zip_path = request
        .output_dir
        .join(format!("vectors_{}.zip", request.version_label));

    let mut files = BTreeMap::new();
    let mut point_layers = BTreeMap::new();
    let mut layer_stats = BTreeMap::new();
    let mut zip_members = vec![
        ("vectors".to_string(), manifest_path.clone()),
        ("stats.json".to_string(), stats_path.clone()),
    ];

    for (layer_name, layer_points) in points_by_layer(&points) {
        let zoom = layer_tile_zoom(&layer_name);
        let point_tiles = build_point_tiles(&layer_points, zoom);
        let tile_path_template = format!("points/{layer_name}/{zoom}/{{x}}/{{y}}.json");
        for tile in &point_tiles {
            let relative_path = point_tile_relative_path(&layer_name, tile.z, tile.x, tile.y);
            let points_path = request.output_dir.join(&relative_path);
            write_json_pretty(
                &points_path,
                &PointTileFile {
                    schema_version: 1,
                    layer: layer_name.clone(),
                    z: tile.z,
                    x: tile.x,
                    y: tile.y,
                    records: tile.records.clone(),
                },
            )?;
            zip_members.push((relative_path, points_path));
        }
        files.insert(format!("point_tiles_{layer_name}"), tile_path_template.clone());
        point_layers.insert(
            layer_name.clone(),
            PointLayerManifest {
                zoom,
                tile_path_template,
            },
        );
        layer_stats.insert(
            layer_name.clone(),
            PointLayerStats {
                zoom,
                tile_count: point_tiles.len(),
                max_points_in_tile: point_tiles
                    .iter()
                    .map(|tile| tile.records.len())
                    .max()
                    .unwrap_or(0),
            },
        );
    }

    let stats = VectorStats {
        schema_version: 1,
        version_label: request.version_label.clone(),
        points: PointStats {
            total_points: points.len(),
            layer_counts: point_layer_counts(&points),
            layers: layer_stats,
        },
        warnings: vec![
            "first-cut vectors dataset includes only point layers".to_string(),
            "non-point FAA boundary features are not yet ingested".to_string(),
        ],
    };
    write_json_pretty(&stats_path, &stats)?;

    files.insert("stats".to_string(), "stats.json".to_string());
    write_json_pretty(
        &manifest_path,
        &VectorManifest {
            schema_version: 1,
            version_label: request.version_label.clone(),
            point_layers,
            files,
        },
    )?;

    write_zip(&zip_path, &zip_members)?;

    Ok(BuildVectorsResult {
        manifest_path,
        stats_path,
        zip_path,
    })
}

fn load_points(conn: &Connection) -> anyhow::Result<Vec<PointRecord>> {
    let mut points = Vec::new();
    let mut seen = BTreeSet::new();

    let point_sources = [
        (
            "airports",
            "SELECT LocationID, ARPLatitude, ARPLongitude, FacilityName, Type FROM airports WHERE ARPLatitude != '' AND ARPLongitude != ''",
            "airport",
        ),
        (
            "nav",
            "SELECT LocationID, ARPLatitude, ARPLongitude, FacilityName, Type FROM nav",
            "nav",
        ),
        (
            "fix",
            "SELECT LocationID, ARPLatitude, ARPLongitude, FacilityName, Type
             FROM fix
             WHERE printf('%.6f,%.6f', ARPLatitude, ARPLongitude) IN (
                 SELECT DISTINCT printf('%.6f,%.6f', Latitude, Longitude)
                 FROM airways
             )",
            "fix",
        ),
        (
            "awos",
            "SELECT LocationID, Latitude, Longitude, Type, Type FROM awos WHERE Latitude != '' AND Longitude != ''",
            "awos",
        ),
        (
            "obs",
            "SELECT printf('%.6f,%.6f', ARPLatitude, ARPLongitude), ARPLatitude, ARPLongitude, printf('Obstacle %.0fft', Height), 'OBS' FROM obs",
            "obstacle",
        ),
    ];

    for (table_name, sql, style_class) in point_sources {
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let lat: f64 = parse_f64_cell(row, 1)?;
            let lon: f64 = parse_f64_cell(row, 2)?;
            let label: String = row.get::<_, String>(3)?;
            let kind: String = row.get::<_, String>(4)?;
            Ok((id, lat, lon, label, kind))
        })?;
        for row in rows {
            let (raw_id, lat, lon, label, kind) = row?;
            if !valid_lat_lon(lat, lon) {
                continue;
            }
            let id = dedup_id(&mut seen, &format!("{table_name}:{raw_id}"), lat, lon);
            points.push(PointRecord {
                id,
                kind: kind.to_lowercase(),
                lat,
                lon,
                label,
                style_class: style_class.to_string(),
            });
        }
    }

    Ok(points)
}

fn build_point_tiles(points: &[PointRecord], zoom: u8) -> Vec<PointTileRecord> {
    let mut tiles = BTreeMap::<(u8, u32, u32), Vec<PointRecord>>::new();
    for point in points {
        let (x, y) = slippy_tile(point.lat, point.lon, zoom);
        tiles.entry((zoom, x, y)).or_default().push(point.clone());
    }
    tiles
        .into_iter()
        .map(|((z, x, y), records)| PointTileRecord { z, x, y, records })
        .collect()
}

fn point_tile_relative_path(layer_name: &str, z: u8, x: u32, y: u32) -> String {
    format!("points/{layer_name}/{z}/{x}/{y}.json")
}

fn points_by_layer(points: &[PointRecord]) -> BTreeMap<String, Vec<PointRecord>> {
    let mut layers = BTreeMap::<String, Vec<PointRecord>>::new();
    for point in points {
        layers
            .entry(point.style_class.clone())
            .or_default()
            .push(point.clone());
    }
    layers
}

fn layer_tile_zoom(layer_name: &str) -> u8 {
    POINT_LAYER_ZOOM_POLICY
        .iter()
        .find_map(|(name, zoom)| (*name == layer_name).then_some(*zoom))
        .unwrap_or(9)
}

fn point_layer_counts(points: &[PointRecord]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for point in points {
        *counts.entry(point.style_class.clone()).or_insert(0) += 1;
    }
    counts
}

fn slippy_tile(lat: f64, lon: f64, zoom: u8) -> (u32, u32) {
    let lat_rad = lat.to_radians();
    let n = 2_f64.powi(zoom as i32);
    let x = ((lon + 180.0) / 360.0 * n).floor();
    let y =
        ((1.0 - ((lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI)) / 2.0 * n)
            .floor();
    (clamp_tile(x, zoom), clamp_tile(y, zoom))
}

fn clamp_tile(value: f64, zoom: u8) -> u32 {
    let max = (1_u32 << zoom) - 1;
    value.max(0.0).min(max as f64) as u32
}

fn valid_lat_lon(lat: f64, lon: f64) -> bool {
    lat.is_finite()
        && lon.is_finite()
        && (-90.0..=90.0).contains(&lat)
        && (-180.0..=180.0).contains(&lon)
}

fn dedup_id(seen: &mut BTreeSet<String>, base: &str, lat: f64, lon: f64) -> String {
    let normalized = base.replace(' ', "_");
    if seen.insert(normalized.clone()) {
        return normalized;
    }
    let fallback = format!("{normalized}:{lat:.6}:{lon:.6}");
    let _ = seen.insert(fallback.clone());
    fallback
}

fn parse_f64_cell(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<f64> {
    use rusqlite::types::ValueRef;
    match row.get_ref(index)? {
        ValueRef::Real(value) => Ok(value),
        ValueRef::Integer(value) => Ok(value as f64),
        ValueRef::Text(bytes) => Ok(String::from_utf8_lossy(bytes)
            .trim()
            .parse::<f64>()
            .unwrap_or(0.0)),
        ValueRef::Null => Ok(0.0),
        ValueRef::Blob(_) => Ok(0.0),
    }
}

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(
        path,
        serde_json::to_vec_pretty(value).context("failed to encode json")?,
    )
    .with_context(|| format!("failed to write {}", path.display()))
}

fn write_zip(path: &Path, members: &[(String, PathBuf)]) -> anyhow::Result<()> {
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    let file =
        fs::File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (member_name, source_path) in members {
        zip.start_file(member_name, options)?;
        let mut source = fs::File::open(source_path)
            .with_context(|| format!("failed to open {}", source_path.display()))?;
        let mut bytes = Vec::new();
        source.read_to_end(&mut bytes)?;
        zip.write_all(&bytes)?;
    }
    zip.finish()?;
    Ok(())
}
