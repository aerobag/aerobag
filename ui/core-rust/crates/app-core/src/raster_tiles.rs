use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{LatLon, MapViewport};

const WORLD_SIZE: f64 = 256.0;
const MAX_LATITUDE: f64 = 85.05112878;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterMapCatalog {
    pub selected_map_id: String,
    pub selected_map: Option<RasterMapViewOption>,
    pub displayed_maps: Vec<RasterMapViewOption>,
    pub geometry: RasterDisplayGeometry,
    #[serde(default)]
    pub family_options: Vec<RasterMapFamilyOption>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterMapFamilyOption {
    pub id: String,
    pub label: String,
    pub launcher_label: String,
    pub enabled: bool,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterMapUiState {
    pub selected_map_id: String,
    pub selected_map_label: String,
    pub selected_family_id: String,
    pub selected_family_label: String,
    pub selected_family_launcher_label: String,
    pub min_zoom: f64,
    pub max_zoom: f64,
    pub initial_viewport: RasterInitialViewport,
    #[serde(default)]
    pub family_options: Vec<RasterMapFamilyOption>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterMapViewOption {
    pub id: String,
    pub label: String,
    pub region_id: String,
    pub coverage: Option<RasterChartCoverage>,
    pub map_view: RasterMapView,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum RasterChartCoverage {
    PolygonSetRef(RasterPolygonSetRef),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterPolygonSetRef {
    pub polygon_set_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RasterDisplayGeometry {
    pub schema_version: u32,
    #[serde(default)]
    pub polygons: Vec<RasterPolygon>,
    #[serde(default)]
    pub polygon_sets: Vec<RasterDisplayPolygonSet>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterPolygon {
    pub id: String,
    pub points: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterDisplayPolygonSet {
    pub id: String,
    pub polygon_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterMapView {
    pub chart_family: String,
    pub chart_name: String,
    pub chart_index: i64,
    pub tile_root: String,
    pub tile_url_root: String,
    pub tile_path_template: String,
    pub tile_size: i64,
    pub min_zoom: f64,
    pub max_zoom: f64,
    pub max_source_zoom: i64,
    pub max_display_zoom: f64,
    pub storage_kind: String,
    pub package_name: Option<String>,
    pub full_coverage_zoom: Option<f64>,
    pub initial_viewport: RasterInitialViewport,
    pub levels: Vec<RasterTileLevel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterInitialViewport {
    pub lat: f64,
    pub lon: f64,
    pub zoom: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterTileLevel {
    pub zoom: i64,
    pub x_min: i64,
    pub x_max: i64,
    pub y_tms_min: i64,
    pub y_tms_max: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterTilePlan {
    pub selected_map_id: String,
    pub tiles: Vec<RasterTileDraw>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RasterTilePlanOptions {
    pub max_tile_display_multiplier: f64,
}

impl Default for RasterTilePlanOptions {
    fn default() -> Self {
        Self {
            max_tile_display_multiplier: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterTileDraw {
    pub draw_key: String,
    pub family: String,
    pub source_zoom: i64,
    pub x: i64,
    pub y_tms: i64,
    pub left_px: f64,
    pub top_px: f64,
    pub size_px: f64,
    pub z_order: i64,
    pub primary: RasterTileSource,
    pub fallbacks: Vec<RasterTileSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterTileSource {
    pub map_view_id: String,
    pub package_name: Option<String>,
    pub storage_kind: String,
    pub relative_path: String,
    pub url: String,
}

#[derive(Debug, Clone)]
struct PlannedTile {
    display_x: i64,
    x: i64,
    y_tms: i64,
    left_px: f64,
    top_px: f64,
    size_px: f64,
    zoom: i64,
    map_view_id: String,
    map_view: RasterMapView,
    candidate_map_views: Vec<(String, RasterMapView)>,
}

#[derive(Debug, Clone, Copy)]
struct TileBounds {
    south: f64,
    west: f64,
    north: f64,
    east: f64,
}

type PolygonSetLookup = HashMap<String, Vec<Vec<[f64; 2]>>>;

pub fn select_map_in_catalog(catalog: &mut RasterMapCatalog, selected_map_id: &str) {
    if catalog
        .displayed_maps
        .iter()
        .any(|view| view.id == selected_map_id)
    {
        catalog.selected_map_id = selected_map_id.to_string();
        catalog.selected_map = catalog
            .displayed_maps
            .iter()
            .find(|view| view.id == selected_map_id)
            .cloned();
    }
}

pub fn select_map_family_in_catalog(catalog: &mut RasterMapCatalog, family_id: &str) {
    let selected_region_id = catalog
        .selected_map
        .as_ref()
        .map(|view| view.region_id.as_str());
    let Some(selected_map) =
        preferred_family_map(&catalog.displayed_maps, family_id, selected_region_id).cloned()
    else {
        return;
    };
    catalog.selected_map_id = selected_map.id.clone();
    catalog.selected_map = Some(selected_map);
    for option in &mut catalog.family_options {
        option.active = option.id == family_id;
    }
}

pub fn raster_map_ui_state(catalog: &RasterMapCatalog) -> Option<RasterMapUiState> {
    let selected_map = catalog.selected_map.as_ref()?;
    let selected_family = catalog
        .family_options
        .iter()
        .find(|option| option.active)
        .or_else(|| {
            catalog
                .family_options
                .iter()
                .find(|option| option.id == selected_map.map_view.chart_family)
        });
    Some(RasterMapUiState {
        selected_map_id: catalog.selected_map_id.clone(),
        selected_map_label: selected_map.label.clone(),
        selected_family_id: selected_family
            .map(|option| option.id.clone())
            .unwrap_or_else(|| selected_map.map_view.chart_family.clone()),
        selected_family_label: selected_family
            .map(|option| option.label.clone())
            .unwrap_or_else(|| selected_map.map_view.chart_name.clone()),
        selected_family_launcher_label: selected_family
            .map(|option| option.launcher_label.clone())
            .unwrap_or_else(|| selected_map.map_view.chart_family.clone()),
        min_zoom: selected_map.map_view.min_zoom,
        max_zoom: selected_map.map_view.max_zoom,
        initial_viewport: selected_map.map_view.initial_viewport.clone(),
        family_options: catalog.family_options.clone(),
    })
}

pub fn preferred_family_map<'a>(
    map_views: &'a [RasterMapViewOption],
    family_id: &str,
    selected_region_id: Option<&str>,
) -> Option<&'a RasterMapViewOption> {
    map_views
        .iter()
        .find(|view| {
            view.map_view.chart_family == family_id
                && Some(view.region_id.as_str()) == selected_region_id
        })
        .or_else(|| {
            map_views
                .iter()
                .find(|view| view.map_view.chart_family == family_id)
        })
}

pub fn raster_tile_plan(
    catalog: &RasterMapCatalog,
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
) -> RasterTilePlan {
    raster_tile_plan_with_options(
        catalog,
        viewport,
        width_px,
        height_px,
        RasterTilePlanOptions::default(),
    )
}

pub fn raster_tile_plan_with_options(
    catalog: &RasterMapCatalog,
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    options: RasterTilePlanOptions,
) -> RasterTilePlan {
    if width_px <= 0.0 || height_px <= 0.0 {
        return RasterTilePlan {
            selected_map_id: catalog.selected_map_id.clone(),
            tiles: Vec::new(),
        };
    }
    let polygon_sets = build_polygon_set_lookup(&catalog.geometry);
    let mut by_family: HashMap<String, Vec<(String, RasterMapViewOption)>> = HashMap::new();
    for view in &catalog.displayed_maps {
        by_family
            .entry(view.map_view.chart_family.clone())
            .or_default()
            .push((view.id.clone(), view.clone()));
    }
    let mut planned = Vec::new();
    let selected_region_id = catalog
        .displayed_maps
        .iter()
        .find(|view| view.id == catalog.selected_map_id)
        .map(|view| view.region_id.as_str());
    for family_views in by_family.values() {
        planned.extend(render_tiles_for_family(
            family_views,
            &polygon_sets,
            viewport,
            width_px,
            height_px,
            selected_region_id,
            options,
        ));
    }
    let mut tiles = dedupe_tiles(planned)
        .into_iter()
        .map(planned_tile_to_draw)
        .collect::<Vec<_>>();
    tiles.sort_by(|left, right| {
        left.z_order
            .cmp(&right.z_order)
            .then(left.y_tms.cmp(&right.y_tms))
            .then(left.x.cmp(&right.x))
            .then(left.draw_key.cmp(&right.draw_key))
    });
    RasterTilePlan {
        selected_map_id: catalog.selected_map_id.clone(),
        tiles,
    }
}

fn render_tiles_for_family(
    family_views: &[(String, RasterMapViewOption)],
    polygon_sets: &PolygonSetLookup,
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    selected_region_id: Option<&str>,
    options: RasterTilePlanOptions,
) -> Vec<PlannedTile> {
    let family_full_coverage_zoom = family_views
        .iter()
        .filter_map(|(_, view)| view.map_view.full_coverage_zoom)
        .min_by(|left, right| left.total_cmp(right));
    let scale = scale_for_zoom(viewport.zoom);
    let center_world = lat_lon_to_world(viewport.center);
    let min_world_x = center_world.0 - width_px / 2.0 / scale;
    let max_world_x = center_world.0 + width_px / 2.0 / scale;
    let min_world_y = center_world.1 - height_px / 2.0 / scale;
    let max_world_y = center_world.1 + height_px / 2.0 / scale;
    let mut tiles = Vec::new();

    for (map_view_id, option) in family_views {
        let map_view = &option.map_view;
        let levels = levels_for_map_view(map_view, viewport.zoom, options);
        for level in levels {
            let is_full_coverage_level =
                family_full_coverage_zoom.is_some_and(|zoom| (level.zoom as f64) <= zoom);
            let tile_world_size = WORLD_SIZE / 2_f64.powi(level.zoom as i32);
            let tile_screen_size = tile_world_size * scale;
            let x_start = (min_world_x / tile_world_size).floor() as i64;
            let x_end = (max_world_x / tile_world_size).ceil() as i64 - 1;
            let y_start = (min_world_y / tile_world_size).floor() as i64;
            let y_end = (max_world_y / tile_world_size).ceil() as i64 - 1;
            let level_scale = 2_i64.pow(level.zoom as u32);

            for y_xyz in y_start..=y_end {
                for display_x in x_start..=x_end {
                    let x = positive_mod_i64(display_x, level_scale);
                    let y_tms = (level_scale - 1) - y_xyz;
                    if x < level.x_min
                        || x > level.x_max
                        || y_tms < level.y_tms_min
                        || y_tms > level.y_tms_max
                    {
                        continue;
                    }
                    if !tile_intersects_coverage(option, polygon_sets, level.zoom, x, y_tms) {
                        continue;
                    }
                    let candidates = if is_full_coverage_level {
                        let Some(representative_id) = full_coverage_representative(
                            family_views,
                            polygon_sets,
                            level.zoom,
                            x,
                            y_tms,
                            selected_region_id,
                        ) else {
                            continue;
                        };
                        if representative_id != *map_view_id {
                            continue;
                        }
                        full_coverage_candidates(
                            family_views,
                            polygon_sets,
                            level.zoom,
                            x,
                            y_tms,
                            map_view_id,
                            map_view,
                        )
                    } else {
                        vec![(map_view_id.clone(), map_view.clone())]
                    };
                    let left_px = (display_x as f64 * tile_world_size - center_world.0) * scale
                        + width_px / 2.0;
                    let top_px =
                        (y_xyz as f64 * tile_world_size - center_world.1) * scale + height_px / 2.0;
                    if !screen_rect_intersects_viewport(
                        left_px,
                        top_px,
                        tile_screen_size,
                        width_px,
                        height_px,
                    ) {
                        continue;
                    }
                    tiles.push(PlannedTile {
                        display_x,
                        x,
                        y_tms,
                        left_px,
                        top_px,
                        size_px: tile_screen_size,
                        zoom: level.zoom,
                        map_view_id: map_view_id.clone(),
                        map_view: map_view.clone(),
                        candidate_map_views: candidates,
                    });
                }
            }
        }
    }
    tiles
}

fn screen_rect_intersects_viewport(
    left_px: f64,
    top_px: f64,
    size_px: f64,
    width_px: f64,
    height_px: f64,
) -> bool {
    left_px < width_px && top_px < height_px && left_px + size_px > 0.0 && top_px + size_px > 0.0
}

fn full_coverage_representative(
    family_views: &[(String, RasterMapViewOption)],
    polygon_sets: &PolygonSetLookup,
    zoom: i64,
    x: i64,
    y_tms: i64,
    selected_region_id: Option<&str>,
) -> Option<String> {
    family_views
        .iter()
        .filter(|(_, option)| {
            let map_view = &option.map_view;
            map_view
                .full_coverage_zoom
                .is_some_and(|full_zoom| (zoom as f64) <= full_zoom)
                && level_contains(map_view, zoom, x, y_tms)
                && tile_intersects_coverage(option, polygon_sets, zoom, x, y_tms)
        })
        .max_by_key(|(map_view_id, option)| {
            let selected_region_bonus = if Some(option.region_id.as_str()) == selected_region_id {
                1_i64
            } else {
                0
            };
            (selected_region_bonus, map_view_id.clone())
        })
        .map(|(map_view_id, _)| map_view_id.clone())
}

fn full_coverage_candidates(
    family_views: &[(String, RasterMapViewOption)],
    polygon_sets: &PolygonSetLookup,
    zoom: i64,
    x: i64,
    y_tms: i64,
    primary_id: &str,
    primary: &RasterMapView,
) -> Vec<(String, RasterMapView)> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    push_candidate(
        &mut candidates,
        &mut seen,
        primary_id.to_string(),
        primary.clone(),
    );
    for (map_view_id, option) in family_views {
        let map_view = &option.map_view;
        if map_view
            .full_coverage_zoom
            .is_none_or(|full_zoom| (zoom as f64) > full_zoom)
        {
            continue;
        }
        if !level_contains(map_view, zoom, x, y_tms) {
            continue;
        }
        if !tile_intersects_coverage(option, polygon_sets, zoom, x, y_tms) {
            continue;
        }
        push_candidate(
            &mut candidates,
            &mut seen,
            map_view_id.clone(),
            map_view.clone(),
        );
    }
    candidates
}

fn push_candidate(
    candidates: &mut Vec<(String, RasterMapView)>,
    seen: &mut HashSet<String>,
    map_view_id: String,
    map_view: RasterMapView,
) {
    let key = format!(
        "{}:{}:{}",
        map_view.package_name.as_deref().unwrap_or(""),
        map_view.tile_root,
        map_view.chart_index
    );
    if seen.insert(key) {
        candidates.push((map_view_id, map_view));
    }
}

fn planned_tile_to_draw(tile: PlannedTile) -> RasterTileDraw {
    let mut sources = tile
        .candidate_map_views
        .iter()
        .map(|(map_view_id, map_view)| {
            tile_source(map_view_id, map_view, tile.zoom, tile.x, tile.y_tms)
        })
        .collect::<Vec<_>>();
    if sources.is_empty() {
        sources.push(tile_source(
            &tile.map_view_id,
            &tile.map_view,
            tile.zoom,
            tile.x,
            tile.y_tms,
        ));
    }
    let primary = sources.remove(0);
    RasterTileDraw {
        draw_key: format!(
            "{}:{}:{}:{}:{}",
            tile.map_view_id, tile.zoom, tile.display_x, tile.x, tile.y_tms
        ),
        family: tile.map_view.chart_family.clone(),
        source_zoom: tile.zoom,
        x: tile.x,
        y_tms: tile.y_tms,
        left_px: tile.left_px,
        top_px: tile.top_px,
        size_px: tile.size_px,
        z_order: raster_tile_z_order(tile.zoom, &tile.map_view.chart_family),
        primary,
        fallbacks: sources,
    }
}

fn tile_source(
    map_view_id: &str,
    map_view: &RasterMapView,
    zoom: i64,
    x: i64,
    y_tms: i64,
) -> RasterTileSource {
    let relative_path = tile_relative_path(map_view, zoom, x, y_tms);
    let url = format!(
        "{}/{}",
        map_view.tile_url_root.trim_end_matches('/'),
        relative_path
    );
    RasterTileSource {
        map_view_id: map_view_id.to_string(),
        package_name: map_view.package_name.clone(),
        storage_kind: map_view.storage_kind.clone(),
        relative_path,
        url,
    }
}

fn tile_relative_path(map_view: &RasterMapView, zoom: i64, x: i64, y_tms: i64) -> String {
    let template = if map_view.tile_path_template.is_empty() {
        format!("{}/{{z}}/{{x}}/{{y}}.webp", map_view.chart_index)
    } else {
        map_view.tile_path_template.clone()
    };
    template
        .replace("{z}", &zoom.to_string())
        .replace("{x}", &x.to_string())
        .replace("{y}", &y_tms.to_string())
}

fn dedupe_tiles(tiles: Vec<PlannedTile>) -> Vec<PlannedTile> {
    let mut by_key: HashMap<String, PlannedTile> = HashMap::new();
    for tile in tiles {
        let key = format!(
            "{}:{}:{}:{}:{}",
            tile.map_view_id, tile.zoom, tile.display_x, tile.x, tile.y_tms
        );
        if let Some(existing) = by_key.get_mut(&key) {
            let mut seen = existing
                .candidate_map_views
                .iter()
                .map(|(_, map_view)| {
                    format!(
                        "{}:{}:{}",
                        map_view.package_name.as_deref().unwrap_or(""),
                        map_view.tile_root,
                        map_view.chart_index
                    )
                })
                .collect::<HashSet<_>>();
            for (id, map_view) in tile.candidate_map_views {
                push_candidate(&mut existing.candidate_map_views, &mut seen, id, map_view);
            }
        } else {
            by_key.insert(key, tile);
        }
    }
    by_key.into_values().collect()
}

fn levels_for_map_view(
    map_view: &RasterMapView,
    zoom: f64,
    options: RasterTilePlanOptions,
) -> Vec<RasterTileLevel> {
    if zoom < map_view.min_zoom || zoom > map_view.max_display_zoom {
        return Vec::new();
    }
    let Some(desired_level) = pick_level(map_view, zoom, options) else {
        return Vec::new();
    };
    vec![desired_level.clone()]
}

fn pick_level(
    map_view: &RasterMapView,
    zoom: f64,
    options: RasterTilePlanOptions,
) -> Option<&RasterTileLevel> {
    let multiplier = if options.max_tile_display_multiplier.is_finite() {
        options.max_tile_display_multiplier.max(1.0)
    } else {
        1.0
    };
    let max_display_size = if map_view.tile_size > 0 {
        map_view.tile_size as f64
    } else {
        WORLD_SIZE
    } * multiplier;
    let eligible_levels = map_view
        .levels
        .iter()
        .filter(|level| level.zoom <= map_view.max_source_zoom);
    eligible_levels
        .clone()
        .filter(|level| {
            let tile_world_size = WORLD_SIZE / 2_f64.powi(level.zoom as i32);
            tile_world_size * scale_for_zoom(zoom) <= max_display_size
        })
        .min_by_key(|level| level.zoom)
        .or_else(|| eligible_levels.max_by_key(|level| level.zoom))
}

fn positive_mod_i64(value: i64, modulus: i64) -> i64 {
    ((value % modulus) + modulus) % modulus
}

fn level_contains(map_view: &RasterMapView, zoom: i64, x: i64, y_tms: i64) -> bool {
    map_view.levels.iter().any(|level| {
        level.zoom == zoom
            && x >= level.x_min
            && x <= level.x_max
            && y_tms >= level.y_tms_min
            && y_tms <= level.y_tms_max
    })
}

fn raster_tile_z_order(zoom: i64, family: &str) -> i64 {
    zoom * 10 + chart_family_render_priority(family)
}

fn chart_family_render_priority(family: &str) -> i64 {
    match family {
        "world-basemap" => -1000,
        "shaded-relief" => -10,
        "tac" => 1,
        _ => 0,
    }
}

fn build_polygon_set_lookup(geometry: &RasterDisplayGeometry) -> PolygonSetLookup {
    let polygons_by_id = geometry
        .polygons
        .iter()
        .map(|polygon| (polygon.id.clone(), polygon.points.clone()))
        .collect::<HashMap<_, _>>();
    geometry
        .polygon_sets
        .iter()
        .map(|polygon_set| {
            let polygons = polygon_set
                .polygon_ids
                .iter()
                .filter_map(|id| polygons_by_id.get(id).cloned())
                .collect::<Vec<_>>();
            (polygon_set.id.clone(), polygons)
        })
        .collect()
}

fn tile_intersects_coverage(
    option: &RasterMapViewOption,
    polygon_sets: &PolygonSetLookup,
    zoom: i64,
    x: i64,
    y_tms: i64,
) -> bool {
    let Some(RasterChartCoverage::PolygonSetRef(coverage)) = &option.coverage else {
        return true;
    };
    let Some(polygons) = polygon_sets.get(&coverage.polygon_set_id) else {
        return false;
    };
    let tile_bounds = tile_bounds_for(zoom, x, y_tms);
    polygons
        .iter()
        .any(|polygon| polygon_intersects_rect(polygon, tile_bounds))
}

fn tile_bounds_for(zoom: i64, x: i64, y_tms: i64) -> TileBounds {
    let level_scale = 2_i64.pow(zoom as u32);
    let y_xyz = (level_scale - 1) - y_tms;
    let tile_world_size = WORLD_SIZE / level_scale as f64;
    let northwest = world_to_lat_lon(x as f64 * tile_world_size, y_xyz as f64 * tile_world_size);
    let southeast = world_to_lat_lon(
        (x + 1) as f64 * tile_world_size,
        (y_xyz + 1) as f64 * tile_world_size,
    );
    TileBounds {
        south: northwest.lat.min(southeast.lat),
        north: northwest.lat.max(southeast.lat),
        west: northwest.lon.min(southeast.lon),
        east: northwest.lon.max(southeast.lon),
    }
}

fn polygon_intersects_rect(polygon: &[[f64; 2]], rect: TileBounds) -> bool {
    polygon
        .iter()
        .any(|point| point_in_rect(point[0], point[1], rect))
        || rect_corners(rect)
            .iter()
            .any(|point| point_in_polygon(point[0], point[1], polygon))
        || polygon_edges_intersect_rect(polygon, rect)
}

fn rect_corners(rect: TileBounds) -> [[f64; 2]; 4] {
    [
        [rect.west, rect.north],
        [rect.east, rect.north],
        [rect.east, rect.south],
        [rect.west, rect.south],
    ]
}

fn polygon_edges_intersect_rect(polygon: &[[f64; 2]], rect: TileBounds) -> bool {
    let corners = rect_corners(rect);
    let rect_edges = [
        (corners[0], corners[1]),
        (corners[1], corners[2]),
        (corners[2], corners[3]),
        (corners[3], corners[0]),
    ];
    polygon.windows(2).any(|edge| {
        rect_edges
            .iter()
            .any(|(from, to)| segments_intersect(edge[0], edge[1], *from, *to))
    })
}

fn point_in_rect(lon: f64, lat: f64, rect: TileBounds) -> bool {
    lon >= rect.west && lon <= rect.east && lat >= rect.south && lat <= rect.north
}

fn point_in_polygon(lon: f64, lat: f64, polygon: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let mut j = polygon.len().saturating_sub(1);
    for i in 0..polygon.len() {
        let xi = polygon[i][0];
        let yi = polygon[i][1];
        let xj = polygon[j][0];
        let yj = polygon[j][1];
        if (yi > lat) != (yj > lat) && lon < (xj - xi) * (lat - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn segments_intersect(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    fn orientation(p: [f64; 2], q: [f64; 2], r: [f64; 2]) -> f64 {
        (q[1] - p[1]) * (r[0] - q[0]) - (q[0] - p[0]) * (r[1] - q[1])
    }
    fn on_segment(p: [f64; 2], q: [f64; 2], r: [f64; 2]) -> bool {
        q[0] <= p[0].max(r[0])
            && q[0] >= p[0].min(r[0])
            && q[1] <= p[1].max(r[1])
            && q[1] >= p[1].min(r[1])
    }

    let o1 = orientation(a, b, c);
    let o2 = orientation(a, b, d);
    let o3 = orientation(c, d, a);
    let o4 = orientation(c, d, b);
    if (o1 > 0.0) != (o2 > 0.0) && (o3 > 0.0) != (o4 > 0.0) {
        return true;
    }
    let eps = 1e-10;
    (o1.abs() < eps && on_segment(a, c, b))
        || (o2.abs() < eps && on_segment(a, d, b))
        || (o3.abs() < eps && on_segment(c, a, d))
        || (o4.abs() < eps && on_segment(c, b, d))
}

fn lat_lon_to_world(point: LatLon) -> (f64, f64) {
    let clamped_lat = point.lat.clamp(-MAX_LATITUDE, MAX_LATITUDE);
    let x = ((point.lon + 180.0) / 360.0) * WORLD_SIZE;
    let y =
        ((1.0 - clamped_lat.to_radians().tan().asinh() / std::f64::consts::PI) / 2.0) * WORLD_SIZE;
    (x, y)
}

fn world_to_lat_lon(world_x: f64, world_y: f64) -> LatLon {
    let lon = (world_x / WORLD_SIZE) * 360.0 - 180.0;
    let n = std::f64::consts::PI - (2.0 * std::f64::consts::PI * world_y) / WORLD_SIZE;
    let lat = n.sinh().atan().to_degrees();
    LatLon { lat, lon }
}

fn scale_for_zoom(zoom: f64) -> f64 {
    2_f64.powf(zoom)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(zoom: i64, x_min: i64, x_max: i64, y_min: i64, y_max: i64) -> RasterTileLevel {
        RasterTileLevel {
            zoom,
            x_min,
            x_max,
            y_tms_min: y_min,
            y_tms_max: y_max,
        }
    }

    fn option(
        id: &str,
        family: &str,
        package: &str,
        levels: Vec<RasterTileLevel>,
    ) -> RasterMapViewOption {
        RasterMapViewOption {
            id: id.to_string(),
            label: id.to_string(),
            region_id: id.split(':').nth(1).unwrap_or(id).to_string(),
            coverage: None,
            map_view: RasterMapView {
                chart_family: family.to_string(),
                chart_name: id.to_string(),
                chart_index: 0,
                tile_root: "tiles".to_string(),
                tile_url_root: format!("/{package}/tiles"),
                tile_path_template: "{z}/{x}/{y}.webp".to_string(),
                tile_size: 512,
                min_zoom: 0.0,
                max_zoom: 12.5,
                max_source_zoom: 12,
                max_display_zoom: 12.5,
                storage_kind: "sectional_package".to_string(),
                package_name: Some(package.to_string()),
                full_coverage_zoom: Some(7.0),
                initial_viewport: RasterInitialViewport {
                    lat: 38.1,
                    lon: -122.0,
                    zoom: 7.3,
                },
                levels,
            },
        }
    }

    #[test]
    fn sacramento_viewport_does_not_use_alaska_low_zoom_representative() {
        let catalog = RasterMapCatalog {
            selected_map_id: "tac:nw".to_string(),
            selected_map: None,
            displayed_maps: vec![
                option(
                    "sec:ak",
                    "sec",
                    "AK_SEC",
                    vec![
                        level(0, 0, 0, 0, 0),
                        level(1, 0, 1, 0, 1),
                        level(2, 0, 3, 0, 3),
                    ],
                ),
                option(
                    "tac:ak",
                    "tac",
                    "AK_TAC",
                    vec![
                        level(0, 0, 0, 0, 0),
                        level(1, 0, 1, 0, 1),
                        level(2, 0, 3, 0, 3),
                    ],
                ),
                option(
                    "sec:nw",
                    "sec",
                    "NW_SEC",
                    vec![
                        level(0, 0, 0, 0, 0),
                        level(1, 0, 1, 0, 1),
                        level(2, 0, 3, 0, 3),
                        level(7, 18, 23, 76, 81),
                    ],
                ),
                option(
                    "tac:nw",
                    "tac",
                    "NW_TAC",
                    vec![
                        level(0, 0, 0, 0, 0),
                        level(1, 0, 1, 0, 1),
                        level(2, 0, 3, 0, 3),
                        level(7, 18, 23, 76, 81),
                    ],
                ),
            ],
            geometry: RasterDisplayGeometry::default(),
            family_options: Vec::new(),
        };
        let plan = raster_tile_plan(
            &catalog,
            &MapViewport {
                center: LatLon {
                    lat: 38.13483035117734,
                    lon: -121.95686691849119,
                },
                zoom: 7.316666666666666,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            786.0,
            708.0,
        );
        assert!(plan
            .tiles
            .iter()
            .any(|tile| tile.primary.package_name.as_deref() == Some("NW_SEC")));
        assert!(plan
            .tiles
            .iter()
            .any(|tile| tile.primary.package_name.as_deref() == Some("NW_TAC")));
        assert!(!plan
            .tiles
            .iter()
            .any(|tile| tile.primary.package_name.as_deref() == Some("AK_SEC")));
        assert!(!plan
            .tiles
            .iter()
            .any(|tile| tile.primary.package_name.as_deref() == Some("AK_TAC")));
    }

    #[test]
    fn chart_packages_plan_only_one_retina_appropriate_zoom_level() {
        let catalog = RasterMapCatalog {
            selected_map_id: "sec:nw".to_string(),
            selected_map: None,
            displayed_maps: vec![option(
                "sec:nw",
                "sec",
                "NW_SEC",
                vec![
                    level(0, 0, 0, 0, 0),
                    level(1, 0, 1, 0, 1),
                    level(2, 0, 3, 0, 3),
                    level(7, 18, 23, 76, 81),
                ],
            )],
            geometry: RasterDisplayGeometry::default(),
            family_options: Vec::new(),
        };
        let plan = raster_tile_plan(
            &catalog,
            &MapViewport {
                center: LatLon {
                    lat: 38.13483035117734,
                    lon: -121.95686691849119,
                },
                zoom: 7.3,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            786.0,
            708.0,
        );
        assert!(!plan.tiles.is_empty());
        assert!(plan.tiles.iter().all(|tile| tile.source_zoom == 7));
    }

    #[test]
    fn static_product_can_overzoom_source_until_display_limit() {
        let mut basemap = option(
            "world-basemap",
            "world-basemap",
            "world-basemap",
            vec![
                level(0, 0, 0, 0, 0),
                level(1, 0, 1, 0, 1),
                level(2, 0, 3, 0, 3),
                level(3, 0, 7, 0, 7),
                level(4, 0, 15, 0, 15),
            ],
        );
        basemap.map_view.max_source_zoom = 4;
        basemap.map_view.max_display_zoom = 7.0;
        let catalog = RasterMapCatalog {
            selected_map_id: "sec:nw".to_string(),
            selected_map: None,
            displayed_maps: vec![basemap],
            geometry: RasterDisplayGeometry::default(),
            family_options: Vec::new(),
        };

        let overzoomed = raster_tile_plan(
            &catalog,
            &MapViewport {
                center: LatLon { lat: 0.0, lon: 0.0 },
                zoom: 6.5,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            512.0,
            512.0,
        );
        assert!(!overzoomed.tiles.is_empty());
        assert!(overzoomed
            .tiles
            .iter()
            .all(|tile| tile.source_zoom == 4 && tile.family == "world-basemap"));

        let past_display_limit = raster_tile_plan(
            &catalog,
            &MapViewport {
                center: LatLon { lat: 0.0, lon: 0.0 },
                zoom: 7.1,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            512.0,
            512.0,
        );
        assert!(past_display_limit.tiles.is_empty());
    }

    #[test]
    fn world_basemap_sorts_below_other_raster_layers() {
        assert!(raster_tile_z_order(4, "world-basemap") < raster_tile_z_order(0, "shaded-relief"));
        assert!(raster_tile_z_order(4, "world-basemap") < raster_tile_z_order(0, "sec"));
    }

    #[test]
    fn tiles_touching_viewport_edge_do_not_count_as_visible() {
        let catalog = RasterMapCatalog {
            selected_map_id: "sec:nw".to_string(),
            selected_map: None,
            displayed_maps: vec![option(
                "sec:nw",
                "sec",
                "NW_SEC",
                vec![level(1, 0, 1, 0, 1)],
            )],
            geometry: RasterDisplayGeometry::default(),
            family_options: Vec::new(),
        };
        let plan = raster_tile_plan(
            &catalog,
            &MapViewport {
                center: LatLon {
                    lat: 0.0,
                    lon: -90.0,
                },
                zoom: 1.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            256.0,
            128.0,
        );

        assert!(!plan.tiles.is_empty());
        assert!(plan.tiles.iter().all(|tile| tile.x == 0));
        assert!(plan.tiles.iter().all(|tile| {
            tile.left_px < 256.0
                && tile.top_px < 128.0
                && tile.left_px + tile.size_px > 0.0
                && tile.top_px + tile.size_px > 0.0
        }));
    }

    #[test]
    fn raster_tiles_wrap_source_x_but_draw_in_repeated_world_copy() {
        let catalog = RasterMapCatalog {
            selected_map_id: "sec:world".to_string(),
            selected_map: None,
            displayed_maps: vec![option(
                "sec:world",
                "sec",
                "WORLD_SEC",
                vec![level(2, 0, 3, 0, 3)],
            )],
            geometry: RasterDisplayGeometry::default(),
            family_options: Vec::new(),
        };
        let plan = raster_tile_plan(
            &catalog,
            &MapViewport {
                center: LatLon {
                    lat: 0.0,
                    lon: -540.0,
                },
                zoom: 2.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            512.0,
            256.0,
        );

        assert!(!plan.tiles.is_empty());
        assert!(plan.tiles.iter().all(|tile| (0..=3).contains(&tile.x)));
        assert!(plan.tiles.iter().any(|tile| {
            tile.left_px < 512.0
                && tile.top_px < 256.0
                && tile.left_px + tile.size_px > 0.0
                && tile.top_px + tile.size_px > 0.0
        }));
        assert!(plan
            .tiles
            .iter()
            .any(|tile| tile.draw_key.contains(":-4:0:")));
    }

    #[test]
    fn raster_tiles_keep_multiple_visible_copies_of_same_source_tile() {
        let catalog = RasterMapCatalog {
            selected_map_id: "sec:world".to_string(),
            selected_map: None,
            displayed_maps: vec![option(
                "sec:world",
                "sec",
                "WORLD_SEC",
                vec![level(1, 0, 1, 0, 1)],
            )],
            geometry: RasterDisplayGeometry::default(),
            family_options: Vec::new(),
        };
        let plan = raster_tile_plan(
            &catalog,
            &MapViewport {
                center: LatLon { lat: 0.0, lon: 0.0 },
                zoom: 1.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            1024.0,
            256.0,
        );
        let copies_of_x0_y1 = plan
            .tiles
            .iter()
            .filter(|tile| tile.x == 0 && tile.y_tms == 1)
            .count();
        let copies_of_x1_y1 = plan
            .tiles
            .iter()
            .filter(|tile| tile.x == 1 && tile.y_tms == 1)
            .count();

        assert!(
            copies_of_x0_y1 >= 2,
            "missing repeated x0/y1 tile: {plan:?}"
        );
        assert!(
            copies_of_x1_y1 >= 2,
            "missing repeated x1/y1 tile: {plan:?}"
        );
    }

    #[test]
    fn tile_size_allows_lower_source_zoom_until_it_would_blur() {
        let catalog = RasterMapCatalog {
            selected_map_id: "sec:nw".to_string(),
            selected_map: None,
            displayed_maps: vec![option(
                "sec:nw",
                "sec",
                "NW_SEC",
                vec![
                    level(8, 40, 43, 155, 158),
                    level(9, 80, 86, 310, 316),
                    level(10, 160, 172, 620, 632),
                ],
            )],
            geometry: RasterDisplayGeometry::default(),
            family_options: Vec::new(),
        };
        let plan_at_integer_zoom = raster_tile_plan(
            &catalog,
            &MapViewport {
                center: LatLon {
                    lat: 38.13483035117734,
                    lon: -121.95686691849119,
                },
                zoom: 10.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            786.0,
            708.0,
        );
        assert!(!plan_at_integer_zoom.tiles.is_empty());
        assert!(plan_at_integer_zoom
            .tiles
            .iter()
            .all(|tile| { tile.source_zoom == 9 && tile.size_px <= 512.0 }));

        let plan_when_zoomed_in = raster_tile_plan(
            &catalog,
            &MapViewport {
                center: LatLon {
                    lat: 38.13483035117734,
                    lon: -121.95686691849119,
                },
                zoom: 10.1,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            786.0,
            708.0,
        );
        assert!(!plan_when_zoomed_in.tiles.is_empty());
        assert!(plan_when_zoomed_in
            .tiles
            .iter()
            .all(|tile| { tile.source_zoom == 10 && tile.size_px <= 512.0 }));
    }

    #[test]
    fn fast_tile_option_allows_two_x_overscaling_before_next_zoom() {
        let catalog = RasterMapCatalog {
            selected_map_id: "sec:nw".to_string(),
            selected_map: None,
            displayed_maps: vec![option(
                "sec:nw",
                "sec",
                "NW_SEC",
                vec![
                    level(8, 40, 43, 155, 158),
                    level(9, 80, 86, 310, 316),
                    level(10, 160, 172, 620, 632),
                ],
            )],
            geometry: RasterDisplayGeometry::default(),
            family_options: Vec::new(),
        };
        let normal = raster_tile_plan(
            &catalog,
            &MapViewport {
                center: LatLon {
                    lat: 38.13483035117734,
                    lon: -121.95686691849119,
                },
                zoom: 10.1,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            786.0,
            708.0,
        );
        assert!(!normal.tiles.is_empty());
        assert!(normal.tiles.iter().all(|tile| tile.source_zoom == 10));

        let fast = raster_tile_plan_with_options(
            &catalog,
            &MapViewport {
                center: LatLon {
                    lat: 38.13483035117734,
                    lon: -121.95686691849119,
                },
                zoom: 10.1,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            786.0,
            708.0,
            RasterTilePlanOptions {
                max_tile_display_multiplier: 2.0,
            },
        );
        assert!(!fast.tiles.is_empty());
        assert!(fast
            .tiles
            .iter()
            .all(|tile| tile.source_zoom == 9 && tile.size_px <= 1024.0));
        assert!(fast.tiles.len() < normal.tiles.len());
    }
}
