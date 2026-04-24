use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use geo::{BooleanOps, Coord, LineString, MultiPolygon, Polygon};
use quick_xml::events::Event;
use quick_xml::Reader;
use rusqlite::Connection;
use serde::Serialize;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

const POINT_LAYER_ZOOM_POLICY: &[(&str, u8)] =
    &[("airport", 9), ("fix", 9), ("nav", 9), ("awos", 9)];
const MIN_OBSTACLE_AGL_FT: i32 = 400;
const TALL_OBSTACLE_MIN_AGL_FT: i32 = 1000;
const OBSTACLE_LAYER_ZOOM: u8 = 12;
const OBSTACLE_LAYER_MIN_ZOOM: u8 = 0;
const OBSTACLE_LAYER_MAX_ZOOM: u8 = 12;
const OBSTACLE_THINNING_MAX_ZOOM: u8 = 11;
// These AGL thresholds were derived with:
// `preprocessor-cli analyze-obstacle-thresholds --input-dir <obstacle-input-dir>`
// against the 2026-04-24 obstacle source, choosing the lowest 50 ft increment at each zoom that
// kept the busiest tile at or below 100 obstacles. We only apply the additional thinning at z11
// and below; z12 intentionally keeps the full obstacle set above the existing 400 ft AGL floor
// even though the analyzer can suggest a higher cap-compliant threshold there.
const OBSTACLE_MIN_AGL_BY_ZOOM: &[(u8, i32)] = &[
    (0, 1800),
    (1, 1800),
    (2, 1600),
    (3, 1550),
    (4, 1500),
    (5, 1150),
    (6, 850),
    (7, 750),
    (8, 700),
    (9, 700),
    (10, 700),
    (11, 700),
    (12, MIN_OBSTACLE_AGL_FT),
];
const AIRSPACE_REF_MIN_ZOOM: u8 = 0;
const AIRSPACE_REF_MAX_ZOOM: u8 = 12;
const AIRSPACE_REF_MIN_PIXEL_SPAN: f64 = 30.0;
const MOA_REF_MIN_ZOOM: u8 = 8;
const CONTROLLED_AIRSPACE_DETAIL_MIN_PIXEL_SPAN: f64 = 20.0;
const CONTROLLED_AIRSPACE_OUTLINE_MAX_ZOOM: u8 = 8;
const CONTROLLED_AIRSPACE_OUTLINE_SIMPLIFY_TOLERANCE_DEGREES: f64 = 0.005;
const CONTROLLED_AIRSPACE_OUTLINE_UNION_SNAP_GRID_DEGREES: f64 = 0.0001;
const CONTROLLED_AIRSPACE_OUTLINE_UNION_EXPAND_DEGREES: f64 = 0.001;
const AIRSPACE_LABEL_MIN_ZOOM: u8 = 0;
const AIRSPACE_LABEL_MAX_ZOOM: u8 = 12;
const AIRSPACE_LABEL_MIN_PIXEL_SPAN: f64 = 50.0;
const AIRSPACE_LABEL_MIN_EDGE_CLEARANCE_PX: f64 = 25.0;
const AIRSPACE_LABEL_SAMPLE_GRID: usize = 10;
const AIRSPACE_LABEL_CONTAINMENT_RATIO: f64 = 0.98;

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
    pub errors_path: PathBuf,
    pub zip_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct BuildBravoUnionSvgRequest {
    pub class_airspace_shp: PathBuf,
    pub output_svg: PathBuf,
    pub version_label: String,
}

