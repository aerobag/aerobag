use anyhow::{bail, Context};
use geo::{BooleanOps, Coord, LineString, MultiPolygon, Polygon};
use preprocessor_core::{nav_kv::NavKvRoot, Region, RegionBounds};
use preprocessor_vectors::{expanded_union_polygon_from_closed_ring, simplify_closed_ring};
use serde::Deserialize;
use std::{
    env, fs,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
use zip::ZipArchive;

const DEFAULT_TOLERANCE_DEG: f64 = 0.01;
const DEFAULT_SNAP_GRID_DEG: f64 = 0.0001;
const DEFAULT_EXPAND_DEG: f64 = 0.001;

#[derive(Debug, Deserialize)]
struct PolygonSetRecord {
    id: String,
    polygons: Vec<PolygonRecord>,
}

#[derive(Debug, Deserialize)]
struct PolygonRecord {
    points: Vec<[f64; 2]>,
}

#[derive(Clone)]
struct ProofShape {
    id: String,
    original_points: usize,
    union_points: usize,
    simplified_points: usize,
    original: Vec<Vec<[f64; 2]>>,
    simplified: Vec<Vec<[f64; 2]>>,
}

fn main() -> anyhow::Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.len() < 2 || args.len() > 6 {
        bail!(
            "usage: chart-coverage-proof <had-dir-or-zip> <output.svg> [--offline-regions] [simplify-tolerance-deg] [expand-deg] [snap-grid-deg]"
        );
    }
    let had = HadSource::open(Path::new(&args[0]))?;
    let output_path = PathBuf::from(&args[1]);
    let offline_regions = args.get(2).is_some_and(|arg| arg == "--offline-regions");
    let numeric_start = if offline_regions { 3 } else { 2 };
    let tolerance = args
        .get(numeric_start)
        .map(|raw| raw.parse::<f64>())
        .transpose()
        .context("invalid simplify tolerance")?
        .unwrap_or(DEFAULT_TOLERANCE_DEG);
    let expand = args
        .get(numeric_start + 1)
        .map(|raw| raw.parse::<f64>())
        .transpose()
        .context("invalid expand degrees")?
        .unwrap_or(DEFAULT_EXPAND_DEG);
    let snap_grid = args
        .get(numeric_start + 2)
        .map(|raw| raw.parse::<f64>())
        .transpose()
        .context("invalid snap grid degrees")?
        .unwrap_or(DEFAULT_SNAP_GRID_DEG);

    let mut shapes = if offline_regions {
        offline_region_shapes(&had, tolerance, snap_grid, expand)?
    } else {
        chart_coverage_shapes(&had, tolerance, snap_grid, expand)?
    };

    shapes.sort_by(|left, right| left.id.cmp(&right.id));
    write_svg(&output_path, &shapes, tolerance, snap_grid, expand)?;
    println!(
        "wrote {} ({} chart coverage sets, tolerance {tolerance} deg, expand {expand} deg, snap {snap_grid} deg)",
        output_path.display(),
        shapes.len()
    );
    for shape in shapes {
        println!(
            "{:24} original={:6} union={:6} simplified={:5}",
            shape.id, shape.original_points, shape.union_points, shape.simplified_points
        );
    }
    Ok(())
}

fn chart_coverage_shapes(
    had: &HadSource,
    tolerance: f64,
    snap_grid: f64,
    expand: f64,
) -> anyhow::Result<Vec<ProofShape>> {
    let mut shapes = Vec::new();
    for family in ["sec", "tac", "enr-l", "enr-h"] {
        for region in ["ak", "ec", "nc", "ne", "nw", "pac", "sc", "se", "sw"] {
            let set_id = format!("chart-coverage:{family}:{region}");
            let Some(record) = read_polygon_set(had, &set_id)? else {
                continue;
            };
            shapes.push(proof_shape(record, tolerance, snap_grid, expand, None));
        }
    }
    Ok(shapes)
}

fn offline_region_shapes(
    had: &HadSource,
    tolerance: f64,
    snap_grid: f64,
    expand: f64,
) -> anyhow::Result<Vec<ProofShape>> {
    let mut shapes = Vec::new();
    for region in ["ak", "ec", "nc", "ne", "nw", "pac", "sc", "se", "sw"] {
        let mut polygons = Vec::new();
        for family in ["sec", "tac", "enr-l", "enr-h"] {
            let set_id = format!("chart-coverage:{family}:{region}");
            let Some(record) = read_polygon_set(had, &set_id)? else {
                continue;
            };
            polygons.extend(record.polygons);
        }
        if polygons.is_empty() {
            continue;
        }
        let region_bounds = Region::from_code(&region.to_ascii_uppercase())
            .with_context(|| format!("unknown offline chart region {region}"))?
            .bounds_list();
        shapes.push(proof_shape(
            PolygonSetRecord {
                id: format!("offline-chart:{region}"),
                polygons,
            },
            tolerance,
            snap_grid,
            expand,
            Some(region_bounds),
        ));
    }
    Ok(shapes)
}

