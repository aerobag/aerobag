use app_core::{NavKvLookup, NavKvRoot, NavKvStore};
use image::{Rgba, RgbaImage};
use procedure_geometry_types as pgt;
use std::fs;
use std::path::PathBuf;

const CANVAS_SIZE_PX: u32 = 1800;
const CANVAS_PAD_PX: f64 = 90.0;

#[test]
#[ignore = "manual diagnostic render for a published procedure geometry record"]
fn writes_current_buexre_overlay() {
    let airport_id = env_non_empty("BUEXRE_AIRPORT");
    let procedure_id = env_non_empty("BUEXRE_PROCEDURE");
    let enroute_transition = env_non_empty("BUEXRE_TRANSITION");
    let output_dir = PathBuf::from(
        env_non_empty("BUEXRE_OUTPUT_DIR").unwrap_or_else(|| "/tmp/procedure-plots".to_string()),
    );
    fs::create_dir_all(&output_dir)
        .unwrap_or_else(|err| panic!("create {}: {err}", output_dir.display()));

    let store = fixture_nav_store();
    let record_key = match (airport_id, procedure_id) {
        (Some(airport_id), Some(procedure_id)) => {
            let key = pgt::ProcedureGeometryKey {
                airport_id,
                procedure_id,
                kind: pgt::ProcedureKind::Approach,
                runway_transition: None,
                enroute_transition,
            };
            pgt::procedure_geometry_navdb_key(&key)
        }
        _ => first_geometry_key(&store),
    };
    let record = read_record(&store, &record_key);
    let output_stem = env_non_empty("BUEXRE_OUTPUT").unwrap_or_else(|| {
        format!(
            "procedure_geometry_{}_{}_{}",
            record.key.airport_id,
            record.key.procedure_id,
            record.key.enroute_transition.as_deref().unwrap_or("_")
        )
    });
    let plot = ProcedurePlot::from_record(&record);

    let mut image = RgbaImage::from_pixel(CANVAS_SIZE_PX, CANVAS_SIZE_PX, Rgba([22, 24, 28, 255]));
    for (bundle_index, bundle) in record.leg_bundles.iter().enumerate() {
        let color = match bundle.path.style {
            pgt::ProcedureGeometryPathStyle::Solid => Rgba([255, 150, 35, 255]),
            pgt::ProcedureGeometryPathStyle::Dashed => Rgba([170, 170, 170, 230]),
        };
        for element in &bundle.path.elements {
            draw_element(&mut image, &plot, element, color, 3);
        }
        for waypoint in &bundle.waypoints {
            if let Some(position) = nav_ref_position(&waypoint.nav_ref) {
                let (x, y) = plot.project(position);
                draw_disc(&mut image, x, y, 4, Rgba([255, 255, 255, 255]));
            }
        }
        if bundle_index == 0 {
            if let Some(start) = first_element_point(bundle) {
                let (x, y) = plot.project(start);
                draw_disc(&mut image, x, y, 7, Rgba([0, 220, 120, 255]));
            }
        }
    }

    let png_path = output_dir.join(format!("{output_stem}.png"));
    image
        .save(&png_path)
        .unwrap_or_else(|err| panic!("write {}: {err}", png_path.display()));

    let note_path = output_dir.join(format!("{output_stem}.txt"));
    fs::write(
        &note_path,
        render_record_notes(&record, &record_key, &png_path),
    )
    .unwrap_or_else(|err| panic!("write {}: {err}", note_path.display()));
}

fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn first_geometry_key(store: &NavKvStore) -> String {
    let keys = store.keys_with_prefix("procedure/geometry/");
    keys.into_iter().next().unwrap_or_else(|| {
        panic!(
            "fixture nav-db contains no procedure/geometry records; rebuild the fixture nav-db after the preprocessor geometry extraction"
        )
    })
}

fn fixture_nav_store() -> NavKvStore {
    let (root_bytes, pages) = app_fixtures::load_fixture_nav_kv_pages();
    let root = NavKvRoot::parse(&root_bytes).expect("parse fixture nav_kv root");
    let mut store = NavKvStore::new(root);
    for (index, page) in pages.into_iter().enumerate() {
        store.insert_page(index as u32, page);
    }
    store
}