#[derive(Debug, Clone)]
pub struct BuildBravoUnionSvgResult {
    pub output_svg: PathBuf,
    pub bravo_count: usize,
    pub source_shelf_count: usize,
    pub union_polygon_count: usize,
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

#[derive(Debug, Clone)]
pub struct AnalyzeObstacleThresholdsRequest {
    pub input_dir: PathBuf,
    pub cap_per_tile: usize,
    pub min_zoom: u8,
    pub max_zoom: u8,
    pub threshold_step_ft: i32,
}

#[derive(Debug, Clone)]
pub struct ObstacleThresholdAnalysisRow {
    pub zoom: u8,
    pub min_agl_ft: i32,
    pub kept_points: usize,
    pub nonempty_tiles: usize,
    pub max_points_per_tile: usize,
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
    diagnostic_error_count: usize,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct BuildDiagnostics {
    schema_version: u32,
    product: String,
    version_label: String,
    error_count: usize,
    errors: Vec<BuildDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
struct BuildDiagnostic {
    severity: String,
    code: String,
    message: String,
    expected: usize,
    actual: usize,
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
    min_zoom: u8,
    max_zoom: u8,
    available_zooms: Vec<u8>,
    tile_path_template: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    zoom_levels: Option<Vec<PointLayerZoomLevelManifest>>,
}

#[derive(Debug, Clone, Serialize)]
struct PointLayerStats {
    zoom: u8,
    tile_count: usize,
    max_points_in_tile: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    zoom_levels: Option<Vec<PointLayerZoomLevelStats>>,
}

#[derive(Debug, Clone, Serialize)]
struct PointLayerZoomLevelManifest {
    zoom: u8,
    filtered: bool,
    min_agl_ft: i32,
}

#[derive(Debug, Clone, Serialize)]
struct PointLayerZoomLevelStats {
    zoom: u8,
    filtered: bool,
    min_agl_ft: i32,
    kept_points: usize,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    saa_source_xml_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    saa_emitted_feature_count: Option<usize>,
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
    obstacle: Option<ObstacleProperties>,
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
struct ObstacleProperties {
    height_agl_ft: f64,
    elevation_msl_ft: f64,
    top_msl_ft: f64,
    is_tall: bool,
}

#[derive(Debug, Clone)]
struct ObstaclePointRecord {
    record: PointRecord,
    agl_ft: i32,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    label_candidates: Vec<AirspaceLabelCandidate>,
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
struct AirspaceLabelCandidate {
    rank: u32,
    score: f64,
    lon: f64,
    lat: f64,
}

#[derive(Debug, Clone, Serialize)]
struct AirspacePath {
    role: String,
    closed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    interior_side: Option<String>,
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
    rank: u32,
    score: f64,
    style_hint: String,
}

pub fn build_bravo_union_svg(
    request: &BuildBravoUnionSvgRequest,
) -> anyhow::Result<BuildBravoUnionSvgResult> {
    let (features, _) = load_class_airspace_features(
        &request.class_airspace_shp,
        &request.version_label,
        false,
        &[],
    )
    .with_context(|| {
        format!(
            "failed to load class airspace shapefile {}",
            request.class_airspace_shp.display()
        )
    })?;
    let mut groups = BTreeMap::<String, Vec<AirspaceFeature>>::new();
    for feature in features {
        if feature.airspace_class.eq_ignore_ascii_case("B") {
            let key = feature
                .ident
                .clone()
                .filter(|ident| !ident.is_empty())
                .unwrap_or_else(|| feature.name.clone());
            groups.entry(key).or_default().push(feature);
        }
    }

    let mut rendered = Vec::new();
    let mut union_polygon_count = 0usize;
    let mut source_shelf_count = 0usize;
    for (ident, group) in groups {
        source_shelf_count += group.len();
        let union = geo_union_for_airspace_group(&group)
            .with_context(|| format!("failed to union Class B airspace group {ident}"))?;
        union_polygon_count += union.0.len();
        if !union.0.is_empty() {
            rendered.push(BravoUnionSvgCell {
                ident,
                bbox: bbox_for_airspace_group(&group).unwrap_or([-180.0, -90.0, 180.0, 90.0]),
                source_features: group,
                union_rings: exterior_rings_from_geo_union(&union),
            });
        }
    }

    rendered.sort_by(|left, right| left.ident.cmp(&right.ident));
    write_bravo_union_svg(&request.output_svg, &rendered)?;

    Ok(BuildBravoUnionSvgResult {
        output_svg: request.output_svg.clone(),
        bravo_count: rendered.len(),
        source_shelf_count,
        union_polygon_count,
    })
}

struct BravoUnionSvgCell {
    ident: String,
    bbox: [f64; 4],
    source_features: Vec<AirspaceFeature>,
    union_rings: Vec<Vec<[f64; 2]>>,
}

fn geo_union_for_airspace_group(features: &[AirspaceFeature]) -> anyhow::Result<MultiPolygon<f64>> {
    let mut union = MultiPolygon::<f64>(Vec::new());
    for feature in features {
        for path in &feature.paths {
            let Some(polygon) = controlled_airspace_outline_polygon_from_path(path) else {
                continue;
            };
            union = if union.0.is_empty() {
                MultiPolygon(vec![polygon])
            } else {
                union.union(&polygon)
            };
        }
    }
    Ok(union)
}

fn exterior_rings_from_geo_union(union: &MultiPolygon<f64>) -> Vec<Vec<[f64; 2]>> {
    union
        .0
        .iter()
        .map(|polygon| {
            polygon
                .exterior()
                .points()
                .map(|point| [point.x(), point.y()])
                .collect::<Vec<_>>()
        })
        .collect()
}

fn geo_union_for_airspace_refs(features: &[&AirspaceFeature]) -> MultiPolygon<f64> {
    let mut union = MultiPolygon::<f64>(Vec::new());
    for feature in features {
        for path in &feature.paths {
            let Some(polygon) = controlled_airspace_outline_polygon_from_path(path) else {
                continue;
            };
            union = if union.0.is_empty() {
                MultiPolygon(vec![polygon])
            } else {
                union.union(&polygon)
            };
        }
    }
    union
}

fn controlled_airspace_outline_polygon_from_path(path: &AirspacePath) -> Option<Polygon<f64>> {
    if !path.closed || path.points.len() < 4 {
        return None;
    }
    let snapped = path
        .points
        .iter()
        .map(|point| {
            [
                snap_coord(
                    point[0],
                    CONTROLLED_AIRSPACE_OUTLINE_UNION_SNAP_GRID_DEGREES,
                ),
                snap_coord(
                    point[1],
                    CONTROLLED_AIRSPACE_OUTLINE_UNION_SNAP_GRID_DEGREES,
                ),
            ]
        })
        .collect::<Vec<_>>();
    let expanded = expand_ring_outward_by_vertex_bisectors(
        &snapped,
        CONTROLLED_AIRSPACE_OUTLINE_UNION_EXPAND_DEGREES,
    );
    let mut coords = expanded
        .iter()
        .map(|point| Coord {
            x: point[0],
            y: point[1],
        })
        .collect::<Vec<_>>();
    let first = *coords.first()?;
    if coords.last().copied() != Some(first) {
        coords.push(first);
    }
    if coords.len() < 4 {
        return None;
    }
    Some(Polygon::new(LineString::new(coords), Vec::new()))
}

fn snap_coord(value: f64, grid: f64) -> f64 {
    if grid <= 0.0 {
        value
    } else {
        (value / grid).round() * grid
    }
}

fn expand_ring_outward_by_vertex_bisectors(points: &[[f64; 2]], epsilon: f64) -> Vec<[f64; 2]> {
    // This is not a general GIS buffer. It is a low-zoom-only nudge that makes
    // neighboring FAA shelf polygons overlap before boolean union, without
    // publishing the expanded geometry. Moving each vertex along the local
    // outward angle bisector avoids the concave-polygon spikes we saw when
    // expanding vertices away from the polygon centroid.
    if epsilon <= 0.0 || points.len() < 4 {
        return points.to_vec();
    }
    let clockwise = signed_ring_area(points).is_some_and(|area| area < 0.0);
    let mut open = points.to_vec();
    let was_closed = open.first() == open.last();
    if was_closed {
        open.pop();
    }
    if open.len() < 3 {
        return points.to_vec();
    }

    let mut expanded = open
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let previous = open[(index + open.len() - 1) % open.len()];
            let next = open[(index + 1) % open.len()];
            let incoming = outward_unit_normal(previous, *point, clockwise);
            let outgoing = outward_unit_normal(*point, next, clockwise);
            let direction =
                normalize_vector([incoming[0] + outgoing[0], incoming[1] + outgoing[1]])
                    .or_else(|| normalize_vector([point[0] - previous[0], point[1] - previous[1]]))
                    .unwrap_or([0.0, 0.0]);
            [
                point[0] + epsilon * direction[0],
                point[1] + epsilon * direction[1],
            ]
        })
        .collect::<Vec<_>>();
    if was_closed {
        if let Some(first) = expanded.first().copied() {
            expanded.push(first);
        }
    }
    expanded
}

fn outward_unit_normal(start: [f64; 2], end: [f64; 2], clockwise: bool) -> [f64; 2] {
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let normal = if clockwise { [-dy, dx] } else { [dy, -dx] };
    normalize_vector(normal).unwrap_or([0.0, 0.0])
}

fn normalize_vector(vector: [f64; 2]) -> Option<[f64; 2]> {
    let length = (vector[0] * vector[0] + vector[1] * vector[1]).sqrt();
    if length <= f64::EPSILON {
        None
    } else {
        Some([vector[0] / length, vector[1] / length])
    }
}

fn bbox_for_airspace_group(features: &[AirspaceFeature]) -> Option<[f64; 4]> {
    let mut bbox: Option<[f64; 4]> = None;
    for feature in features {
        bbox = Some(match bbox {
            Some(existing) => [
                existing[0].min(feature.bbox[0]),
                existing[1].min(feature.bbox[1]),
                existing[2].max(feature.bbox[2]),
                existing[3].max(feature.bbox[3]),
            ],
            None => feature.bbox,
        });
    }
    bbox
}

fn write_bravo_union_svg(path: &Path, cells: &[BravoUnionSvgCell]) -> anyhow::Result<()> {
    let columns = 5usize;
    let cell_width = 360.0;
    let cell_height = 300.0;
    let title_height = 24.0;
    let padding = 18.0;
    let rows = cells.len().div_ceil(columns);
    let width = (columns as f64 * cell_width) as usize;
    let height = (rows as f64 * cell_height) as usize;
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<rect width="100%" height="100%" fill="#f8fafc"/>
<style>
  text {{ font-family: ui-sans-serif, sans-serif; font-size: 13px; fill: #0f172a; }}
  .cell {{ fill: #ffffff; stroke: #cbd5e1; stroke-width: 1; }}
  .source {{ fill: #2563eb; fill-opacity: 0.12; stroke: #2563eb; stroke-opacity: 0.50; stroke-width: 1; }}
  .union {{ fill: none; stroke: #dc2626; stroke-width: 2.25; stroke-linejoin: round; stroke-linecap: round; }}
</style>
"##
    );
    for (index, cell) in cells.iter().enumerate() {
        let column = index % columns;
        let row = index / columns;
        let x0 = column as f64 * cell_width;
        let y0 = row as f64 * cell_height;
        svg.push_str(&format!(
            r##"<g transform="translate({x0:.1},{y0:.1})">
<rect class="cell" x="4" y="4" width="{:.1}" height="{:.1}" rx="8"/>
<text x="14" y="20">{} - {} shelves, {} union polys</text>
"##,
            cell_width - 8.0,
            cell_height - 8.0,
            svg_escape(&cell.ident),
            cell.source_features.len(),
            cell.union_rings.len()
        ));
        let projector =
            SvgCellProjector::new(cell.bbox, cell_width, cell_height, title_height, padding);
        for feature in &cell.source_features {
            for path in &feature.paths {
                if path.closed {
                    svg.push_str(&format!(
                        r##"<path class="source" d="{}"/>
"##,
                        svg_path_for_points(&path.points, true, &projector)
                    ));
                }
            }
        }
        for exterior in &cell.union_rings {
            let simplified = simplify_closed_ring(
                exterior,
                CONTROLLED_AIRSPACE_OUTLINE_SIMPLIFY_TOLERANCE_DEGREES,
            );
            svg.push_str(&format!(
                r##"<path class="union" d="{}"/>
"##,
                svg_path_for_points(&simplified, true, &projector)
            ));
        }
        svg.push_str("</g>\n");
    }
    svg.push_str("</svg>\n");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, svg).with_context(|| format!("failed to write {}", path.display()))
}

struct SvgCellProjector {
    west: f64,
    north: f64,
    scale: f64,
    x_offset: f64,
    y_offset: f64,
}

impl SvgCellProjector {
    fn new(bbox: [f64; 4], cell_width: f64, cell_height: f64, title_height: f64, pad: f64) -> Self {
        let lon_span = (bbox[2] - bbox[0]).max(0.000001);
        let lat_span = (bbox[3] - bbox[1]).max(0.000001);
        let plot_width = cell_width - 2.0 * pad;
        let plot_height = cell_height - title_height - 2.0 * pad;
        let scale = (plot_width / lon_span).min(plot_height / lat_span);
        let drawn_width = lon_span * scale;
        let drawn_height = lat_span * scale;
        Self {
            west: bbox[0],
            north: bbox[3],
            scale,
            x_offset: pad + (plot_width - drawn_width) / 2.0,
            y_offset: title_height + pad + (plot_height - drawn_height) / 2.0,
        }
    }

    fn point(&self, point: [f64; 2]) -> (f64, f64) {
        (
            self.x_offset + (point[0] - self.west) * self.scale,
            self.y_offset + (self.north - point[1]) * self.scale,
        )
    }
}

fn svg_path_for_points(points: &[[f64; 2]], closed: bool, projector: &SvgCellProjector) -> String {
    let mut out = String::new();
    for (index, point) in points.iter().enumerate() {
        let (x, y) = projector.point(*point);
        if index == 0 {
            out.push_str(&format!("M{x:.2},{y:.2}"));
        } else {
            out.push_str(&format!("L{x:.2},{y:.2}"));
        }
    }
    if closed {
        out.push('Z');
    }
    out
}

fn svg_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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
    let mut diagnostic_errors = Vec::new();
    let mut saa_source_xml_count = None;
    let mut saa_emitted_feature_count = None;
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
        let saa = load_saa_airspace_features(path, &request.version_label)
            .with_context(|| format!("failed to load SAA AIXM {}", path.display()))?;
        if saa.source_xml_count != saa.features.len() {
            diagnostic_errors.push(BuildDiagnostic {
                severity: "ERROR".to_string(),
                code: "saa_feature_count_mismatch".to_string(),
                message: format!(
                    "SAA AIXM source XML count ({}) does not match emitted feature count ({})",
                    saa.source_xml_count,
                    saa.features.len()
                ),
                expected: saa.source_xml_count,
                actual: saa.features.len(),
            });
        }
        saa_source_xml_count = Some(saa.source_xml_count);
        saa_emitted_feature_count = Some(saa.features.len());
        airspace_features.extend(saa.features);
    }
    let stats_path = request.output_dir.join("stats.json");
    let errors_path = request.output_dir.join("errors.json");
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
                min_zoom: zoom,
                max_zoom: zoom,
                available_zooms: vec![zoom],
                tile_path_template,
                zoom_levels: None,
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
                zoom_levels: None,
            },
        );
    }

    let mut airspace_manifest = None;
    let mut airspace_stats = None;
    if !airspace_features.is_empty() {
        let controlled_outline_features =
            build_controlled_airspace_outline_features(&airspace_features, &request.version_label);
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
            write_airspace_feature(&request.output_dir, feature, &mut zip_members)?;

            for zoom in AIRSPACE_REF_MIN_ZOOM..=AIRSPACE_REF_MAX_ZOOM {
                if controlled_airspace_uses_outline_at_zoom(feature, zoom) {
                    continue;
                }
                if !airspace_ref_is_available_at_zoom(feature, zoom) {
                    continue;
                }
                if !bbox_is_visible_at_zoom(
                    feature.bbox,
                    zoom,
                    airspace_ref_min_pixel_span(feature),
                ) {
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
                for candidate in &feature.label_candidates {
                    if !label_candidate_has_airspace_edge_clearance(
                        candidate.score,
                        candidate.lat,
                        zoom,
                    ) {
                        continue;
                    }
                    let (label_x, label_y) = slippy_tile(candidate.lat, candidate.lon, zoom);
                    insert_airspace_tile_label(
                        label_tiles.entry((zoom, label_x, label_y)).or_default(),
                        AirspaceTileLabel {
                            feature_id: feature.id.clone(),
                            text: feature.label.text.clone(),
                            lon: candidate.lon,
                            lat: candidate.lat,
                            rank: candidate.rank,
                            score: candidate.score,
                            style_hint: feature.style_hint.clone(),
                        },
                    );
                }
            }
        }

        for feature in &controlled_outline_features {
            write_airspace_feature(&request.output_dir, feature, &mut zip_members)?;

            for zoom in AIRSPACE_REF_MIN_ZOOM..=CONTROLLED_AIRSPACE_OUTLINE_MAX_ZOOM {
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
            saa_source_xml_count,
            saa_emitted_feature_count,
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
        diagnostic_error_count: diagnostic_errors.len(),
        warnings: Vec::new(),
    };
    write_json_pretty(&stats_path, &stats)?;
    write_json_pretty(
        &errors_path,
        &BuildDiagnostics {
            schema_version: 1,
            product: "vectors".to_string(),
            version_label: request.version_label.clone(),
            error_count: diagnostic_errors.len(),
            errors: diagnostic_errors,
        },
    )?;

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
        errors_path,
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

    let obstacle_points = load_obstacle_points(&request.input_dir)?;
    let stats_path = request.output_dir.join("stats.json");
    let manifest_path = request
        .output_dir
        .join(format!("obstacles_{}.manifest", request.version_label));
    let zip_path = request
        .output_dir
        .join(format!("obstacles_{}.zip", request.version_label));

    let available_zooms = (OBSTACLE_LAYER_MIN_ZOOM..=OBSTACLE_LAYER_MAX_ZOOM).collect::<Vec<_>>();
    let tile_path_template = "points/obstacle/{z}/{x}/{y}.json".to_string();

    let mut files = BTreeMap::new();
    let mut point_layers = BTreeMap::new();
    let mut zip_members = vec![
        ("obstacles".to_string(), manifest_path.clone()),
        ("stats.json".to_string(), stats_path.clone()),
    ];
    let mut zoom_level_stats = Vec::new();

    for &(zoom, min_agl_ft) in OBSTACLE_MIN_AGL_BY_ZOOM {
        let filtered = zoom <= OBSTACLE_THINNING_MAX_ZOOM;
        let filtered_points = obstacle_points
            .iter()
            .filter(|point| point.agl_ft >= min_agl_ft)
            .map(|point| point.record.clone())
            .collect::<Vec<_>>();
        let point_tiles = build_point_tiles(&filtered_points, zoom);
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
        zoom_level_stats.push(PointLayerZoomLevelStats {
            zoom,
            filtered,
            min_agl_ft,
            kept_points: filtered_points.len(),
            tile_count: point_tiles.len(),
            max_points_in_tile: point_tiles
                .iter()
                .map(|tile| tile.records.len())
                .max()
                .unwrap_or(0),
        });
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
            min_zoom: OBSTACLE_LAYER_MIN_ZOOM,
            max_zoom: OBSTACLE_LAYER_MAX_ZOOM,
            available_zooms,
            tile_path_template,
            zoom_levels: Some(
                OBSTACLE_MIN_AGL_BY_ZOOM
                    .iter()
                    .map(|&(zoom, min_agl_ft)| PointLayerZoomLevelManifest {
                        zoom,
                        filtered: zoom <= OBSTACLE_THINNING_MAX_ZOOM,
                        min_agl_ft,
                    })
                    .collect(),
            ),
        },
    );

    write_json_pretty(
        &stats_path,
        &VectorStats {
            schema_version: 1,
            version_label: request.version_label.clone(),
            points: PointStats {
                total_points: obstacle_points.len(),
                layer_counts: BTreeMap::from([("obstacle".to_string(), obstacle_points.len())]),
                layers: BTreeMap::from([(
                    "obstacle".to_string(),
                    PointLayerStats {
                        zoom: OBSTACLE_LAYER_ZOOM,
                        tile_count: zoom_level_stats
                            .iter()
                            .find(|stats| stats.zoom == OBSTACLE_LAYER_ZOOM)
                            .map(|stats| stats.tile_count)
                            .unwrap_or(0),
                        max_points_in_tile: zoom_level_stats
                            .iter()
                            .find(|stats| stats.zoom == OBSTACLE_LAYER_ZOOM)
                            .map(|stats| stats.max_points_in_tile)
                            .unwrap_or(0),
                        zoom_levels: Some(zoom_level_stats),
                    },
                )]),
            },
            airspace: None,
            diagnostic_error_count: 0,
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

pub fn analyze_obstacle_thresholds(
    request: &AnalyzeObstacleThresholdsRequest,
) -> anyhow::Result<Vec<ObstacleThresholdAnalysisRow>> {
    let obstacle_points = load_obstacle_points(&request.input_dir)?;
    let max_agl_ft = obstacle_points
        .iter()
        .map(|point| point.agl_ft)
        .max()
        .unwrap_or(MIN_OBSTACLE_AGL_FT);
    let mut thresholds = Vec::new();
    let mut threshold = ((max_agl_ft + request.threshold_step_ft - 1) / request.threshold_step_ft)
        * request.threshold_step_ft;
    while threshold >= MIN_OBSTACLE_AGL_FT {
        thresholds.push(threshold);
        threshold -= request.threshold_step_ft;
    }
    if thresholds.last().copied() != Some(MIN_OBSTACLE_AGL_FT) {
        thresholds.push(MIN_OBSTACLE_AGL_FT);
    }

    let mut rows = Vec::new();
    for zoom in request.min_zoom..=request.max_zoom {
        let mut selected = None;
        let mut fallback = None;
        for min_agl_ft in thresholds.iter().copied().rev() {
            let filtered_points = obstacle_points
                .iter()
                .filter(|point| point.agl_ft >= min_agl_ft)
                .map(|point| point.record.clone())
                .collect::<Vec<_>>();
            let point_tiles = build_point_tiles(&filtered_points, zoom);
            let row = ObstacleThresholdAnalysisRow {
                zoom,
                min_agl_ft,
                kept_points: filtered_points.len(),
                nonempty_tiles: point_tiles.len(),
                max_points_per_tile: point_tiles
                    .iter()
                    .map(|tile| tile.records.len())
                    .max()
                    .unwrap_or(0),
            };
            fallback = Some(row.clone());
            if row.max_points_per_tile <= request.cap_per_tile {
                selected = Some(row);
                break;
            }
        }
        rows.push(selected.or(fallback).expect("threshold list should not be empty"));
    }
    Ok(rows)
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
                obstacle: None,
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

fn load_obstacle_points(input_dir: &Path) -> anyhow::Result<Vec<ObstaclePointRecord>> {
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
        if height_agl < MIN_OBSTACLE_AGL_FT as f64 || !valid_lat_lon(lat, lon) {
            continue;
        }
        let elevation_msl = height_msl - height_agl;
        let id = dedup_id(
            &mut seen,
            &format!("obs:{lat:.6}:{lon:.6}:{height_msl:.0}"),
            lat,
            lon,
        );
        points.push(ObstaclePointRecord {
            record: PointRecord {
                id,
                kind: "obs".to_string(),
                lat,
                lon,
                label: format!("{:.0}", height_msl),
                style_class: "obstacle".to_string(),
                obstacle: Some(ObstacleProperties {
                    height_agl_ft: height_agl,
                    elevation_msl_ft: elevation_msl,
                    top_msl_ft: height_msl,
                    is_tall: height_agl >= TALL_OBSTACLE_MIN_AGL_FT as f64,
                }),
                towered: None,
                fuel_available: None,
                public_use: None,
                private_use: None,
                has_paved_runway: None,
                heliport: None,
                has_water_runway: None,
                longest_runway_length_ft: None,
                longest_runway_heading_true_deg: None,
            },
            agl_ft: height_agl.round() as i32,
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
        if polygon_area_centroid(&shape.parts)
            .is_some_and(|candidate| !point_in_polygon_parts(candidate, &shape.parts))
        {
            diagnostics.class_label_candidate_outside_polygon_count += 1;
        }
        let vertical_label = vertical_label(&vertical);
        let label_anchor = polygon_label_anchor(&shape.parts);
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
            label_candidates: Vec::new(),
            paths,
            source_properties: properties,
        });
    }

    assign_class_airspace_label_candidates(&mut features, airport_points, &mut diagnostics);

    Ok((features, diagnostics))
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ControlledAirspaceOutlineKey {
    class: String,
    ident: String,
    local_type: String,
}

fn build_controlled_airspace_outline_features(
    features: &[AirspaceFeature],
    version_label: &str,
) -> Vec<AirspaceFeature> {
    let mut groups = BTreeMap::<ControlledAirspaceOutlineKey, Vec<&AirspaceFeature>>::new();
    for feature in features {
        if !is_controlled_airspace_detail(feature) {
            continue;
        }
        groups
            .entry(ControlledAirspaceOutlineKey {
                class: feature.airspace_class.to_ascii_lowercase(),
                ident: feature.ident.clone().unwrap_or_else(|| "anon".to_string()),
                local_type: feature.local_type.to_ascii_lowercase(),
            })
            .or_default()
            .push(feature);
    }

    let mut seen = BTreeSet::new();
    groups
        .into_iter()
        .filter_map(|(key, group)| {
            controlled_airspace_outline_feature(key, group, version_label, &mut seen)
        })
        .collect()
}

fn controlled_airspace_outline_feature(
    key: ControlledAirspaceOutlineKey,
    group: Vec<&AirspaceFeature>,
    version_label: &str,
    seen: &mut BTreeSet<String>,
) -> Option<AirspaceFeature> {
    let rings = outline_rings_for_features(&group);
    if rings.is_empty() {
        return None;
    }
    let bbox = parts_bbox(&rings)?;
    let representative = group.first()?;
    let anchor = polygon_label_anchor(&rings);
    let id = dedup_airspace_id(
        seen,
        &format!(
            "airspace:{}:outline:{}:{}:{}",
            version_label, key.class, key.ident, key.local_type
        ),
    );
    let paths = rings
        .iter()
        .filter_map(|part| airspace_path_from_points(part, "boundary"))
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return None;
    }
    let mut source_properties = BTreeMap::new();
    source_properties.insert(
        "DERIVED_FROM".to_string(),
        "controlled_airspace_shelves".to_string(),
    );
    source_properties.insert(
        "OUTLINE_COMPONENT_COUNT".to_string(),
        group.len().to_string(),
    );
    Some(AirspaceFeature {
        schema_version: 1,
        id,
        kind: "airspace".to_string(),
        source: "derived_controlled_airspace_outline".to_string(),
        cycle: version_label.trim_start_matches("data_").to_string(),
        name: format!("{} OUTLINE", representative.name),
        ident: representative.ident.clone(),
        airspace_class: representative.airspace_class.clone(),
        local_type: format!("{}_OUTLINE", representative.local_type),
        style_hint: representative.style_hint.clone(),
        vertical_label: String::new(),
        vertical: AirspaceVertical {
            lower: AirspaceLimit {
                display: String::new(),
                feet: None,
                unit: None,
                reference: None,
                description: None,
            },
            upper: AirspaceLimit {
                display: String::new(),
                feet: None,
                unit: None,
                reference: None,
                description: None,
            },
        },
        bbox,
        label: AirspaceLabel {
            text: String::new(),
            lon: anchor[0],
            lat: anchor[1],
        },
        label_candidates: Vec::new(),
        paths,
        source_properties,
    })
}

fn is_controlled_airspace_detail(feature: &AirspaceFeature) -> bool {
    feature.source == "faa_nasr_class_airspace_shapefile"
        && matches!(
            feature.airspace_class.to_ascii_uppercase().as_str(),
            "B" | "C" | "D"
        )
}

fn controlled_airspace_uses_outline_at_zoom(feature: &AirspaceFeature, zoom: u8) -> bool {
    is_controlled_airspace_detail(feature) && zoom <= CONTROLLED_AIRSPACE_OUTLINE_MAX_ZOOM
}

fn airspace_ref_min_pixel_span(feature: &AirspaceFeature) -> f64 {
    if is_controlled_airspace_detail(feature) {
        CONTROLLED_AIRSPACE_DETAIL_MIN_PIXEL_SPAN
    } else {
        AIRSPACE_REF_MIN_PIXEL_SPAN
    }
}

fn airspace_ref_is_available_at_zoom(feature: &AirspaceFeature, zoom: u8) -> bool {
    !feature.airspace_class.eq_ignore_ascii_case("MOA") || zoom >= MOA_REF_MIN_ZOOM
}

fn outline_rings_for_features(features: &[&AirspaceFeature]) -> Vec<Vec<[f64; 2]>> {
    // Low zoom does not need shelf-level B/C/D geometry. Build a dissolved
    // outline from all shelves in the same airport/class group, then simplify
    // it. This gives the UI a cheap "outer airspace is here" boundary for
    // z0..z8; full shelves start materializing at z9.
    //
    // FAA shelf boundaries often share arcs with different vertex splits. Before
    // unioning, we snap to a tiny grid and expand each shelf by a low-zoom-only
    // epsilon so adjacent shelves definitely overlap. The expansion is at most
    // about 0.06 NM and is applied only to this derived overview outline, never
    // to the published shelf-level geometry.
    exterior_rings_from_geo_union(&geo_union_for_airspace_refs(features))
        .into_iter()
        .map(|ring| {
            simplify_closed_ring(
                &ring,
                CONTROLLED_AIRSPACE_OUTLINE_SIMPLIFY_TOLERANCE_DEGREES,
            )
        })
        .collect()
}

fn load_saa_airspace_features(
    path: &Path,
    version_label: &str,
) -> anyhow::Result<SaaAirspaceLoadResult> {
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
    let mut source_xml_count = 0usize;
    let mut seen = BTreeSet::new();
    for index in 0..inner.len() {
        let mut file = inner.by_index(index)?;
        if !file.name().to_ascii_lowercase().ends_with(".xml") {
            continue;
        }
        source_xml_count += 1;
        let mut xml = String::new();
        file.read_to_string(&mut xml)?;
        if let Some(feature) = parse_saa_xml(&xml, file.name(), version_label, &mut seen)? {
            features.push(feature);
        }
    }
    Ok(SaaAirspaceLoadResult {
        source_xml_count,
        features,
    })
}

struct SaaAirspaceLoadResult {
    source_xml_count: usize,
    features: Vec<AirspaceFeature>,
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
    paths: Vec<SaaPath>,
    current_component_operation: Option<String>,
    current_ring_points: Option<Vec<[f64; 2]>>,
    in_line_string_segment: bool,
    current_line_points: Vec<[f64; 2]>,
    in_arc_by_center_point: bool,
    arc_center: Option<[f64; 2]>,
    arc_radius_unit: Option<String>,
    arc_radius_value: Option<String>,
    arc_start_angle: Option<f64>,
    arc_end_angle: Option<f64>,
    circle_center: Option<[f64; 2]>,
    circle_radius_unit: Option<String>,
}

struct SaaPath {
    points: Vec<[f64; 2]>,
    operation: Option<String>,
}

impl SaaParseState {
    fn push_path(&mut self, points: Vec<[f64; 2]>) {
        self.paths.push(SaaPath {
            points,
            operation: self.current_component_operation.clone(),
        });
    }
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
                    if name == "AirspaceGeometryComponent" {
                        state.current_component_operation = None;
                    }
                    if name == "CircleByCenterPoint" {
                        state.in_circle_by_center_point = true;
                        state.circle_center = None;
                        state.circle_radius_unit = None;
                    }
                    if name == "Ring" || name == "LinearRing" {
                        state.current_ring_points = Some(Vec::new());
                    }
                    if name == "LineStringSegment" {
                        state.in_line_string_segment = true;
                        state.current_line_points.clear();
                    }
                    if name == "ArcByCenterPoint" {
                        state.in_arc_by_center_point = true;
                        state.arc_center = None;
                        state.arc_radius_unit = None;
                        state.arc_radius_value = None;
                        state.arc_start_angle = None;
                        state.arc_end_angle = None;
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
                        let unit = event
                            .attributes()
                            .flatten()
                            .find(|attr| attr.key.as_ref() == b"uom")
                            .map(|attr| String::from_utf8_lossy(&attr.value).into_owned());
                        if state.in_circle_by_center_point {
                            state.circle_radius_unit = unit.clone();
                        }
                        if state.in_arc_by_center_point {
                            state.arc_radius_unit = unit;
                        }
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
                            Some("operation") => {
                                state.current_component_operation = Some(text);
                            }
                            Some("pos") => {
                                if let Some(point) = parse_aixm_pos(&text) {
                                    if state.in_circle_by_center_point {
                                        state.circle_center = Some(point);
                                    } else if state.in_arc_by_center_point {
                                        state.arc_center = Some(point);
                                    } else if state.in_line_string_segment {
                                        state.current_line_points.push(point);
                                    } else if let Some(ring) = state.current_ring_points.as_mut() {
                                        // Some SAA AIXM polygons use GML LinearRing with direct
                                        // pos children instead of LineStringSegment/ArcByCenterPoint.
                                        // Only accept bare positions while a boundary ring is open;
                                        // AIXM positions outside that context can be semantic refs
                                        // such as arc centers, navaids, or communications metadata.
                                        append_path_points(ring, &[point]);
                                    } else {
                                        // AIXM positions appear in several semantic contexts. Only
                                        // explicit boundary segments should become drawable vertices.
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
                                            state.push_path(path);
                                        }
                                    }
                                } else if state.in_arc_by_center_point {
                                    state.arc_radius_value = Some(text);
                                }
                            }
                            Some("startAngle") => {
                                if state.in_arc_by_center_point {
                                    state.arc_start_angle = text.parse::<f64>().ok();
                                }
                            }
                            Some("endAngle") => {
                                if state.in_arc_by_center_point {
                                    state.arc_end_angle = text.parse::<f64>().ok();
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
                if name == "LineStringSegment" {
                    state.in_line_string_segment = false;
                    if let Some(ring) = state.current_ring_points.as_mut() {
                        append_path_points(ring, &state.current_line_points);
                    } else if state.current_line_points.len() >= 2 {
                        state.push_path(state.current_line_points.clone());
                    }
                    state.current_line_points.clear();
                }
                if name == "ArcByCenterPoint" {
                    state.in_arc_by_center_point = false;
                    if let (Some(center), Some(radius), Some(start_angle), Some(end_angle)) = (
                        state.arc_center,
                        state.arc_radius_value.as_deref(),
                        state.arc_start_angle,
                        state.arc_end_angle,
                    ) {
                        if let Some(points) = approximate_aixm_arc(
                            center,
                            radius,
                            state.arc_radius_unit.as_deref(),
                            start_angle,
                            end_angle,
                        ) {
                            if let Some(ring) = state.current_ring_points.as_mut() {
                                append_path_points(ring, &points);
                            } else {
                                state.push_path(points);
                            }
                        }
                    }
                    state.arc_center = None;
                    state.arc_radius_unit = None;
                    state.arc_radius_value = None;
                    state.arc_start_angle = None;
                    state.arc_end_angle = None;
                }
                if name == "Ring" || name == "LinearRing" {
                    if let Some(mut points) = state.current_ring_points.take() {
                        if points.len() >= 2 {
                            if points.first() != points.last() {
                                if let Some(first) = points.first().copied() {
                                    points.push(first);
                                }
                            }
                            state.push_path(points);
                        }
                    }
                }
                if name == "AirspaceGeometryComponent" {
                    state.current_component_operation = None;
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
    let all_points = state
        .paths
        .iter()
        .flat_map(|path| path.points.iter())
        .copied()
        .collect::<Vec<_>>();
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
        .filter_map(|path| saa_airspace_path(path, "boundary"))
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return Ok(None);
    }
    let raw_paths = state
        .paths
        .iter()
        .map(|path| path.points.clone())
        .collect::<Vec<_>>();
    let anchor = polygon_label_anchor(&raw_paths);
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
        label_candidates: vec![AirspaceLabelCandidate {
            rank: 0,
            score: 0.0,
            lon: anchor[0],
            lat: anchor[1],
        }],
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

fn insert_airspace_tile_label(labels: &mut Vec<AirspaceTileLabel>, candidate: AirspaceTileLabel) {
    if let Some(existing) = labels
        .iter_mut()
        .find(|label| label.feature_id == candidate.feature_id)
    {
        if candidate.rank < existing.rank {
            *existing = candidate;
        }
    } else {
        labels.push(candidate);
    }
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

fn assign_class_airspace_label_candidates(
    features: &mut [AirspaceFeature],
    airport_points: &[PointRecord],
    diagnostics: &mut AirspaceLabelDiagnostics,
) {
    let groups = airspace_sibling_groups(features);
    for group in groups {
        for &feature_index in &group {
            let current_parts = feature_parts(&features[feature_index]);
            let inner_parts = group
                .iter()
                .copied()
                .filter(|&other_index| other_index != feature_index)
                .filter(|&other_index| {
                    vertical_ranges_overlap(
                        &features[feature_index].vertical,
                        &features[other_index].vertical,
                    )
                })
                .filter_map(|other_index| {
                    let other_parts = feature_parts(&features[other_index]);
                    polygon_contains_polygon_by_sampling(&current_parts, &other_parts)
                        .then_some(other_parts)
                })
                .collect::<Vec<_>>();
            let nearby_airports = nearby_airport_points(&current_parts, airport_points);
            let candidates =
                ranked_label_candidates(&current_parts, &inner_parts, &nearby_airports);
            if candidates.is_empty() {
                diagnostics.class_label_anchor_outside_polygon_count += 1;
                continue;
            }
            features[feature_index].label.lon = candidates[0].lon;
            features[feature_index].label.lat = candidates[0].lat;
            features[feature_index].label_candidates = candidates;
        }
    }
}

fn vertical_ranges_overlap(left: &AirspaceVertical, right: &AirspaceVertical) -> bool {
    let Some(left_lower) = left.lower.feet else {
        return true;
    };
    let Some(left_upper) = left.upper.feet else {
        return true;
    };
    let Some(right_lower) = right.lower.feet else {
        return true;
    };
    let Some(right_upper) = right.upper.feet else {
        return true;
    };
    left_lower < right_upper && right_lower < left_upper
}

fn airspace_sibling_groups(features: &[AirspaceFeature]) -> Vec<Vec<usize>> {
    let mut groups = BTreeMap::<(String, Option<String>, String), Vec<usize>>::new();
    for (index, feature) in features.iter().enumerate() {
        groups
            .entry((
                feature.airspace_class.to_ascii_uppercase(),
                feature
                    .ident
                    .as_ref()
                    .map(|value| value.to_ascii_uppercase()),
                feature.local_type.to_ascii_uppercase(),
            ))
            .or_default()
            .push(index);
    }
    groups.into_values().collect()
}

fn feature_parts(feature: &AirspaceFeature) -> Vec<Vec<[f64; 2]>> {
    feature
        .paths
        .iter()
        .filter(|path| path.role == "boundary")
        .map(|path| path.points.clone())
        .collect()
}

fn ranked_label_candidates(
    parts: &[Vec<[f64; 2]>],
    inner_parts: &[Vec<Vec<[f64; 2]>>],
    airport_points: &[[f64; 2]],
) -> Vec<AirspaceLabelCandidate> {
    let mut candidates = sampled_label_points(parts)
        .into_iter()
        .filter(|candidate| {
            !inner_parts
                .iter()
                .any(|inner| point_in_polygon_parts(*candidate, inner))
        })
        .map(|point| AirspaceLabelCandidate {
            rank: 0,
            score: label_candidate_score(point, parts, inner_parts, airport_points),
            lon: point[0],
            lat: point[1],
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.dedup_by(|left, right| {
        (left.lon - right.lon).abs() < 1.0e-10 && (left.lat - right.lat).abs() < 1.0e-10
    });
    rerank_label_candidates(&mut candidates);
    candidates
}

fn rerank_label_candidates(candidates: &mut [AirspaceLabelCandidate]) {
    for (rank, candidate) in candidates.iter_mut().enumerate() {
        candidate.rank = rank as u32;
    }
}

fn sampled_label_points(parts: &[Vec<[f64; 2]>]) -> Vec<[f64; 2]> {
    let Some(bbox) = parts_bbox(parts) else {
        return Vec::new();
    };
    let lon_step = (bbox[2] - bbox[0]) / AIRSPACE_LABEL_SAMPLE_GRID as f64;
    let lat_step = (bbox[3] - bbox[1]) / AIRSPACE_LABEL_SAMPLE_GRID as f64;
    if lon_step <= 0.0 || lat_step <= 0.0 {
        return Vec::new();
    }

    let mut points = Vec::new();
    for x_index in 0..AIRSPACE_LABEL_SAMPLE_GRID {
        for y_index in 0..AIRSPACE_LABEL_SAMPLE_GRID {
            let candidate = [
                bbox[0] + (x_index as f64 + 0.5) * lon_step,
                bbox[1] + (y_index as f64 + 0.5) * lat_step,
            ];
            if point_in_polygon_parts(candidate, parts) {
                points.push(candidate);
            }
        }
    }
    if let Some(centroid) = polygon_area_centroid(parts) {
        if point_in_polygon_parts(centroid, parts) {
            points.push(centroid);
        }
    }
    points
}

fn label_candidate_score(
    point: [f64; 2],
    parts: &[Vec<[f64; 2]>],
    inner_parts: &[Vec<Vec<[f64; 2]>>],
    airport_points: &[[f64; 2]],
) -> f64 {
    let mut score = squared_distance_to_nearest_boundary(point, parts);
    for inner in inner_parts {
        score = score.min(squared_distance_to_nearest_boundary(point, inner));
    }
    for airport in airport_points {
        score = score.min(squared_distance(point, *airport));
    }
    score.sqrt()
}

fn nearby_airport_points(parts: &[Vec<[f64; 2]>], airport_points: &[PointRecord]) -> Vec<[f64; 2]> {
    let Some([west, south, east, north]) = parts_bbox(parts) else {
        return Vec::new();
    };
    let margin = 0.25;
    airport_points
        .iter()
        .filter(|airport| {
            airport.lon >= west - margin
                && airport.lon <= east + margin
                && airport.lat >= south - margin
                && airport.lat <= north + margin
        })
        .map(|airport| [airport.lon, airport.lat])
        .collect()
}

fn polygon_contains_polygon_by_sampling(
    outer_parts: &[Vec<[f64; 2]>],
    inner_parts: &[Vec<[f64; 2]>],
) -> bool {
    let samples = sampled_label_points(inner_parts);
    if samples.is_empty() {
        return false;
    }
    let inside_count = samples
        .iter()
        .filter(|sample| point_in_polygon_parts(**sample, outer_parts))
        .count();
    (inside_count as f64 / samples.len() as f64) >= AIRSPACE_LABEL_CONTAINMENT_RATIO
}

fn simplify_closed_ring(points: &[[f64; 2]], tolerance: f64) -> Vec<[f64; 2]> {
    if points.len() <= 4 {
        return points.to_vec();
    }
    let mut open = points.to_vec();
    if open.first() == open.last() {
        open.pop();
    }
    if open.len() <= 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; open.len()];
    keep[0] = true;
    keep[open.len() - 1] = true;
    rdp_mark_keep(&open, 0, open.len() - 1, tolerance, &mut keep);
    let mut simplified = open
        .iter()
        .zip(keep.iter())
        .filter_map(|(point, keep)| keep.then_some(*point))
        .collect::<Vec<_>>();
    if simplified.len() < 3 {
        return points.to_vec();
    }
    if let Some(first) = simplified.first().copied() {
        simplified.push(first);
    }
    simplified
}

fn rdp_mark_keep(points: &[[f64; 2]], start: usize, end: usize, tolerance: f64, keep: &mut [bool]) {
    if end <= start + 1 {
        return;
    }
    let mut max_distance = 0.0;
    let mut max_index = start;
    for index in start + 1..end {
        let distance = point_segment_distance(points[index], points[start], points[end]);
        if distance > max_distance {
            max_distance = distance;
            max_index = index;
        }
    }
    if max_distance > tolerance {
        keep[max_index] = true;
        rdp_mark_keep(points, start, max_index, tolerance, keep);
        rdp_mark_keep(points, max_index, end, tolerance, keep);
    }
}

fn point_segment_distance(point: [f64; 2], start: [f64; 2], end: [f64; 2]) -> f64 {
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let length_squared = dx * dx + dy * dy;
    if length_squared <= f64::EPSILON {
        return squared_distance(point, start).sqrt();
    }
    let t = (((point[0] - start[0]) * dx + (point[1] - start[1]) * dy) / length_squared)
        .clamp(0.0, 1.0);
    let projected = [start[0] + t * dx, start[1] + t * dy];
    squared_distance(point, projected).sqrt()
}

fn label_candidate_has_airspace_edge_clearance(
    score_degrees: f64,
    latitude: f64,
    zoom: u8,
) -> bool {
    score_degrees * local_pixels_per_degree(latitude, zoom) >= AIRSPACE_LABEL_MIN_EDGE_CLEARANCE_PX
}

fn local_pixels_per_degree(latitude: f64, zoom: u8) -> f64 {
    let lon_scale = 256.0 * 2_f64.powi(zoom as i32) / 360.0;
    let lat_scale = lon_scale / latitude.to_radians().cos().abs().max(0.001);
    lon_scale.max(lat_scale)
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
    let (lon_radius_degrees, lat_radius_degrees) =
        aixm_radius_degrees(center, radius_value, radius_unit)?;
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

fn approximate_aixm_arc(
    center: [f64; 2],
    radius_value: &str,
    radius_unit: Option<&str>,
    start_angle_deg: f64,
    end_angle_deg: f64,
) -> Option<Vec<[f64; 2]>> {
    let (lon_radius_degrees, lat_radius_degrees) =
        aixm_radius_degrees(center, radius_value, radius_unit)?;
    let sweep_deg = directed_aixm_arc_sweep_degrees(start_angle_deg, end_angle_deg);
    let sample_count = ((sweep_deg.abs() / 5.0).ceil() as usize).clamp(4, 96);
    let mut points = Vec::with_capacity(sample_count + 1);
    for index in 0..=sample_count {
        let fraction = index as f64 / sample_count as f64;
        let angle = (start_angle_deg + sweep_deg * fraction).to_radians();
        let lon = round_coord(center[0] + lon_radius_degrees * angle.cos());
        let lat = round_coord(center[1] + lat_radius_degrees * angle.sin());
        if valid_lat_lon(lat, lon) {
            points.push([lon, lat]);
        }
    }
    (points.len() > 1).then_some(points)
}

fn aixm_radius_degrees(
    center: [f64; 2],
    radius_value: &str,
    radius_unit: Option<&str>,
) -> Option<(f64, f64)> {
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
        "MI" => radius * 5280.0 / 6076.11549,
        "M" => radius / 1852.0,
        "KM" => radius / 1.852,
        "FT" => radius / 6076.11549,
        _ => return None,
    };
    let lat_radius_degrees = radius_nm / 60.0;
    let cos_lat = center[1].to_radians().cos().abs().max(0.000_001);
    let lon_radius_degrees = lat_radius_degrees / cos_lat;
    Some((lon_radius_degrees, lat_radius_degrees))
}

fn directed_aixm_arc_sweep_degrees(start_angle: f64, end_angle: f64) -> f64 {
    // GML ArcByCenterPoint arcs are directed from startAngle to endAngle; they
    // are not implicitly normalized to the shortest arc. AIXM applies the same
    // convention, with CRS84 angle values increasing counter-clockwise.
    //
    // References:
    // - GML 3.2.1 ArcByCenterPoint: center, radius, start bearing, end bearing.
    //   https://repository.data2type.de/GML/v_3.2.1/html/el.ArcByCenterPoint.html
    // - AIXM coding guidance: arc direction follows increasing values when
    //   start < end and decreasing values when start > end.
    //   https://swim-eurocontrol.atlassian.net/wiki/spaces/ACG/pages/212239935/ArcByCenterPoint+Interpretation+Summary
    end_angle - start_angle
}

fn append_path_points(target: &mut Vec<[f64; 2]>, points: &[[f64; 2]]) {
    for point in points {
        if target.last() == Some(point) {
            continue;
        }
        target.push(*point);
    }
}

fn airspace_path_from_points(points: &[[f64; 2]], role: &str) -> Option<AirspacePath> {
    let _ = points.first()?;
    Some(AirspacePath {
        role: role.to_string(),
        closed: points.first() == points.last(),
        interior_side: None,
        points: points.to_vec(),
    })
}

fn saa_airspace_path(path: &SaaPath, role: &str) -> Option<AirspacePath> {
    let _ = path.points.first()?;
    Some(AirspacePath {
        role: role.to_string(),
        closed: path.points.first() == path.points.last(),
        interior_side: saa_path_interior_side(path),
        points: path.points.clone(),
    })
}

fn saa_path_interior_side(path: &SaaPath) -> Option<String> {
    let mut side = path_winding_interior_side(&path.points)?;
    if path
        .operation
        .as_deref()
        .is_some_and(|operation| operation.eq_ignore_ascii_case("SUBTR"))
    {
        side = opposite_side(side);
    }
    Some(side.to_string())
}

fn path_winding_interior_side(points: &[[f64; 2]]) -> Option<&'static str> {
    let signed_area = signed_ring_area(points)?;
    // Do not infer SAA holes from ring containment. FAA SAA AIXM uses
    // AirspaceGeometryComponent.operation (BASE/UNION/SUBTR) to define
    // aggregation; nested components may be subtractions or independent
    // contributors. OGC 12-028r1 "Use of Geography Markup Language (GML) for
    // Aviation Data", sections 8.2.6 and 9, says AIXM v5 airspace holes are
    // encoded as AirspaceVolume subtractions, and section 9 specifically calls
    // out operation/operationSequence as required for correctly representing
    // aggregation. The FAA-bundled AIXM_Features.xsd says operation indicates
    // whether a component participates by addition/subtraction/intersection.
    //
    // Once operation is known, winding still tells us the component polygon's
    // local interior side: in x=longitude, y=latitude coordinates, a
    // counter-clockwise ring has interior on the left while traversing the
    // path, and a clockwise ring has interior on the right. SUBTR components
    // flip that side because the represented airspace is outside the component.
    Some(if signed_area > 0.0 { "left" } else { "right" })
}

fn opposite_side(side: &str) -> &'static str {
    match side {
        "left" => "right",
        "right" => "left",
        _ => "left",
    }
}

fn signed_ring_area(points: &[[f64; 2]]) -> Option<f64> {
    if points.len() < 3 {
        return None;
    }
    let mut twice_area = 0.0;
    for index in 0..points.len() {
        let current = points[index];
        let next = points[(index + 1) % points.len()];
        twice_area += current[0] * next[1] - next[0] * current[1];
    }
    if twice_area.abs() < 1.0e-12 {
        return None;
    }
    Some(twice_area / 2.0)
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

fn write_airspace_feature(
    output_dir: &Path,
    feature: &AirspaceFeature,
    zip_members: &mut Vec<(String, PathBuf)>,
) -> anyhow::Result<()> {
    let relative_path = airspace_feature_relative_path(&feature.id);
    let feature_path = output_dir.join(&relative_path);
    write_json_compact(&feature_path, feature)?;
    zip_members.push((relative_path, feature_path));
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saa_aixm_parser_does_not_turn_arc_centers_into_boundary_vertices() {
        let xml = r#"
            <SaaMessage>
              <hasMember>
                <Airspace>
                  <timeSlice>
                    <AirspaceTimeSlice>
                      <designator>A381</designator>
                      <name>A-381 TEST</name>
                      <suaType>AA</suaType>
                      <geometryComponent>
                        <AirspaceGeometryComponent>
                          <theAirspaceVolume>
                            <AirspaceVolume>
                              <upperLimit uom="FT">4000</upperLimit>
                              <upperLimitReference>MSL</upperLimitReference>
                              <lowerLimit uom="FT">0</lowerLimit>
                              <lowerLimitReference>SFC</lowerLimitReference>
                              <horizontalProjection>
                                <Surface>
                                  <patches>
                                    <PolygonPatch>
                                      <exterior>
                                        <Ring>
                                          <curveMember>
                                            <Curve>
                                              <segments>
                                                <LineStringSegment>
                                                  <pos>-90.803056 29.621667</pos>
                                                  <pos>-90.773889 29.660833</pos>
                                                </LineStringSegment>
                                                <ArcByCenterPoint>
                                                  <pointProperty>
                                                    <Point>
                                                      <pos>-90.660556 29.567778</pos>
                                                    </Point>
                                                  </pointProperty>
                                                  <radius uom="NM">4.0</radius>
                                                  <startAngle uom="deg">125.9</startAngle>
                                                  <endAngle uom="deg">166.7228172</endAngle>
                                                </ArcByCenterPoint>
                                                <LineStringSegment>
                                                  <pos>-90.735 29.583056</pos>
                                                  <pos>-90.803056 29.621667</pos>
                                                </LineStringSegment>
                                              </segments>
                                            </Curve>
                                          </curveMember>
                                        </Ring>
                                      </exterior>
                                    </PolygonPatch>
                                  </patches>
                                </Surface>
                              </horizontalProjection>
                            </AirspaceVolume>
                          </theAirspaceVolume>
                        </AirspaceGeometryComponent>
                      </geometryComponent>
                    </AirspaceTimeSlice>
                  </timeSlice>
                </Airspace>
              </hasMember>
            </SaaMessage>
        "#;
        let mut seen = BTreeSet::new();
        let feature = parse_saa_xml(xml, "A-381 TEST.xml", "data_2604", &mut seen)
            .expect("parse should succeed")
            .expect("feature should be emitted");

        assert_eq!(feature.paths.len(), 1);
        let points = &feature.paths[0].points;
        assert!(!points.contains(&[-90.660556, 29.567778]));
        assert!(
            points.len() > 4,
            "arc should be approximated into boundary points"
        );
        assert_eq!(points.first(), points.last());
    }

    #[test]
    fn saa_aixm_parser_preserves_directed_arc_sweeps() {
        let xml = r#"
            <SaaMessage>
              <hasMember>
                <Airspace>
                  <timeSlice>
                    <AirspaceTimeSlice>
                      <designator>R6612</designator>
                      <name>R-6612 DAHLGREN COMPLEX, VA</name>
                      <suaType>RA</suaType>
                      <geometryComponent>
                        <AirspaceGeometryComponent>
                          <theAirspaceVolume>
                            <AirspaceVolume>
                              <upperLimit uom="FT">07000</upperLimit>
                              <upperLimitReference>MSL</upperLimitReference>
                              <lowerLimit uom="FT">GND</lowerLimit>
                              <lowerLimitReference>SFC</lowerLimitReference>
                              <horizontalProjection>
                                <Surface srsName="URN:OGC:DEF:CRS:OGC:1.3:CRS84">
                                  <patches>
                                    <PolygonPatch>
                                      <exterior>
                                        <Ring>
                                          <curveMember>
                                            <Curve>
                                              <segments>
                                                <ArcByCenterPoint>
                                                  <pointProperty>
                                                    <Point>
                                                      <pos>-77.036944 38.299722</pos>
                                                    </Point>
                                                  </pointProperty>
                                                  <radius uom="FT">7000.0</radius>
                                                  <startAngle uom="deg">71.2</startAngle>
                                                  <endAngle uom="deg">-142.9943029</endAngle>
                                                </ArcByCenterPoint>
                                              </segments>
                                            </Curve>
                                          </curveMember>
                                          <curveMember>
                                            <Curve>
                                              <segments>
                                                <ArcByCenterPoint>
                                                  <pointProperty>
                                                    <Point>
                                                      <pos>-77.048611 38.306389</pos>
                                                    </Point>
                                                  </pointProperty>
                                                  <radius uom="FT">7000.0</radius>
                                                  <startAngle uom="deg">251.2</startAngle>
                                                  <endAngle uom="deg">36.7403182</endAngle>
                                                </ArcByCenterPoint>
                                              </segments>
                                            </Curve>
                                          </curveMember>
                                        </Ring>
                                      </exterior>
                                    </PolygonPatch>
                                  </patches>
                                </Surface>
                              </horizontalProjection>
                            </AirspaceVolume>
                          </theAirspaceVolume>
                        </AirspaceGeometryComponent>
                      </geometryComponent>
                    </AirspaceTimeSlice>
                  </timeSlice>
                </Airspace>
              </hasMember>
            </SaaMessage>
        "#;
        let mut seen = BTreeSet::new();
        let feature = parse_saa_xml(xml, "R-6612 TEST.xml", "data_2604", &mut seen)
            .expect("parse should succeed")
            .expect("feature should be emitted");

        assert_eq!(feature.paths.len(), 1);
        assert!(
            feature.bbox[0] < -77.07,
            "west edge should include the outer circle"
        );
        assert!(
            feature.bbox[1] < 38.281,
            "south edge should include the outer circle"
        );
        assert!(
            feature.bbox[2] > -77.013,
            "east edge should include the outer circle"
        );
        assert!(
            feature.bbox[3] > 38.325,
            "north edge should include the outer circle"
        );
    }

    #[test]
    fn saa_aixm_parser_accepts_linear_ring_positions() {
        let xml = r#"
            <SaaMessage>
              <hasMember>
                <Airspace>
                  <timeSlice>
                    <AirspaceTimeSlice>
                      <designator>R6608A</designator>
                      <name>R-6608A QUANTICO, VA</name>
                      <suaType>RA</suaType>
                      <geometryComponent>
                        <AirspaceGeometryComponent>
                          <theAirspaceVolume>
                            <AirspaceVolume>
                              <upperLimit uom="FT">10000</upperLimit>
                              <upperLimitReference>MSL</upperLimitReference>
                              <lowerLimit uom="FT">GND</lowerLimit>
                              <lowerLimitReference>SFC</lowerLimitReference>
                              <horizontalProjection>
                                <Surface srsName="URN:OGC:DEF:CRS:OGC:1.3:CRS84">
                                  <patches>
                                    <PolygonPatch>
                                      <exterior>
                                        <LinearRing>
                                          <pos>-77.568333 38.586111</pos>
                                          <pos>-77.568333 38.616667</pos>
                                          <pos>-77.538611 38.630556</pos>
                                          <pos>-77.462222 38.621389</pos>
                                          <pos>-77.462222 38.593056</pos>
                                          <pos>-77.568333 38.586111</pos>
                                        </LinearRing>
                                      </exterior>
                                    </PolygonPatch>
                                  </patches>
                                </Surface>
                              </horizontalProjection>
                            </AirspaceVolume>
                          </theAirspaceVolume>
                        </AirspaceGeometryComponent>
                      </geometryComponent>
                    </AirspaceTimeSlice>
                  </timeSlice>
                </Airspace>
              </hasMember>
            </SaaMessage>
        "#;
        let mut seen = BTreeSet::new();
        let feature = parse_saa_xml(xml, "R-6608A TEST.xml", "data_2604", &mut seen)
            .expect("parse should succeed")
            .expect("feature should be emitted");

        assert_eq!(feature.name, "R-6608A QUANTICO, VA");
        assert_eq!(feature.ident.as_deref(), Some("R6608A"));
        assert_eq!(feature.airspace_class, "RA");
        assert_eq!(feature.paths.len(), 1);
        assert_eq!(feature.paths[0].points.len(), 6);
        assert_eq!(
            feature.paths[0].points.first(),
            feature.paths[0].points.last()
        );
    }

    #[test]
    fn saa_aixm_parser_accepts_statute_mile_circle_radius() {
        let xml = r#"
            <SaaMessage>
              <hasMember>
                <Airspace>
                  <timeSlice>
                    <AirspaceTimeSlice>
                      <designator>R6317</designator>
                      <name>R-6317 EL SAUZ, TX</name>
                      <suaType>RA</suaType>
                      <geometryComponent>
                        <AirspaceGeometryComponent>
                          <theAirspaceVolume>
                            <AirspaceVolume>
                              <upperLimit uom="FT">15000</upperLimit>
                              <upperLimitReference>MSL</upperLimitReference>
                              <lowerLimit uom="FT">GND</lowerLimit>
                              <lowerLimitReference>SFC</lowerLimitReference>
                              <horizontalProjection>
                                <Surface srsName="URN:OGC:DEF:CRS:OGC:1.3:CRS84">
                                  <patches>
                                    <PolygonPatch>
                                      <exterior>
                                        <Ring>
                                          <curveMember>
                                            <Curve>
                                              <segments>
                                                <CircleByCenterPoint>
                                                  <pointProperty>
                                                    <Point>
                                                      <pos>-98.816667 26.572222</pos>
                                                    </Point>
                                                  </pointProperty>
                                                  <radius uom="MI">3.0</radius>
                                                </CircleByCenterPoint>
                                              </segments>
                                            </Curve>
                                          </curveMember>
                                        </Ring>
                                      </exterior>
                                    </PolygonPatch>
                                  </patches>
                                </Surface>
                              </horizontalProjection>
                            </AirspaceVolume>
                          </theAirspaceVolume>
                        </AirspaceGeometryComponent>
                      </geometryComponent>
                    </AirspaceTimeSlice>
                  </timeSlice>
                </Airspace>
              </hasMember>
            </SaaMessage>
        "#;
        let mut seen = BTreeSet::new();
        let feature = parse_saa_xml(xml, "R-6317 TEST.xml", "data_2604", &mut seen)
            .expect("parse should succeed")
            .expect("feature should be emitted");

        assert_eq!(feature.name, "R-6317 EL SAUZ, TX");
        assert_eq!(feature.airspace_class, "RA");
        assert_eq!(feature.paths.len(), 1);
        assert!(feature.paths[0].points.len() > 90);
        assert_eq!(
            feature.paths[0].points.first(),
            feature.paths[0].points.last()
        );
    }

    #[test]
    fn saa_aixm_parser_sets_interior_side_from_operation_and_winding() {
        let xml = r#"
            <SaaMessage>
              <hasMember>
                <Airspace>
                  <timeSlice>
                    <AirspaceTimeSlice>
                      <designator>TST1</designator>
                      <name>TEST SAA</name>
                      <suaType>MOA</suaType>
                      <geometryComponent>
                        <AirspaceGeometryComponent>
                          <operation>BASE</operation>
                          <operationSequence>0</operationSequence>
                          <theAirspaceVolume>
                            <AirspaceVolume>
                              <upperLimit uom="FT">10000</upperLimit>
                              <upperLimitReference>MSL</upperLimitReference>
                              <lowerLimit uom="FT">GND</lowerLimit>
                              <lowerLimitReference>SFC</lowerLimitReference>
                              <horizontalProjection>
                                <Surface>
                                  <patches>
                                    <PolygonPatch>
                                      <exterior>
                                        <LinearRing>
                                          <pos>-100.0 40.0</pos>
                                          <pos>-100.0 41.0</pos>
                                          <pos>-99.0 41.0</pos>
                                          <pos>-99.0 40.0</pos>
                                          <pos>-100.0 40.0</pos>
                                        </LinearRing>
                                      </exterior>
                                    </PolygonPatch>
                                  </patches>
                                </Surface>
                              </horizontalProjection>
                            </AirspaceVolume>
                          </theAirspaceVolume>
                        </AirspaceGeometryComponent>
                      </geometryComponent>
                      <geometryComponent>
                        <AirspaceGeometryComponent>
                          <operation>SUBTR</operation>
                          <operationSequence>1</operationSequence>
                          <theAirspaceVolume>
                            <AirspaceVolume>
                              <upperLimit uom="FT">10000</upperLimit>
                              <upperLimitReference>MSL</upperLimitReference>
                              <lowerLimit uom="FT">GND</lowerLimit>
                              <lowerLimitReference>SFC</lowerLimitReference>
                              <horizontalProjection>
                                <Surface>
                                  <patches>
                                    <PolygonPatch>
                                      <exterior>
                                        <LinearRing>
                                          <pos>-99.8 40.2</pos>
                                          <pos>-99.8 40.4</pos>
                                          <pos>-99.6 40.4</pos>
                                          <pos>-99.6 40.2</pos>
                                          <pos>-99.8 40.2</pos>
                                        </LinearRing>
                                      </exterior>
                                    </PolygonPatch>
                                  </patches>
                                </Surface>
                              </horizontalProjection>
                            </AirspaceVolume>
                          </theAirspaceVolume>
                        </AirspaceGeometryComponent>
                      </geometryComponent>
                    </AirspaceTimeSlice>
                  </timeSlice>
                </Airspace>
              </hasMember>
            </SaaMessage>
        "#;
        let mut seen = BTreeSet::new();
        let feature = parse_saa_xml(xml, "TEST SAA.xml", "data_2604", &mut seen)
            .expect("parse should succeed")
            .expect("feature should be emitted");

        assert_eq!(feature.paths.len(), 2);
        assert_eq!(feature.paths[0].interior_side.as_deref(), Some("right"));
        assert_eq!(
            feature.paths[1].interior_side.as_deref(),
            Some("left"),
            "SUBTR components invert the path winding interior"
        );
    }
}