fn read_polygon_set(had: &HadSource, set_id: &str) -> anyhow::Result<Option<PolygonSetRecord>> {
    let Some(bytes) = had.query(&format!(
        "geometry/polygon-set/{}",
        had_key_component(set_id)
    ))?
    else {
        return Ok(None);
    };
    let record = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to decode polygon set {set_id}"))?;
    Ok(Some(record))
}

fn proof_shape(
    record: PolygonSetRecord,
    tolerance: f64,
    snap_grid: f64,
    expand: f64,
    clip_bounds: Option<&[RegionBounds]>,
) -> ProofShape {
    let mut union = MultiPolygon(vec![]);
    let mut original = Vec::new();
    let mut original_points = 0;
    for polygon in &record.polygons {
        if polygon.points.len() < 3 {
            continue;
        }
        original_points += polygon.points.len();
        original.push(polygon.points.clone());
        let Some(geo_polygon) =
            expanded_union_polygon_from_closed_ring(&polygon.points, snap_grid, expand)
        else {
            continue;
        };
        union = if union.0.is_empty() {
            MultiPolygon(vec![geo_polygon])
        } else {
            union.union(&geo_polygon)
        };
    }
    if let Some(bounds) = clip_bounds {
        union = union.intersection(&region_bounds_multi_polygon(bounds));
    }

    let union_points = multi_polygon_point_count(&union);
    let simplified = union
        .0
        .iter()
        .flat_map(|polygon| {
            let exterior = polygon
                .exterior()
                .0
                .iter()
                .map(|coord| [coord.x, coord.y])
                .collect::<Vec<_>>();
            let points = simplify_closed_ring(&exterior, tolerance);
            if points.len() >= 4 {
                Some(points)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let simplified_points = simplified.iter().map(Vec::len).sum();

    ProofShape {
        id: record.id,
        original_points,
        union_points,
        simplified_points,
        original,
        simplified,
    }
}

fn region_bounds_multi_polygon(bounds_list: &[RegionBounds]) -> MultiPolygon {
    MultiPolygon(
        bounds_list
            .iter()
            .map(|bounds| {
                Polygon::new(
                    LineString::new(vec![
                        Coord {
                            x: bounds.lon_min,
                            y: bounds.lat_max,
                        },
                        Coord {
                            x: bounds.lon_max,
                            y: bounds.lat_max,
                        },
                        Coord {
                            x: bounds.lon_max,
                            y: bounds.lat_min,
                        },
                        Coord {
                            x: bounds.lon_min,
                            y: bounds.lat_min,
                        },
                        Coord {
                            x: bounds.lon_min,
                            y: bounds.lat_max,
                        },
                    ]),
                    Vec::new(),
                )
            })
            .collect(),
    )
}

fn multi_polygon_point_count(polygons: &MultiPolygon) -> usize {
    polygons
        .0
        .iter()
        .map(|polygon| polygon.exterior().0.len())
        .sum()
}

fn write_svg(
    path: &Path,
    shapes: &[ProofShape],
    tolerance: f64,
    snap_grid: f64,
    expand: f64,
) -> anyhow::Result<()> {
    let panel_w = 360.0;
    let panel_h = 230.0;
    let cols = 4;
    let rows = shapes.len().div_ceil(cols);
    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n",
        panel_w * cols as f64,
        panel_h * rows as f64,
        panel_w * cols as f64,
        panel_h * rows as f64
    ));
    svg.push_str("<rect width=\"100%\" height=\"100%\" fill=\"#f8f8f6\"/>\n");
    svg.push_str(&format!(
        "<text x=\"16\" y=\"18\" font-family=\"monospace\" font-size=\"13\">Expanded union + Douglas-Peucker chart coverage. tolerance {tolerance}°, expand {expand}°, snap {snap_grid}°. Blue=original, red=simplified union.</text>\n"
    ));
    for (index, shape) in shapes.iter().enumerate() {
        draw_shape_panel(&mut svg, shape, index, cols, panel_w, panel_h);
    }
    svg.push_str("</svg>\n");
    fs::write(path, svg).with_context(|| format!("failed to write {}", path.display()))
}

fn draw_shape_panel(
    svg: &mut String,
    shape: &ProofShape,
    index: usize,
    cols: usize,
    panel_w: f64,
    panel_h: f64,
) {
    let col = index % cols;
    let row = index / cols;
    let ox = col as f64 * panel_w;
    let oy = row as f64 * panel_h + 14.0;
    let all_points = shape
        .original
        .iter()
        .chain(shape.simplified.iter())
        .flat_map(|polygon| polygon.iter().copied())
        .collect::<Vec<_>>();
    if all_points.is_empty() {
        return;
    }
    let min_lon = all_points
        .iter()
        .map(|point| point[0])
        .fold(f64::INFINITY, f64::min);
    let max_lon = all_points
        .iter()
        .map(|point| point[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let min_lat = all_points
        .iter()
        .map(|point| point[1])
        .fold(f64::INFINITY, f64::min);
    let max_lat = all_points
        .iter()
        .map(|point| point[1])
        .fold(f64::NEG_INFINITY, f64::max);
    let margin = 28.0;
    let scale = ((panel_w - 2.0 * margin) / (max_lon - min_lon).max(1.0e-9))
        .min((panel_h - 2.0 * margin - 22.0) / (max_lat - min_lat).max(1.0e-9));
    let project = |point: [f64; 2]| {
        (
            ox + margin + (point[0] - min_lon) * scale,
            oy + margin + 22.0 + (max_lat - point[1]) * scale,
        )
    };

    svg.push_str(&format!(
        "<g><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"white\" stroke=\"#ddd\"/>\n",
        ox + 8.0,
        oy + 8.0,
        panel_w - 16.0,
        panel_h - 16.0
    ));
    svg.push_str(&format!(
        "<text x=\"{}\" y=\"{}\" font-family=\"monospace\" font-size=\"12\" fill=\"#222\">{} {}/{}/{}</text>\n",
        ox + 14.0,
        oy + 25.0,
        escape_xml(&shape.id),
        shape.original_points,
        shape.union_points,
        shape.simplified_points
    ));
    for polygon in &shape.original {
        svg_path(svg, polygon, project, "none", "#1f77b4", 0.45, 0.55);
    }
    for polygon in &shape.simplified {
        svg_path(
            svg,
            polygon,
            project,
            "rgba(214,39,40,0.05)",
            "#d62728",
            1.6,
            0.9,
        );
    }
    svg.push_str("</g>\n");
}

fn svg_path<F>(
    svg: &mut String,
    points: &[[f64; 2]],
    project: F,
    fill: &str,
    stroke: &str,
    stroke_width: f64,
    opacity: f64,
) where
    F: Fn([f64; 2]) -> (f64, f64),
{
    if points.is_empty() {
        return;
    }
    let mut d = String::new();
    for (index, point) in points.iter().enumerate() {
        let (x, y) = project(*point);
        d.push_str(if index == 0 { "M" } else { "L" });
        d.push_str(&format!("{x:.2},{y:.2}"));
    }
    d.push('Z');
    svg.push_str(&format!(
        "<path d=\"{d}\" fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"{stroke_width}\" opacity=\"{opacity}\" stroke-linejoin=\"round\" stroke-linecap=\"round\"/>\n"
    ));
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn had_key_component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        let c = byte as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
            out.push(c);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

enum HadSource {
    Dir { dir: PathBuf, root: NavKvRoot },
    Zip { path: PathBuf, root: NavKvRoot },
}

impl HadSource {
    fn open(path: &Path) -> anyhow::Result<Self> {
        if path.is_dir() {
            let root_bytes = fs::read(path.join("root"))
                .with_context(|| format!("failed to read {}", path.join("root").display()))?;
            let root = NavKvRoot::parse(&root_bytes).map_err(anyhow::Error::msg)?;
            return Ok(Self::Dir {
                dir: path.to_path_buf(),
                root,
            });
        }

        let file =
            File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
        let mut archive = ZipArchive::new(file)
            .with_context(|| format!("failed to read zip {}", path.display()))?;
        let root_bytes = read_zip_member(&mut archive, "root")?;
        let root = NavKvRoot::parse(&root_bytes).map_err(anyhow::Error::msg)?;
        Ok(Self::Zip {
            path: path.to_path_buf(),
            root,
        })
    }

    fn query(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        match self {
            Self::Dir { dir, root } => Ok(root.extract_value(key, |page_index| {
                fs::read(dir.join(format!("page_{page_index:04}"))).ok()
            })),
            Self::Zip { path, root } => {
                let file = File::open(path)
                    .with_context(|| format!("failed to open {}", path.display()))?;
                let mut archive = ZipArchive::new(file)
                    .with_context(|| format!("failed to read zip {}", path.display()))?;
                Ok(root.extract_value(key, |page_index| {
                    read_zip_member(&mut archive, &format!("page_{page_index:04}")).ok()
                }))
            }
        }
    }
}

fn read_zip_member(archive: &mut ZipArchive<File>, name: &str) -> anyhow::Result<Vec<u8>> {
    let mut file = archive
        .by_name(name)
        .with_context(|| format!("missing zip member {name}"))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .with_context(|| format!("failed to read zip member {name}"))?;
    Ok(bytes)
}
