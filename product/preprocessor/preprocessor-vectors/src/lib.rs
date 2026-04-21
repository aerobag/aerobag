use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use quick_xml::events::Event;
use quick_xml::Reader;
use rusqlite::Connection;
use serde::Serialize;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

const POINT_LAYER_ZOOM_POLICY: &[(&str, u8)] =
    &[("airport", 9), ("fix", 9), ("nav", 9), ("awos", 9)];
const OBSTACLE_LAYER_ZOOM: u8 = 12;
const AIRSPACE_REF_MIN_ZOOM: u8 = 0;
const AIRSPACE_REF_MAX_ZOOM: u8 = 8;
const AIRSPACE_REF_MIN_PIXEL_SPAN: f64 = 3.0;
const AIRSPACE_LABEL_MIN_ZOOM: u8 = 0;
const AIRSPACE_LABEL_MAX_ZOOM: u8 = 12;
const AIRSPACE_LABEL_MIN_PIXEL_SPAN: f64 = 50.0;

#[derive(Debug, Clone)]
pub struct BuildVectorsRequest {
    pub main_db: PathBuf,
    pub data_input_dir: Option<PathBuf>,
    pub output_dir: PathBuf,
    pub version_label: String,
    pub include_class_e_airspace: bool,
}

#[derive(Debug, Clone)]
pub struct BuildVectorsResult {
    pub manifest_path: PathBuf,
    pub stats_path: PathBuf,
    pub zip_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct BuildObstacleDatasetRequest {
    pub input_dir: PathBuf,
    pub output_dir: PathBuf,
    pub version_label: String,
}

#[derive(Debug, Clone)]
pub struct BuildObstacleDatasetResult {
    pub manifest_path: PathBuf,
    pub stats_path: PathBuf,
    pub zip_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct VectorManifest {
    schema_version: u32,
    version_label: String,
    point_layers: BTreeMap<String, PointLayerManifest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    airspace: Option<AirspaceManifest>,
    files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
struct VectorStats {
    schema_version: u32,
    version_label: String,
    points: PointStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    airspace: Option<AirspaceStats>,
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
struct AirspaceManifest {
    source: String,
    source_path: String,
    feature_path_template: String,
    reference_tile_path_template: String,
    label_tile_path_template: String,
    reference_tile_min_zoom: u8,
    reference_tile_max_zoom: u8,
    label_tile_min_zoom: u8,
    label_tile_max_zoom: u8,
    geometry_encoding: String,
}

#[derive(Debug, Clone, Serialize)]
struct AirspaceStats {
    source_found: bool,
    feature_count: usize,
    class_counts: BTreeMap<String, usize>,
    reference_tile_count: usize,
    label_tile_count: usize,
    max_refs_in_tile: usize,
    max_labels_in_tile: usize,
    class_label_candidate_outside_polygon_count: usize,
    class_label_anchor_outside_polygon_count: usize,
    class_airport_label_adjustment_count: usize,
}

#[derive(Debug, Clone, Default)]
struct AirspaceLabelDiagnostics {
    class_label_candidate_outside_polygon_count: usize,
    class_label_anchor_outside_polygon_count: usize,
    class_airport_label_adjustment_count: usize,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    towered: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fuel_available: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    public_use: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    private_use: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    has_paved_runway: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    heliport: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    has_water_runway: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    longest_runway_length_ft: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    longest_runway_heading_true_deg: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct AirspaceFeature {
    schema_version: u32,
    id: String,
    kind: String,
    source: String,
    cycle: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    ident: Option<String>,
    airspace_class: String,
    local_type: String,
    style_hint: String,
    vertical_label: String,
    vertical: AirspaceVertical,
    bbox: [f64; 4],
    label: AirspaceLabel,
    paths: Vec<AirspacePath>,
    source_properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
struct AirspaceVertical {
    lower: AirspaceLimit,
    upper: AirspaceLimit,
}

#[derive(Debug, Clone, Serialize)]
struct AirspaceLimit {
    display: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    feet: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AirspaceLabel {
    text: String,
    lon: f64,
    lat: f64,
}

#[derive(Debug, Clone, Serialize)]
struct AirspacePath {
    role: String,
    closed: bool,
    points: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize)]
struct AirspaceReferenceTileFile {
    schema_version: u32,
    layer: String,
    z: u8,
    x: u32,
    y: u32,
    refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AirspaceLabelTileFile {
    schema_version: u32,
    layer: String,
    z: u8,
    x: u32,
    y: u32,
    labels: Vec<AirspaceTileLabel>,
}

#[derive(Debug, Clone, Serialize)]
struct AirspaceTileLabel {
    feature_id: String,
    text: String,
    lon: f64,
    lat: f64,
    style_hint: String,
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
    let airport_points = points
        .iter()
        .filter(|point| point.style_class == "airport")
        .cloned()
        .collect::<Vec<_>>();
    let airspace_source = request
        .data_input_dir
        .as_ref()
        .map(|dir| dir.join("Additional_Data/Shape_Files/Class_Airspace.shp"));
    let saa_source = request
        .data_input_dir
        .as_ref()
        .map(|dir| dir.join("SAA-AIXM_5_Schema/SaaSubscriberFile.zip"));
    let mut airspace_label_diagnostics = AirspaceLabelDiagnostics::default();
    let mut airspace_features = match airspace_source.as_deref() {
        Some(path) if path.exists() => {
            let (features, diagnostics) = load_class_airspace_features(
                path,
                &request.version_label,
                request.include_class_e_airspace,
                &airport_points,
            )
            .with_context(|| format!("failed to load airspace shapefile {}", path.display()))?;
            airspace_label_diagnostics = diagnostics;
            features
        }
        _ => Vec::new(),
    };
    if let Some(path) = saa_source.as_ref().filter(|path| path.exists()) {
        airspace_features.extend(
            load_saa_airspace_features(path, &request.version_label)
                .with_context(|| format!("failed to load SAA AIXM {}", path.display()))?,
        );
    }
    let stats_path = request.output_dir.join("stats.json");
    let manifest_path = request
        .output_dir
        .join(format!("vectors_{}.manifest", request.version_label));
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
        files.insert(
            format!("point_tiles_{layer_name}"),
            tile_path_template.clone(),
        );
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

    let mut airspace_manifest = None;
    let mut airspace_stats = None;
    if !airspace_features.is_empty() {
        let feature_path_template = "had/{id}.json".to_string();
        let reference_tile_path_template = "airspace/refs/{z}/{x}/{y}.json".to_string();
        let label_tile_path_template = "airspace/labels/{z}/{x}/{y}.json".to_string();
        let mut reference_tiles = BTreeMap::<(u8, u32, u32), Vec<String>>::new();
        let mut label_tiles = BTreeMap::<(u8, u32, u32), Vec<AirspaceTileLabel>>::new();
        let mut class_counts = BTreeMap::<String, usize>::new();

        for feature in &airspace_features {
            *class_counts
                .entry(feature.airspace_class.clone())
                .or_insert(0) += 1;
            let feature_path = request
                .output_dir
                .join(airspace_feature_relative_path(&feature.id));
            write_json_compact(&feature_path, feature)?;
            zip_members.push((airspace_feature_relative_path(&feature.id), feature_path));

            for zoom in AIRSPACE_REF_MIN_ZOOM..=AIRSPACE_REF_MAX_ZOOM {
                if !bbox_is_visible_at_zoom(feature.bbox, zoom, AIRSPACE_REF_MIN_PIXEL_SPAN) {
                    continue;
                }
                for tile in tiles_for_bbox(feature.bbox, zoom) {
                    reference_tiles
                        .entry(tile)
                        .or_default()
                        .push(feature.id.clone());
                }
            }
            for zoom in AIRSPACE_LABEL_MIN_ZOOM..=AIRSPACE_LABEL_MAX_ZOOM {
                if !bbox_is_visible_at_zoom(feature.bbox, zoom, AIRSPACE_LABEL_MIN_PIXEL_SPAN) {
                    continue;
                }
                let (label_x, label_y) = slippy_tile(feature.label.lat, feature.label.lon, zoom);
                label_tiles
                    .entry((zoom, label_x, label_y))
                    .or_default()
                    .push(AirspaceTileLabel {
                        feature_id: feature.id.clone(),
                        text: feature.label.text.clone(),
                        lon: feature.label.lon,
                        lat: feature.label.lat,
                        style_hint: feature.style_hint.clone(),
                    });
            }
        }

        let mut max_refs_in_tile = 0usize;
        for ((z, x, y), mut refs) in reference_tiles {
            refs.sort();
            refs.dedup();
            max_refs_in_tile = max_refs_in_tile.max(refs.len());
            let relative_path = airspace_ref_tile_relative_path(z, x, y);
            let path = request.output_dir.join(&relative_path);
            write_json_compact(
                &path,
                &AirspaceReferenceTileFile {
                    schema_version: 1,
                    layer: "airspace".to_string(),
                    z,
                    x,
                    y,
                    refs,
                },
            )?;
            zip_members.push((relative_path, path));
        }

        let mut max_labels_in_tile = 0usize;
        let label_tile_count = label_tiles.len();
        for ((z, x, y), labels) in label_tiles {
            max_labels_in_tile = max_labels_in_tile.max(labels.len());
            let relative_path = airspace_label_tile_relative_path(z, x, y);
            let path = request.output_dir.join(&relative_path);
            write_json_compact(
                &path,
                &AirspaceLabelTileFile {
                    schema_version: 1,
                    layer: "airspace-labels".to_string(),
                    z,
                    x,
                    y,
                    labels,
                },
            )?;
            zip_members.push((relative_path, path));
        }

        files.insert(
            "airspace_features".to_string(),
            feature_path_template.clone(),
        );
        files.insert(
            "airspace_reference_tiles".to_string(),
            reference_tile_path_template.clone(),
        );
        files.insert(
            "airspace_label_tiles".to_string(),
            label_tile_path_template.clone(),
        );
        airspace_manifest = Some(AirspaceManifest {
            source: "FAA NASR Class B,C,D,E Airspace Shape Files and SAA AIXM".to_string(),
            source_path:
                "Additional_Data/Shape_Files/Class_Airspace.shp; SAA-AIXM_5_Schema/SaaSubscriberFile.zip"
                    .to_string(),
            feature_path_template,
            reference_tile_path_template,
            label_tile_path_template,
            reference_tile_min_zoom: AIRSPACE_REF_MIN_ZOOM,
            reference_tile_max_zoom: AIRSPACE_REF_MAX_ZOOM,
            label_tile_min_zoom: AIRSPACE_LABEL_MIN_ZOOM,
            label_tile_max_zoom: AIRSPACE_LABEL_MAX_ZOOM,
            geometry_encoding:
                "lon/lat point arrays; current shapefile arcs are FAA-densified line segments"
                    .to_string(),
        });
        airspace_stats = Some(AirspaceStats {
            source_found: true,
            feature_count: airspace_features.len(),
            class_counts,
            reference_tile_count: files
                .get("airspace_reference_tiles")
                .map(|_| {
                    zip_members
                        .iter()
                        .filter(|(name, _)| name.starts_with("airspace/refs/"))
                        .count()
                })
                .unwrap_or(0),
            label_tile_count,
            max_refs_in_tile,
            max_labels_in_tile,
            class_label_candidate_outside_polygon_count: airspace_label_diagnostics
                .class_label_candidate_outside_polygon_count,
            class_label_anchor_outside_polygon_count: airspace_label_diagnostics
                .class_label_anchor_outside_polygon_count,
            class_airport_label_adjustment_count: airspace_label_diagnostics
                .class_airport_label_adjustment_count,
        });
    }

    let stats = VectorStats {
        schema_version: 1,
        version_label: request.version_label.clone(),
        points: PointStats {
            total_points: points.len(),
            layer_counts: point_layer_counts(&points),
            layers: layer_stats,
        },
        airspace: airspace_stats,
        warnings: Vec::new(),
    };
    write_json_pretty(&stats_path, &stats)?;

    files.insert("stats".to_string(), "stats.json".to_string());
    write_json_pretty(
        &manifest_path,
        &VectorManifest {
            schema_version: 1,
            version_label: request.version_label.clone(),
            point_layers,
            airspace: airspace_manifest,
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

pub fn build_obstacle_dataset(
    request: &BuildObstacleDatasetRequest,
) -> anyhow::Result<BuildObstacleDatasetResult> {
    if request.output_dir.exists() {
        fs::remove_dir_all(&request.output_dir)
            .with_context(|| format!("failed to clear {}", request.output_dir.display()))?;
    }
    fs::create_dir_all(&request.output_dir)
        .with_context(|| format!("failed to create {}", request.output_dir.display()))?;

    let points = load_obstacle_points(&request.input_dir)?;
    let stats_path = request.output_dir.join("stats.json");
    let manifest_path = request
        .output_dir
        .join(format!("obstacles_{}.manifest", request.version_label));
    let zip_path = request
        .output_dir
        .join(format!("obstacles_{}.zip", request.version_label));

    let point_tiles = build_point_tiles(&points, OBSTACLE_LAYER_ZOOM);
    let tile_path_template = format!("points/obstacle/{}/{{x}}/{{y}}.json", OBSTACLE_LAYER_ZOOM);

    let mut files = BTreeMap::new();
    let mut point_layers = BTreeMap::new();
    let mut zip_members = vec![
        ("obstacles".to_string(), manifest_path.clone()),
        ("stats.json".to_string(), stats_path.clone()),
    ];

    for tile in &point_tiles {
        let relative_path = point_tile_relative_path("obstacle", tile.z, tile.x, tile.y);
        let points_path = request.output_dir.join(&relative_path);
        write_json_pretty(
            &points_path,
            &PointTileFile {
                schema_version: 1,
                layer: "obstacle".to_string(),
                z: tile.z,
                x: tile.x,
                y: tile.y,
                records: tile.records.clone(),
            },
        )?;
        zip_members.push((relative_path, points_path));
    }

    files.insert(
        "point_tiles_obstacle".to_string(),
        tile_path_template.clone(),
    );
    files.insert("stats".to_string(), "stats.json".to_string());
    point_layers.insert(
        "obstacle".to_string(),
        PointLayerManifest {
            zoom: OBSTACLE_LAYER_ZOOM,
            tile_path_template,
        },
    );

    write_json_pretty(
        &stats_path,
        &VectorStats {
            schema_version: 1,
            version_label: request.version_label.clone(),
            points: PointStats {
                total_points: points.len(),
                layer_counts: BTreeMap::from([("obstacle".to_string(), points.len())]),
                layers: BTreeMap::from([(
                    "obstacle".to_string(),
                    PointLayerStats {
                        zoom: OBSTACLE_LAYER_ZOOM,
                        tile_count: point_tiles.len(),
                        max_points_in_tile: point_tiles
                            .iter()
                            .map(|tile| tile.records.len())
                            .max()
                            .unwrap_or(0),
                    },
                )]),
            },
            airspace: None,
            warnings: vec![
                "obstacle dataset is published separately from the cycle bundle".to_string(),
            ],
        },
    )?;

    write_json_pretty(
        &manifest_path,
        &VectorManifest {
            schema_version: 1,
            version_label: request.version_label.clone(),
            point_layers,
            airspace: None,
            files,
        },
    )?;

    write_zip(&zip_path, &zip_members)?;

    Ok(BuildObstacleDatasetResult {
        manifest_path,
        stats_path,
        zip_path,
    })
}

fn load_points(conn: &Connection) -> anyhow::Result<Vec<PointRecord>> {
    let mut points = Vec::new();
    let mut seen = BTreeSet::new();
    let runway_info = load_airport_runway_info(conn)?;

    let point_sources = [
        (
            "airports",
            "SELECT LocationID, ARPLatitude, ARPLongitude, FacilityName, Type, ATCT, FuelTypes, Use FROM airports WHERE ARPLatitude != '' AND ARPLongitude != ''",
            "airport",
        ),
        (
            "nav",
            "SELECT LocationID, ARPLatitude, ARPLongitude, FacilityName, Type
             FROM nav
             WHERE UPPER(TRIM(Type)) IN ('VOR', 'VOR/DME', 'VORTAC')",
            "nav",
        ),
        (
            "fix",
            "SELECT LocationID, ARPLatitude, ARPLongitude, FacilityName, Type
             FROM fix
             WHERE printf('%.6f,%.6f', ARPLatitude, ARPLongitude) IN (
                 SELECT DISTINCT printf('%.6f,%.6f', Latitude, Longitude)
                 FROM airways_branch
             )",
            "fix",
        ),
        (
            "awos",
            "SELECT LocationID, Latitude, Longitude, Type, Type FROM awos WHERE Latitude != '' AND Longitude != ''",
            "awos",
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
            let (towered, fuel_available, public_use, private_use, heliport) =
                if table_name == "airports" {
                    let atct: String = row.get::<_, String>(5)?;
                    let fuel_types: String = row.get::<_, String>(6)?;
                    let use_code: String = row.get::<_, String>(7)?;
                    let type_upper = kind.trim().to_ascii_uppercase();
                    (
                        Some(atct.trim().eq_ignore_ascii_case("Y")),
                        Some(!fuel_types.trim().is_empty()),
                        Some(use_code.trim().eq_ignore_ascii_case("PU")),
                        Some(use_code.trim().eq_ignore_ascii_case("PR")),
                        Some(type_upper.contains("HELIPORT")),
                    )
                } else {
                    (None, None, None, None, None)
                };
            Ok((
                id,
                lat,
                lon,
                label,
                kind,
                towered,
                fuel_available,
                public_use,
                private_use,
                heliport,
            ))
        })?;
        for row in rows {
            let (
                raw_id,
                lat,
                lon,
                label,
                kind,
                towered,
                fuel_available,
                public_use,
                private_use,
                heliport,
            ) = row?;
            if !valid_lat_lon(lat, lon) {
                continue;
            }
            let airport_runway_info = (table_name == "airports")
                .then(|| runway_info.get(&raw_id))
                .flatten();
            let id = dedup_id(&mut seen, &format!("{table_name}:{raw_id}"), lat, lon);
            points.push(PointRecord {
                id,
                kind: kind.to_lowercase(),
                lat,
                lon,
                label,
                style_class: style_class.to_string(),
                towered,
                fuel_available,
                public_use,
                private_use,
                has_paved_runway: airport_runway_info.map(|runway| runway.has_paved_runway),
                heliport,
                has_water_runway: (table_name == "airports").then_some(
                    airport_runway_info
                        .map(|runway| runway.has_water_runway)
                        .unwrap_or(false)
                        || kind.trim().eq_ignore_ascii_case("SEAPLANE BAS"),
                ),
                longest_runway_length_ft: (table_name == "airports")
                    .then(|| airport_runway_info.map(|runway| runway.length_ft))
                    .flatten(),
                longest_runway_heading_true_deg: (table_name == "airports")
                    .then(|| airport_runway_info.map(|runway| runway.heading_true_deg))
                    .flatten(),
            });
        }
    }

    Ok(points)
}

fn load_obstacle_points(input_dir: &Path) -> anyhow::Result<Vec<PointRecord>> {
    let dof_path = input_dir.join("DOF.DAT");
    let text = String::from_utf8_lossy(
        &fs::read(&dof_path).with_context(|| format!("failed to read {}", dof_path.display()))?,
    )
    .into_owned();
    let mut points = Vec::new();
    let mut seen = BTreeSet::new();
    for raw in text.lines() {
        if raw.len() < 95 {
            continue;
        }
        if !raw.as_bytes()[0].is_ascii_alphanumeric() || raw.as_bytes().get(2) != Some(&b'-') {
            continue;
        }
        let lat_deg = parse_float(field(raw, 35, 2));
        let lat_min = parse_float(field(raw, 38, 2)) / 60.0;
        let lat_sec = parse_float(field(raw, 41, 5)) / 3600.0;
        let lat_hemi = field(raw, 46, 1).trim();
        let lat = if lat_hemi == "N" {
            lat_deg + lat_min + lat_sec
        } else {
            -(lat_deg + lat_min + lat_sec)
        };
        let lon_deg = parse_float(field(raw, 48, 3));
        let lon_min = parse_float(field(raw, 52, 2)) / 60.0;
        let lon_sec = parse_float(field(raw, 55, 5)) / 3600.0;
        let lon_hemi = field(raw, 60, 1).trim();
        let lon = if lon_hemi == "W" {
            -(lon_deg + lon_min + lon_sec)
        } else {
            lon_deg + lon_min + lon_sec
        };
        let height_msl = parse_float(field(raw, 90, 5));
        let height_agl = parse_float(field(raw, 84, 5));
        if height_agl < 400.0 || !valid_lat_lon(lat, lon) {
            continue;
        }
        let id = dedup_id(
            &mut seen,
            &format!("obs:{lat:.6}:{lon:.6}:{height_msl:.0}"),
            lat,
            lon,
        );
        points.push(PointRecord {
            id,
            kind: "obs".to_string(),
            lat,
            lon,
            label: format!("Obstacle {:.0}ft", height_msl),
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
        });
    }
    Ok(points)
}

fn load_class_airspace_features(
    shp_path: &Path,
    version_label: &str,
    include_class_e: bool,
    airport_points: &[PointRecord],
) -> anyhow::Result<(Vec<AirspaceFeature>, AirspaceLabelDiagnostics)> {
    let dbf_path = shp_path.with_extension("dbf");
    let dbf_records = read_dbf_records(&dbf_path)?;
    let shapes = read_shapefile_polygons(shp_path)?;
    let mut features = Vec::new();
    let mut seen = BTreeSet::new();
    let mut diagnostics = AirspaceLabelDiagnostics::default();

    for (index, shape) in shapes.into_iter().enumerate() {
        if shape.parts.is_empty() || !bbox_is_valid(shape.bbox) {
            continue;
        }
        let properties = dbf_records.get(index).cloned().unwrap_or_default();
        let class = property(&properties, "CLASS").unwrap_or("unknown");
        if !include_class_e && class.eq_ignore_ascii_case("E") {
            continue;
        }
        let local_type = property(&properties, "LOCAL_TYPE").unwrap_or("");
        let name = property(&properties, "NAME")
            .filter(|value| !value.is_empty())
            .unwrap_or("Unnamed airspace")
            .to_string();
        let ident = property(&properties, "IDENT")
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let vertical = AirspaceVertical {
            lower: airspace_limit(&properties, "LOWER"),
            upper: airspace_limit(&properties, "UPPER"),
        };
        let vertical_label = vertical_label(&vertical);
        if polygon_area_centroid(&shape.parts)
            .is_some_and(|candidate| !point_in_polygon_parts(candidate, &shape.parts))
        {
            diagnostics.class_label_candidate_outside_polygon_count += 1;
        }
        let mut label_anchor = polygon_label_anchor(&shape.parts);
        if let Some(adjusted_anchor) = airport_adjusted_label_anchor(
            label_anchor,
            &shape.parts,
            class,
            ident.as_deref(),
            airport_points,
        ) {
            label_anchor = adjusted_anchor;
            diagnostics.class_airport_label_adjustment_count += 1;
        }
        if !point_in_polygon_parts(label_anchor, &shape.parts) {
            diagnostics.class_label_anchor_outside_polygon_count += 1;
        }
        let id_base = format!(
            "airspace:{}:{}:{}:{}:{}",
            version_label,
            class.to_ascii_lowercase(),
            ident.as_deref().unwrap_or("anon"),
            local_type.to_ascii_lowercase(),
            index + 1
        );
        let id = dedup_airspace_id(&mut seen, &id_base);
        let paths = shape
            .parts
            .iter()
            .filter_map(|part| airspace_path_from_points(part, "boundary"))
            .collect::<Vec<_>>();

        features.push(AirspaceFeature {
            schema_version: 1,
            id,
            kind: "airspace".to_string(),
            source: "faa_nasr_class_airspace_shapefile".to_string(),
            cycle: version_label.trim_start_matches("data_").to_string(),
            name,
            ident,
            airspace_class: class.to_string(),
            local_type: local_type.to_string(),
            style_hint: airspace_style_hint(class, local_type),
            vertical_label: vertical_label.clone(),
            vertical,
            bbox: shape.bbox,
            label: AirspaceLabel {
                text: vertical_label,
                lon: label_anchor[0],
                lat: label_anchor[1],
            },
            paths,
            source_properties: properties,
        });
    }

    Ok((features, diagnostics))
}

fn load_saa_airspace_features(
    path: &Path,
    version_label: &str,
) -> anyhow::Result<Vec<AirspaceFeature>> {
    let outer_file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut outer = ZipArchive::new(outer_file)
        .with_context(|| format!("failed to read zip {}", path.display()))?;
    let mut nested_bytes = Vec::new();
    outer
        .by_name("Saa_Sub_File.zip")
        .context("SAA package missing Saa_Sub_File.zip")?
        .read_to_end(&mut nested_bytes)?;
    let mut inner =
        ZipArchive::new(Cursor::new(nested_bytes)).context("failed to read nested SAA zip")?;
    let mut features = Vec::new();
    let mut seen = BTreeSet::new();
    for index in 0..inner.len() {
        let mut file = inner.by_index(index)?;
        if !file.name().to_ascii_lowercase().ends_with(".xml") {
            continue;
        }
        let mut xml = String::new();
        file.read_to_string(&mut xml)?;
        if let Some(feature) = parse_saa_xml(&xml, file.name(), version_label, &mut seen)? {
            features.push(feature);
        }
    }
    Ok(features)
}

#[derive(Default)]
struct SaaParseState {
    in_airspace: bool,
    in_extension: bool,
    in_circle_by_center_point: bool,
    current_tag: Option<String>,
    designator: Option<String>,
    name: Option<String>,
    upper_value: Option<String>,
    upper_unit: Option<String>,
    upper_ref: Option<String>,
    lower_value: Option<String>,
    lower_unit: Option<String>,
    lower_ref: Option<String>,
    saa_type: Option<String>,
    points: Vec<[f64; 2]>,
    paths: Vec<Vec<[f64; 2]>>,
    circle_center: Option<[f64; 2]>,
    circle_radius_unit: Option<String>,
}

fn parse_saa_xml(
    xml: &str,
    source_name: &str,
    version_label: &str,
    seen: &mut BTreeSet<String>,
) -> anyhow::Result<Option<AirspaceFeature>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut state = SaaParseState::default();
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                let name = String::from_utf8_lossy(event.local_name().as_ref()).into_owned();
                if name == "Airspace" {
                    state.in_airspace = true;
                } else if state.in_airspace {
                    if name == "AirspaceExtension" {
                        state.in_extension = true;
                    }
                    if name == "CircleByCenterPoint" {
                        state.in_circle_by_center_point = true;
                        state.circle_center = None;
                        state.circle_radius_unit = None;
                    }
                    state.current_tag = Some(name.clone());
                    if name == "upperLimit" {
                        state.upper_unit = event
                            .attributes()
                            .flatten()
                            .find(|attr| attr.key.as_ref() == b"uom")
                            .map(|attr| String::from_utf8_lossy(&attr.value).into_owned());
                    } else if name == "lowerLimit" {
                        state.lower_unit = event
                            .attributes()
                            .flatten()
                            .find(|attr| attr.key.as_ref() == b"uom")
                            .map(|attr| String::from_utf8_lossy(&attr.value).into_owned());
                    } else if name == "radius" {
                        state.circle_radius_unit = event
                            .attributes()
                            .flatten()
                            .find(|attr| attr.key.as_ref() == b"uom")
                            .map(|attr| String::from_utf8_lossy(&attr.value).into_owned());
                    }
                }
            }
            Ok(Event::Text(event)) => {
                if state.in_airspace {
                    let text = event.decode()?.trim().to_string();
                    if !text.is_empty() {
                        match state.current_tag.as_deref() {
                            Some("designator") => state.designator = Some(text),
                            Some("name") => state.name = Some(text),
                            Some("upperLimit") => state.upper_value = Some(text),
                            Some("upperLimitReference") => state.upper_ref = Some(text),
                            Some("lowerLimit") => state.lower_value = Some(text),
                            Some("lowerLimitReference") => state.lower_ref = Some(text),
                            Some("suaType") => state.saa_type = Some(text),
                            Some("pos") => {
                                if let Some(point) = parse_aixm_pos(&text) {
                                    if state.in_circle_by_center_point {
                                        state.circle_center = Some(point);
                                    } else {
                                        state.points.push(point);
                                    }
                                }
                            }
                            Some("radius") => {
                                if state.in_circle_by_center_point {
                                    if let Some(center) = state.circle_center {
                                        if let Some(path) = approximate_aixm_circle(
                                            center,
                                            &text,
                                            state.circle_radius_unit.as_deref(),
                                        ) {
                                            state.paths.push(path);
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::End(event)) => {
                let name = String::from_utf8_lossy(event.local_name().as_ref()).into_owned();
                if name == "Airspace" {
                    break;
                }
                if name == "AirspaceExtension" {
                    state.in_extension = false;
                }
                if name == "CircleByCenterPoint" {
                    state.in_circle_by_center_point = false;
                    state.circle_center = None;
                    state.circle_radius_unit = None;
                }
                if state.current_tag.as_deref() == Some(name.as_str()) {
                    state.current_tag = None;
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => {
                return Err(error).with_context(|| format!("failed parsing {source_name}"))
            }
            _ => {}
        }
    }
    if !state.points.is_empty() {
        state.paths.push(state.points);
    }
    let all_points = state.paths.iter().flatten().copied().collect::<Vec<_>>();
    if all_points.len() < 2 {
        return Ok(None);
    }
    let bbox = points_bbox(&all_points);
    let designator = state
        .designator
        .unwrap_or_else(|| source_name.trim_end_matches(".xml").to_string());
    let name = state.name.unwrap_or_else(|| designator.clone());
    let saa_type = state.saa_type.unwrap_or_else(|| "SAA".to_string());
    let lower = aixm_limit(
        state.lower_value.as_deref().unwrap_or(""),
        state.lower_unit.as_deref(),
        state.lower_ref.as_deref(),
    );
    let upper = aixm_limit(
        state.upper_value.as_deref().unwrap_or(""),
        state.upper_unit.as_deref(),
        state.upper_ref.as_deref(),
    );
    let vertical = AirspaceVertical { lower, upper };
    let vertical_label = vertical_label(&vertical);
    let id = dedup_airspace_id(
        seen,
        &format!(
            "airspace:{}:saa:{}:{}",
            version_label,
            saa_type.to_ascii_lowercase(),
            designator
        ),
    );
    let mut properties = BTreeMap::new();
    properties.insert("SOURCE_FILE".to_string(), source_name.to_string());
    properties.insert("SAA_TYPE".to_string(), saa_type.clone());
    properties.insert("DESIGNATOR".to_string(), designator.clone());
    let paths = state
        .paths
        .iter()
        .filter_map(|points| airspace_path_from_points(points, "boundary"))
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return Ok(None);
    }
    let anchor = polygon_label_anchor(&state.paths);
    Ok(Some(AirspaceFeature {
        schema_version: 1,
        id,
        kind: "airspace".to_string(),
        source: "faa_nasr_saa_aixm".to_string(),
        cycle: version_label.trim_start_matches("data_").to_string(),
        name,
        ident: Some(designator),
        airspace_class: saa_type.clone(),
        local_type: "SAA".to_string(),
        style_hint: saa_style_hint(&saa_type),
        vertical_label: vertical_label.clone(),
        vertical,
        bbox,
        label: AirspaceLabel {
            text: vertical_label,
            lon: anchor[0],
            lat: anchor[1],
        },
        paths,
        source_properties: properties,
    }))
}

#[derive(Debug, Clone)]
struct ShapefilePolygon {
    bbox: [f64; 4],
    parts: Vec<Vec<[f64; 2]>>,
}

fn read_shapefile_polygons(path: &Path) -> anyhow::Result<Vec<ShapefilePolygon>> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    if bytes.len() < 100 {
        anyhow::bail!("shapefile {} is too short", path.display());
    }
    let mut offset = 100usize;
    let mut shapes = Vec::new();
    while offset + 8 <= bytes.len() {
        let content_words = read_i32_be(&bytes, offset + 4)? as usize;
        offset += 8;
        let content_len = content_words
            .checked_mul(2)
            .context("shapefile record length overflow")?;
        if offset + content_len > bytes.len() {
            anyhow::bail!("shapefile record exceeds file length in {}", path.display());
        }
        if content_len >= 4 {
            let shape_type = read_i32_le(&bytes, offset)?;
            if matches!(shape_type, 5 | 15) {
                shapes.push(read_polygon_record(&bytes[offset..offset + content_len])?);
            }
        }
        offset += content_len;
    }
    Ok(shapes)
}

fn read_polygon_record(bytes: &[u8]) -> anyhow::Result<ShapefilePolygon> {
    if bytes.len() < 44 {
        anyhow::bail!("polygon shapefile record is too short");
    }
    let bbox = [
        round_coord(read_f64_le(bytes, 4)?),
        round_coord(read_f64_le(bytes, 12)?),
        round_coord(read_f64_le(bytes, 20)?),
        round_coord(read_f64_le(bytes, 28)?),
    ];
    let num_parts = read_i32_le(bytes, 36)? as usize;
    let num_points = read_i32_le(bytes, 40)? as usize;
    let parts_offset = 44usize;
    let points_offset = parts_offset + num_parts * 4;
    if points_offset + num_points * 16 > bytes.len() {
        anyhow::bail!("polygon shapefile record has invalid part/point lengths");
    }
    let mut part_starts = Vec::with_capacity(num_parts);
    for index in 0..num_parts {
        part_starts.push(read_i32_le(bytes, parts_offset + index * 4)? as usize);
    }
    let mut points = Vec::with_capacity(num_points);
    for index in 0..num_points {
        let base = points_offset + index * 16;
        points.push([
            round_coord(read_f64_le(bytes, base)?),
            round_coord(read_f64_le(bytes, base + 8)?),
        ]);
    }
    let mut parts = Vec::with_capacity(num_parts);
    for (part_index, start) in part_starts.iter().copied().enumerate() {
        let end = part_starts
            .get(part_index + 1)
            .copied()
            .unwrap_or(num_points);
        if start < end && end <= points.len() {
            parts.push(points[start..end].to_vec());
        }
    }
    Ok(ShapefilePolygon { bbox, parts })
}

fn read_dbf_records(path: &Path) -> anyhow::Result<Vec<BTreeMap<String, String>>> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    if bytes.len() < 32 {
        anyhow::bail!("dbf {} is too short", path.display());
    }
    let record_count = read_u32_le(&bytes, 4)? as usize;
    let header_len = read_u16_le(&bytes, 8)? as usize;
    let record_len = read_u16_le(&bytes, 10)? as usize;
    let mut fields = Vec::new();
    let mut offset = 32usize;
    while offset + 32 <= header_len && bytes[offset] != 0x0d {
        let name_end = bytes[offset..offset + 11]
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(11);
        let name = String::from_utf8_lossy(&bytes[offset..offset + name_end])
            .trim()
            .to_string();
        let len = bytes[offset + 16] as usize;
        fields.push((name, len));
        offset += 32;
    }
    let mut records = Vec::with_capacity(record_count);
    for record_index in 0..record_count {
        let record_start = header_len + record_index * record_len;
        if record_start + record_len > bytes.len() {
            break;
        }
        if bytes[record_start] == b'*' {
            records.push(BTreeMap::new());
            continue;
        }
        let mut field_offset = record_start + 1;
        let mut record = BTreeMap::new();
        for (name, len) in &fields {
            let end = (field_offset + *len).min(bytes.len());
            let value = String::from_utf8_lossy(&bytes[field_offset..end])
                .trim()
                .trim_matches('\0')
                .to_string();
            if !value.is_empty() {
                record.insert(name.clone(), value);
            }
            field_offset += *len;
        }
        records.push(record);
    }
    Ok(records)
}

#[derive(Debug, Clone, Copy)]
struct LongestRunwayInfo {
    length_ft: f64,
    heading_true_deg: f64,
    has_paved_runway: bool,
    has_water_runway: bool,
}

fn load_airport_runway_info(
    conn: &Connection,
) -> anyhow::Result<BTreeMap<String, LongestRunwayInfo>> {
    let mut stmt = conn.prepare(
        "SELECT LocationID, Length, Surface, LEHeadingT, LELatitude, LELongitude, HELatitude, HELongitude
         FROM airportrunways",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    })?;

    let mut by_airport = BTreeMap::<String, LongestRunwayInfo>::new();
    for row in rows {
        let (
            location_id,
            length_text,
            surface_text,
            le_heading_text,
            le_lat_text,
            le_lon_text,
            he_lat_text,
            he_lon_text,
        ) = row?;
        let length = parse_float(&length_text);
        if length <= 0.0 {
            continue;
        }
        let surface = surface_text.trim().to_ascii_uppercase();
        let has_paved_runway = surface_is_paved(&surface);
        let has_water_runway = surface.contains("WATER");
        let heading = parse_float(&le_heading_text);
        let heading = if heading > 0.0 {
            normalize_heading(heading)
        } else {
            let le_lat = parse_float(&le_lat_text);
            let le_lon = parse_float(&le_lon_text);
            let he_lat = parse_float(&he_lat_text);
            let he_lon = parse_float(&he_lon_text);
            if !valid_lat_lon(le_lat, le_lon) || !valid_lat_lon(he_lat, he_lon) {
                continue;
            }
            bearing_true_deg(le_lat, le_lon, he_lat, he_lon)
        };
        match by_airport.get(&location_id) {
            Some(best) if best.length_ft >= length => {
                if let Some(existing) = by_airport.get_mut(&location_id) {
                    existing.has_paved_runway |= has_paved_runway;
                    existing.has_water_runway |= has_water_runway;
                }
            }
            _ => {
                by_airport.insert(
                    location_id,
                    LongestRunwayInfo {
                        length_ft: length,
                        heading_true_deg: heading,
                        has_paved_runway,
                        has_water_runway,
                    },
                );
            }
        }
    }

    Ok(by_airport)
}

fn surface_is_paved(surface: &str) -> bool {
    surface
        .split('-')
        .any(|part| matches!(part.trim(), "ASPH" | "CONC" | "BIT" | "PEM"))
}

fn bearing_true_deg(start_lat: f64, start_lon: f64, end_lat: f64, end_lon: f64) -> f64 {
    let start_lat_rad = start_lat.to_radians();
    let end_lat_rad = end_lat.to_radians();
    let delta_lon_rad = (end_lon - start_lon).to_radians();
    let y = delta_lon_rad.sin() * end_lat_rad.cos();
    let x = start_lat_rad.cos() * end_lat_rad.sin()
        - start_lat_rad.sin() * end_lat_rad.cos() * delta_lon_rad.cos();
    normalize_heading(y.atan2(x).to_degrees())
}

fn normalize_heading(heading: f64) -> f64 {
    let normalized = heading.rem_euclid(360.0);
    (normalized * 10.0).round() / 10.0
}

fn field(line: &str, start: usize, len: usize) -> &str {
    let bytes = line.as_bytes();
    if start >= bytes.len() {
        return "";
    }
    let end = (start + len).min(bytes.len());
    std::str::from_utf8(&bytes[start..end]).unwrap_or("")
}

fn parse_float(value: &str) -> f64 {
    value.trim().parse::<f64>().unwrap_or(0.0)
}

fn round_coord(value: f64) -> f64 {
    (value * 10_000_000.0).round() / 10_000_000.0
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

fn airspace_feature_relative_path(id: &str) -> String {
    format!("had/{}.json", id.replace(':', "/"))
}

fn airspace_ref_tile_relative_path(z: u8, x: u32, y: u32) -> String {
    format!("airspace/refs/{z}/{x}/{y}.json")
}

fn airspace_label_tile_relative_path(z: u8, x: u32, y: u32) -> String {
    format!("airspace/labels/{z}/{x}/{y}.json")
}

fn tiles_for_bbox(bbox: [f64; 4], zoom: u8) -> Vec<(u8, u32, u32)> {
    let [west, south, east, north] = bbox;
    if !bbox_is_valid(bbox) {
        return Vec::new();
    }
    let (x_min, y_north) = slippy_tile(north, west, zoom);
    let (x_max, y_south) = slippy_tile(south, east, zoom);
    let x0 = x_min.min(x_max);
    let x1 = x_min.max(x_max);
    let y0 = y_north.min(y_south);
    let y1 = y_north.max(y_south);
    let mut tiles = Vec::new();
    for x in x0..=x1 {
        for y in y0..=y1 {
            tiles.push((zoom, x, y));
        }
    }
    tiles
}

fn bbox_is_visible_at_zoom(bbox: [f64; 4], zoom: u8, min_pixel_span: f64) -> bool {
    if !bbox_is_valid(bbox) {
        return false;
    }
    let [west, south, east, north] = bbox;
    let (west_px, north_px) = slippy_pixel(north, west, zoom);
    let (east_px, south_px) = slippy_pixel(south, east, zoom);
    (east_px - west_px).abs() >= min_pixel_span && (south_px - north_px).abs() >= min_pixel_span
}

fn bbox_is_valid(bbox: [f64; 4]) -> bool {
    bbox.iter().all(|value| value.is_finite())
        && bbox[0] >= -180.0
        && bbox[2] <= 180.0
        && bbox[1] >= -90.0
        && bbox[3] <= 90.0
        && bbox[0] <= bbox[2]
        && bbox[1] <= bbox[3]
}

fn property<'a>(properties: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    properties.get(key).map(|value| value.trim())
}

fn airspace_limit(properties: &BTreeMap<String, String>, prefix: &str) -> AirspaceLimit {
    let desc = property(properties, &format!("{prefix}_DESC"))
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let raw_value = property(properties, &format!("{prefix}_VAL")).unwrap_or("");
    let unit = property(properties, &format!("{prefix}_UOM"))
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let reference = property(properties, &format!("{prefix}_CODE"))
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let feet = raw_value.parse::<i32>().ok().filter(|value| *value >= 0);
    AirspaceLimit {
        display: limit_display(raw_value, reference.as_deref()),
        feet,
        unit,
        reference,
        description: desc,
    }
}

fn aixm_limit(value: &str, unit: Option<&str>, reference: Option<&str>) -> AirspaceLimit {
    let is_flight_level = unit
        .map(|value| value.trim().eq_ignore_ascii_case("FL"))
        .unwrap_or(false);
    let feet = value
        .parse::<i32>()
        .ok()
        .filter(|value| *value >= 0)
        .map(|value| if is_flight_level { value * 100 } else { value });
    AirspaceLimit {
        display: if is_flight_level {
            format!("FL{}", value.trim())
        } else {
            limit_display(value, reference)
        },
        feet,
        unit: unit.filter(|value| !value.is_empty()).map(str::to_string),
        reference: reference
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        description: None,
    }
}

fn limit_display(raw_value: &str, reference: Option<&str>) -> String {
    match reference.unwrap_or("").trim().to_ascii_uppercase().as_str() {
        "SFC" => "SFC".to_string(),
        "FL" => format!("FL{}", raw_value.trim()),
        _ => raw_value
            .trim()
            .parse::<i32>()
            .ok()
            .map(|feet| {
                if feet == 0 {
                    "SFC".to_string()
                } else if feet % 100 == 0 {
                    (feet / 100).to_string()
                } else {
                    feet.to_string()
                }
            })
            .unwrap_or_else(|| raw_value.trim().to_string()),
    }
}

fn vertical_label(vertical: &AirspaceVertical) -> String {
    format!("{}/{}", vertical.upper.display, vertical.lower.display)
}

fn airspace_style_hint(class: &str, local_type: &str) -> String {
    let class = class.trim().to_ascii_lowercase();
    let raw_local = local_type.trim().to_ascii_lowercase();
    if raw_local.starts_with("class_") {
        raw_local
    } else if !class.is_empty() {
        format!("class_{class}")
    } else if !raw_local.is_empty() {
        raw_local
    } else {
        "airspace".to_string()
    }
}

fn saa_style_hint(saa_type: &str) -> String {
    match saa_type.trim().to_ascii_uppercase().as_str() {
        "RA" => "restricted".to_string(),
        "PA" => "prohibited".to_string(),
        "MOA" => "moa".to_string(),
        "WA" => "warning".to_string(),
        "AA" => "alert".to_string(),
        "NSA" => "national_security".to_string(),
        other => format!("saa_{}", other.to_ascii_lowercase()),
    }
}

fn polygon_label_anchor(parts: &[Vec<[f64; 2]>]) -> [f64; 2] {
    if let Some(candidate) = polygon_area_centroid(parts) {
        if point_in_polygon_parts(candidate, parts) {
            return candidate;
        }
    }
    if let Some(candidate) = best_interior_label_point(parts) {
        return candidate;
    }

    polygon_vertex_average(parts)
}

fn polygon_area_centroid(parts: &[Vec<[f64; 2]>]) -> Option<[f64; 2]> {
    let mut weighted_lon = 0.0;
    let mut weighted_lat = 0.0;
    let mut total_area = 0.0;
    for part in parts {
        if let Some((centroid, area)) = polygon_ring_centroid(part) {
            weighted_lon += centroid[0] * area;
            weighted_lat += centroid[1] * area;
            total_area += area;
        }
    }
    if total_area > 0.0 {
        return Some([weighted_lon / total_area, weighted_lat / total_area]);
    }
    None
}

fn polygon_vertex_average(parts: &[Vec<[f64; 2]>]) -> [f64; 2] {
    let mut sum_lon = 0.0;
    let mut sum_lat = 0.0;
    let mut count = 0.0;
    for point in parts.iter().flatten() {
        sum_lon += point[0];
        sum_lat += point[1];
        count += 1.0;
    }
    if count > 0.0 {
        [sum_lon / count, sum_lat / count]
    } else {
        [0.0, 0.0]
    }
}

fn best_interior_label_point(parts: &[Vec<[f64; 2]>]) -> Option<[f64; 2]> {
    let bbox = parts_bbox(parts)?;
    let samples = 16usize;
    let lon_step = (bbox[2] - bbox[0]) / samples as f64;
    let lat_step = (bbox[3] - bbox[1]) / samples as f64;
    if lon_step <= 0.0 || lat_step <= 0.0 {
        return None;
    }

    let mut best_point = None;
    let mut best_distance = -1.0;
    for x_index in 0..samples {
        for y_index in 0..samples {
            let candidate = [
                bbox[0] + (x_index as f64 + 0.5) * lon_step,
                bbox[1] + (y_index as f64 + 0.5) * lat_step,
            ];
            if !point_in_polygon_parts(candidate, parts) {
                continue;
            }
            let distance = squared_distance_to_nearest_boundary(candidate, parts);
            if distance > best_distance {
                best_distance = distance;
                best_point = Some(candidate);
            }
        }
    }
    best_point
}

fn parts_bbox(parts: &[Vec<[f64; 2]>]) -> Option<[f64; 4]> {
    let mut bbox = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    let mut found = false;
    for point in parts.iter().flatten() {
        bbox[0] = bbox[0].min(point[0]);
        bbox[1] = bbox[1].min(point[1]);
        bbox[2] = bbox[2].max(point[0]);
        bbox[3] = bbox[3].max(point[1]);
        found = true;
    }
    found.then_some(bbox)
}

fn polygon_ring_centroid(points: &[[f64; 2]]) -> Option<([f64; 2], f64)> {
    if points.len() < 3 {
        return None;
    }
    let mut twice_area = 0.0;
    let mut centroid_x = 0.0;
    let mut centroid_y = 0.0;
    for index in 0..points.len() {
        let current = points[index];
        let next = points[(index + 1) % points.len()];
        let cross = current[0] * next[1] - next[0] * current[1];
        twice_area += cross;
        centroid_x += (current[0] + next[0]) * cross;
        centroid_y += (current[1] + next[1]) * cross;
    }
    if twice_area.abs() < 1.0e-12 {
        return None;
    }
    Some((
        [
            centroid_x / (3.0 * twice_area),
            centroid_y / (3.0 * twice_area),
        ],
        twice_area.abs() / 2.0,
    ))
}

fn point_in_polygon_parts(point: [f64; 2], parts: &[Vec<[f64; 2]>]) -> bool {
    parts
        .iter()
        .fold(false, |inside, part| inside ^ point_in_ring(point, part))
}

fn point_in_ring(point: [f64; 2], ring: &[[f64; 2]]) -> bool {
    if ring.len() < 3 {
        return false;
    }
    let mut inside = false;
    let x = point[0];
    let y = point[1];
    let mut previous = *ring.last().unwrap();
    for current in ring {
        let crosses = (current[1] > y) != (previous[1] > y);
        if crosses {
            let crossing_x = (previous[0] - current[0]) * (y - current[1])
                / (previous[1] - current[1])
                + current[0];
            if x < crossing_x {
                inside = !inside;
            }
        }
        previous = *current;
    }
    inside
}

fn squared_distance_to_nearest_boundary(point: [f64; 2], parts: &[Vec<[f64; 2]>]) -> f64 {
    let mut best = f64::INFINITY;
    for part in parts {
        if part.len() < 2 {
            continue;
        }
        for index in 0..part.len() {
            let start = part[index];
            let end = part[(index + 1) % part.len()];
            best = best.min(squared_distance_to_segment(point, start, end));
        }
    }
    best
}

fn nearest_boundary_point(point: [f64; 2], parts: &[Vec<[f64; 2]>]) -> Option<[f64; 2]> {
    let mut best_point = None;
    let mut best_distance = f64::INFINITY;
    for part in parts {
        if part.len() < 2 {
            continue;
        }
        for index in 0..part.len() {
            let start = part[index];
            let end = part[(index + 1) % part.len()];
            let candidate = closest_point_on_segment(point, start, end);
            let distance = squared_distance(point, candidate);
            if distance < best_distance {
                best_distance = distance;
                best_point = Some(candidate);
            }
        }
    }
    best_point
}

fn squared_distance_to_segment(point: [f64; 2], start: [f64; 2], end: [f64; 2]) -> f64 {
    squared_distance(point, closest_point_on_segment(point, start, end))
}

fn closest_point_on_segment(point: [f64; 2], start: [f64; 2], end: [f64; 2]) -> [f64; 2] {
    let segment_lon = end[0] - start[0];
    let segment_lat = end[1] - start[1];
    let length_squared = segment_lon * segment_lon + segment_lat * segment_lat;
    if length_squared <= f64::EPSILON {
        return start;
    }
    let t = ((point[0] - start[0]) * segment_lon + (point[1] - start[1]) * segment_lat)
        / length_squared;
    let t = t.clamp(0.0, 1.0);
    [start[0] + t * segment_lon, start[1] + t * segment_lat]
}

fn squared_distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    let lon_delta = a[0] - b[0];
    let lat_delta = a[1] - b[1];
    lon_delta * lon_delta + lat_delta * lat_delta
}

fn airport_adjusted_label_anchor(
    anchor: [f64; 2],
    parts: &[Vec<[f64; 2]>],
    class: &str,
    ident: Option<&str>,
    airport_points: &[PointRecord],
) -> Option<[f64; 2]> {
    if !matches!(class.trim().to_ascii_uppercase().as_str(), "B" | "C" | "D") {
        return None;
    }
    let airport = matching_airport_in_polygon(parts, ident, anchor, airport_points)?;
    if squared_distance(anchor, [airport.lon, airport.lat]) > 0.01 {
        return None;
    }
    let edge = nearest_boundary_point([airport.lon, airport.lat], parts)?;
    for fraction in [0.5, 0.4, 0.6, 0.33, 0.67] {
        let candidate = [
            airport.lon + (edge[0] - airport.lon) * fraction,
            airport.lat + (edge[1] - airport.lat) * fraction,
        ];
        if point_in_polygon_parts(candidate, parts) {
            return Some(candidate);
        }
    }
    None
}

fn matching_airport_in_polygon<'a>(
    parts: &[Vec<[f64; 2]>],
    ident: Option<&str>,
    anchor: [f64; 2],
    airport_points: &'a [PointRecord],
) -> Option<&'a PointRecord> {
    if let Some(exact_ident) = ident.map(normalize_ident) {
        if let Some(airport) = airport_points.iter().find(|airport| {
            normalize_ident(airport_raw_identifier(airport).unwrap_or("")) == exact_ident
                && point_in_polygon_parts([airport.lon, airport.lat], parts)
        }) {
            return Some(airport);
        }
    }

    let bbox = parts_bbox(parts)?;
    airport_points
        .iter()
        .filter(|airport| {
            airport.lon >= bbox[0]
                && airport.lon <= bbox[2]
                && airport.lat >= bbox[1]
                && airport.lat <= bbox[3]
                && squared_distance(anchor, [airport.lon, airport.lat]) <= 0.01
                && point_in_polygon_parts([airport.lon, airport.lat], parts)
        })
        .min_by(|left, right| {
            squared_distance(anchor, [left.lon, left.lat])
                .partial_cmp(&squared_distance(anchor, [right.lon, right.lat]))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

fn airport_raw_identifier(airport: &PointRecord) -> Option<&str> {
    airport
        .id
        .strip_prefix("airport:")
        .and_then(|value| value.split(':').next())
}

fn normalize_ident(ident: &str) -> String {
    let ident = ident.trim().to_ascii_uppercase();
    if ident.len() == 4 && ident.starts_with('K') {
        ident[1..].to_string()
    } else {
        ident
    }
}

fn parse_aixm_pos(text: &str) -> Option<[f64; 2]> {
    let mut parts = text.split_whitespace();
    let lon = round_coord(parts.next()?.parse::<f64>().ok()?);
    let lat = round_coord(parts.next()?.parse::<f64>().ok()?);
    valid_lat_lon(lat, lon).then_some([lon, lat])
}

fn approximate_aixm_circle(
    center: [f64; 2],
    radius_value: &str,
    radius_unit: Option<&str>,
) -> Option<Vec<[f64; 2]>> {
    let radius = radius_value.trim().parse::<f64>().ok()?;
    if radius <= 0.0 {
        return None;
    }
    let radius_nm = match radius_unit
        .unwrap_or("")
        .trim()
        .to_ascii_uppercase()
        .as_str()
    {
        "NM" => radius,
        "M" => radius / 1852.0,
        "KM" => radius / 1.852,
        "FT" => radius / 6076.11549,
        _ => return None,
    };
    let lat_radius_degrees = radius_nm / 60.0;
    let cos_lat = center[1].to_radians().cos().abs().max(0.000_001);
    let lon_radius_degrees = lat_radius_degrees / cos_lat;
    let sample_count = 96;
    let mut points = Vec::with_capacity(sample_count + 1);
    for index in 0..=sample_count {
        let angle = std::f64::consts::TAU * (index as f64) / (sample_count as f64);
        let lon = round_coord(center[0] + lon_radius_degrees * angle.cos());
        let lat = round_coord(center[1] + lat_radius_degrees * angle.sin());
        if valid_lat_lon(lat, lon) {
            points.push([lon, lat]);
        }
    }
    (points.len() > 3).then_some(points)
}

fn airspace_path_from_points(points: &[[f64; 2]], role: &str) -> Option<AirspacePath> {
    let _ = points.first()?;
    Some(AirspacePath {
        role: role.to_string(),
        closed: points.first() == points.last(),
        points: points.to_vec(),
    })
}

fn points_bbox(points: &[[f64; 2]]) -> [f64; 4] {
    let mut west = f64::INFINITY;
    let mut south = f64::INFINITY;
    let mut east = f64::NEG_INFINITY;
    let mut north = f64::NEG_INFINITY;
    for point in points {
        west = west.min(point[0]);
        east = east.max(point[0]);
        south = south.min(point[1]);
        north = north.max(point[1]);
    }
    [west, south, east, north]
}

fn dedup_airspace_id(seen: &mut BTreeSet<String>, base: &str) -> String {
    let normalized = base
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == ':' || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    if seen.insert(normalized.clone()) {
        return normalized;
    }
    let mut index = 2usize;
    loop {
        let candidate = format!("{normalized}:{index}");
        if seen.insert(candidate.clone()) {
            return candidate;
        }
        index += 1;
    }
}

fn slippy_tile(lat: f64, lon: f64, zoom: u8) -> (u32, u32) {
    let (x, y) = slippy_pixel(lat, lon, zoom);
    (clamp_tile(x / 256.0, zoom), clamp_tile(y / 256.0, zoom))
}

fn slippy_pixel(lat: f64, lon: f64, zoom: u8) -> (f64, f64) {
    let lat_rad = lat.to_radians();
    let scale = 256.0 * 2_f64.powi(zoom as i32);
    let x = (lon + 180.0) / 360.0 * scale;
    let y =
        (1.0 - ((lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI)) / 2.0 * scale;
    (x, y)
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

fn read_i32_be(bytes: &[u8], offset: usize) -> anyhow::Result<i32> {
    Ok(i32::from_be_bytes(read_array(bytes, offset)?))
}

fn read_i32_le(bytes: &[u8], offset: usize) -> anyhow::Result<i32> {
    Ok(i32::from_le_bytes(read_array(bytes, offset)?))
}

fn read_u16_le(bytes: &[u8], offset: usize) -> anyhow::Result<u16> {
    Ok(u16::from_le_bytes(read_array(bytes, offset)?))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> anyhow::Result<u32> {
    Ok(u32::from_le_bytes(read_array(bytes, offset)?))
}

fn read_f64_le(bytes: &[u8], offset: usize) -> anyhow::Result<f64> {
    Ok(f64::from_le_bytes(read_array(bytes, offset)?))
}

fn read_array<const N: usize>(bytes: &[u8], offset: usize) -> anyhow::Result<[u8; N]> {
    let end = offset + N;
    let slice = bytes
        .get(offset..end)
        .with_context(|| format!("buffer too short at offset {offset} for {N} bytes"))?;
    let mut out = [0_u8; N];
    out.copy_from_slice(slice);
    Ok(out)
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

fn write_json_compact<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(
        path,
        serde_json::to_vec(value).context("failed to encode json")?,
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