fn read_record(store: &NavKvStore, key: &str) -> pgt::ProcedureGeometryRecord {
    match store.get_bytes(key).expect("read nav_kv key") {
        NavKvLookup::Hit(bytes) => serde_json::from_slice(&bytes)
            .unwrap_or_else(|err| panic!("parse procedure geometry {key}: {err}")),
        NavKvLookup::MissingKey => {
            let nearby = store
                .keys_with_prefix("procedure/geometry/")
                .into_iter()
                .take(12)
                .collect::<Vec<_>>();
            panic!("missing procedure geometry key {key}; first geometry keys: {nearby:?}")
        }
        NavKvLookup::MissingPages(pages) => {
            panic!("fixture store unexpectedly missing pages for {key}: {pages:?}")
        }
    }
}

fn render_record_notes(
    record: &pgt::ProcedureGeometryRecord,
    record_key: &str,
    png_path: &std::path::Path,
) -> String {
    let mut lines = vec![
        format!("key={record_key}"),
        format!("png={}", png_path.display()),
        format!("airport={}", record.key.airport_id),
        format!("procedure={}", record.key.procedure_id),
        format!(
            "enroute_transition={}",
            record.key.enroute_transition.as_deref().unwrap_or("")
        ),
        String::new(),
    ];
    for (index, bundle) in record.leg_bundles.iter().enumerate() {
        lines.push(format!(
            "step-{index:02} {} {:?} seq={} elements={} rows={:?}",
            bundle.id,
            bundle.path_termination,
            bundle.leg_sequence,
            bundle.path.elements.len(),
            bundle.source_row_sequences,
        ));
    }
    if !record.data_quality.is_empty() {
        lines.push(String::new());
        lines.push("data_quality:".to_string());
        for item in &record.data_quality {
            lines.push(format!("- {}", item.message));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

#[derive(Debug, Clone, Copy)]
struct ProcedurePlot {
    min_lat: f64,
    max_lat: f64,
    min_lon: f64,
    max_lon: f64,
}

impl ProcedurePlot {
    fn from_record(record: &pgt::ProcedureGeometryRecord) -> Self {
        let mut points = Vec::new();
        for bundle in &record.leg_bundles {
            for element in &bundle.path.elements {
                collect_element_points(element, &mut points);
            }
        }
        assert!(
            !points.is_empty(),
            "procedure geometry record has no drawable points"
        );
        let (mut min_lat, mut max_lat) = (f64::INFINITY, f64::NEG_INFINITY);
        let (mut min_lon, mut max_lon) = (f64::INFINITY, f64::NEG_INFINITY);
        for point in points {
            min_lat = min_lat.min(point.lat);
            max_lat = max_lat.max(point.lat);
            min_lon = min_lon.min(point.lon);
            max_lon = max_lon.max(point.lon);
        }
        if (max_lat - min_lat).abs() < 0.01 {
            min_lat -= 0.005;
            max_lat += 0.005;
        }
        if (max_lon - min_lon).abs() < 0.01 {
            min_lon -= 0.005;
            max_lon += 0.005;
        }
        Self {
            min_lat,
            max_lat,
            min_lon,
            max_lon,
        }
    }

    fn project(&self, point: pgt::ProcedureLatLon) -> (i32, i32) {
        let usable = CANVAS_SIZE_PX as f64 - 2.0 * CANVAS_PAD_PX;
        let x =
            CANVAS_PAD_PX + ((point.lon - self.min_lon) / (self.max_lon - self.min_lon)) * usable;
        let y =
            CANVAS_PAD_PX + ((self.max_lat - point.lat) / (self.max_lat - self.min_lat)) * usable;
        (x.round() as i32, y.round() as i32)
    }
}

fn draw_element(
    image: &mut RgbaImage,
    plot: &ProcedurePlot,
    element: &pgt::ProcedureGeometryElement,
    color: Rgba<u8>,
    radius: i32,
) {
    let points = element_polyline(element, 48);
    let pixels = points
        .into_iter()
        .map(|point| plot.project(point))
        .collect::<Vec<_>>();
    draw_polyline(image, &pixels, color, radius);
}

fn collect_element_points(
    element: &pgt::ProcedureGeometryElement,
    out: &mut Vec<pgt::ProcedureLatLon>,
) {
    match element {
        pgt::ProcedureGeometryElement::Segment { start, end } => {
            out.push(*start);
            out.push(*end);
        }
        pgt::ProcedureGeometryElement::Arc { start, end, .. } => {
            out.push(*start);
            out.push(*end);
            out.extend(element_polyline(element, 16));
        }
    }
}

fn element_polyline(
    element: &pgt::ProcedureGeometryElement,
    steps: usize,
) -> Vec<pgt::ProcedureLatLon> {
    match element {
        pgt::ProcedureGeometryElement::Segment { start, end } => vec![*start, *end],
        pgt::ProcedureGeometryElement::Arc {
            center,
            radius_nm,
            start,
            clockwise,
            sweep_degrees,
            ..
        } => {
            let start_radial = bearing_degrees(*center, *start);
            let signed_sweep = if *clockwise {
                sweep_degrees.abs()
            } else {
                -sweep_degrees.abs()
            };
            (0..=steps)
                .map(|index| {
                    let radial = start_radial + signed_sweep * (index as f64 / steps as f64);
                    destination_point(*center, radial, *radius_nm)
                })
                .collect()
        }
    }
}

fn first_element_point(bundle: &pgt::ProcedureGeometryLegBundle) -> Option<pgt::ProcedureLatLon> {
    match bundle.path.elements.first()? {
        pgt::ProcedureGeometryElement::Segment { start, .. } => Some(*start),
        pgt::ProcedureGeometryElement::Arc { start, .. } => Some(*start),
    }
}

fn nav_ref_position(nav_ref: &pgt::ProcedureNavRef) -> Option<pgt::ProcedureLatLon> {
    match nav_ref {
        pgt::ProcedureNavRef::LatLon(position) => Some(*position),
        _ => None,
    }
}

fn draw_polyline(image: &mut RgbaImage, points: &[(i32, i32)], color: Rgba<u8>, radius: i32) {
    for pair in points.windows(2) {
        draw_line(image, pair[0], pair[1], color, radius);
    }
}

fn draw_line(
    image: &mut RgbaImage,
    start: (i32, i32),
    end: (i32, i32),
    color: Rgba<u8>,
    radius: i32,
) {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let steps = dx.abs().max(dy.abs()).max(1);
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let x = start.0 as f64 + dx as f64 * t;
        let y = start.1 as f64 + dy as f64 * t;
        draw_disc(image, x.round() as i32, y.round() as i32, radius, color);
    }
}

fn draw_disc(image: &mut RgbaImage, cx: i32, cy: i32, radius: i32, color: Rgba<u8>) {
    for y in (cy - radius)..=(cy + radius) {
        for x in (cx - radius)..=(cx + radius) {
            if (x - cx).pow(2) + (y - cy).pow(2) > radius.pow(2) {
                continue;
            }
            if x < 0 || y < 0 || x >= image.width() as i32 || y >= image.height() as i32 {
                continue;
            }
            image.put_pixel(x as u32, y as u32, color);
        }
    }
}

fn bearing_degrees(from: pgt::ProcedureLatLon, to: pgt::ProcedureLatLon) -> f64 {
    let from_lat = from.lat.to_radians();
    let to_lat = to.lat.to_radians();
    let delta_lon = (to.lon - from.lon).to_radians();
    let y = delta_lon.sin() * to_lat.cos();
    let x = from_lat.cos() * to_lat.sin() - from_lat.sin() * to_lat.cos() * delta_lon.cos();
    y.atan2(x).to_degrees().rem_euclid(360.0)
}

fn destination_point(
    origin: pgt::ProcedureLatLon,
    bearing_deg: f64,
    distance_nm: f64,
) -> pgt::ProcedureLatLon {
    let angular_distance = distance_nm / 3440.065;
    let bearing_rad = bearing_deg.to_radians();
    let lat1 = origin.lat.to_radians();
    let lon1 = origin.lon.to_radians();
    let lat2 = (lat1.sin() * angular_distance.cos()
        + lat1.cos() * angular_distance.sin() * bearing_rad.cos())
    .asin();
    let lon2 = lon1
        + (bearing_rad.sin() * angular_distance.sin() * lat1.cos())
            .atan2(angular_distance.cos() - lat1.sin() * lat2.sin());
    pgt::ProcedureLatLon {
        lat: lat2.to_degrees(),
        lon: lon2.to_degrees(),
    }
}
