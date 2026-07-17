use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{core_clock_ms, LatLon, MapViewport};

const WORLD_SIZE: f64 = 256.0;
const MAX_LATITUDE: f64 = 85.05112878;
const NO_RASTER_FAMILY_ID: &str = "none";
const NO_RASTER_SELECTED_MAP_ID: &str = "none";
const CORE_INITIAL_VIEWPORT: RasterInitialViewport = RasterInitialViewport {
    lat: 47.4931388888889,
    lon: -122.21575,
    zoom: 10.0,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterMapCatalog {
    pub selected_map_id: String,
    pub selected_map: Option<RasterMapViewOption>,
    pub available_maps: Vec<RasterMapViewOption>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    pub active: bool,
    #[serde(default)]
    pub has_references: bool,
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
    #[serde(default)]
    pub reference_assets: Vec<RasterChartReferenceAsset>,
    pub map_view: RasterMapView,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RasterChartReferenceCoverage {
    pub lat_min: f64,
    pub lat_max: f64,
    pub lon_min: f64,
    pub lon_max: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterChartReferenceAsset {
    pub id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_coverage: Option<RasterChartReferenceCoverage>,
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
    #[serde(default)]
    pub package_relative_path: Option<String>,
    #[serde(default)]
    pub package_effective_date: Option<String>,
    #[serde(default)]
    pub package_expiration_date: Option<String>,
    pub full_coverage_zoom: Option<f64>,
    #[serde(default)]
    pub wide_angle: Option<RasterWideAngleMapView>,
    pub initial_viewport: RasterInitialViewport,
    pub levels: Vec<RasterTileLevel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterWideAngleMapView {
    pub region_id: String,
    pub max_zoom: f64,
    pub package_name: String,
    #[serde(default)]
    pub package_relative_path: Option<String>,
    #[serde(default)]
    pub package_effective_date: Option<String>,
    #[serde(default)]
    pub package_expiration_date: Option<String>,
    pub tile_url_root: String,
    pub tile_path_template: String,
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
    pub boxes: Vec<RasterTileBounds>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterTileBounds {
    pub x_min: i64,
    pub x_max: i64,
    pub y_tms_min: i64,
    pub y_tms_max: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterTilePlan {
    pub selected_map_id: String,
    pub tiles: Vec<RasterTileDraw>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_reference_action: Option<RasterChartReferenceAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug_timing: Option<RasterTilePlanDebugTiming>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterChartReferenceAction {
    pub family_id: String,
    #[serde(default)]
    pub suggested_chart_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterTilePlanDebugTiming {
    pub planner_total_ms: u64,
    pub planner_group_ms: u64,
    pub planner_render_ms: u64,
    pub planner_dedupe_ms: u64,
    pub planner_draw_ms: u64,
    pub planner_sort_ms: u64,
    pub planner_families: usize,
    pub planner_displayed_maps: usize,
    pub planner_planned_tiles: usize,
    pub planner_deduped_tiles: usize,
    pub planner_tiles: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_total_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_lock_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_advance_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_freshness_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_catalog_filter_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_source_displayed_maps: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_source_available_maps: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_displayed_maps: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RasterTilePlanOptions {
    pub max_tile_display_multiplier: f64,
    pub device_pixel_ratio: f64,
    pub resource_mode: RasterResourceMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RasterResourceMode {
    PublicUnpacked,
    InstalledPackage,
}

impl Default for RasterTilePlanOptions {
    fn default() -> Self {
        Self {
            max_tile_display_multiplier: 1.0,
            device_pixel_ratio: 1.0,
            resource_mode: RasterResourceMode::InstalledPackage,
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
    pub resource: RasterTileResource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RasterTileResource {
    InstalledPackage {
        package_name: String,
        member_path: String,
    },
    PublicUnpacked {
        package_name: String,
        member_path: String,
    },
    ResolvedPublicUrl {
        url: String,
    },
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

pub fn select_map_in_catalog(catalog: &mut RasterMapCatalog, selected_map_id: &str) {
    if let Some(selected_map) = catalog
        .available_maps
        .iter()
        .find(|view| view.id == selected_map_id)
        .cloned()
    {
        catalog.selected_map_id = selected_map_id.to_string();
        catalog.selected_map = Some(selected_map);
        update_displayed_maps_for_family(catalog);
    }
}

pub fn select_map_family_in_catalog(catalog: &mut RasterMapCatalog, family_id: &str) {
    if family_id == NO_RASTER_FAMILY_ID {
        catalog.selected_map_id = NO_RASTER_SELECTED_MAP_ID.to_string();
        for option in &mut catalog.family_options {
            option.active = option.id == NO_RASTER_FAMILY_ID;
        }
        catalog.displayed_maps.clear();
        return;
    }
    let selected_region_id = catalog
        .selected_map
        .as_ref()
        .map(|view| view.region_id.as_str());
    let Some(selected_map) =
        preferred_family_map(&catalog.available_maps, family_id, selected_region_id).cloned()
    else {
        return;
    };
    catalog.selected_map_id = selected_map.id.clone();
    catalog.selected_map = Some(selected_map);
    for option in &mut catalog.family_options {
        option.active = option.id == family_id;
    }
    update_displayed_maps_for_family(catalog);
}

fn update_displayed_maps_for_family(catalog: &mut RasterMapCatalog) {
    let selected_family_id = catalog
        .selected_map
        .as_ref()
        .map(|view| view.map_view.chart_family.as_str())
        .unwrap_or("sec");
    if catalog
        .family_options
        .iter()
        .any(|option| option.id == NO_RASTER_FAMILY_ID && option.active)
    {
        catalog.displayed_maps.clear();
        return;
    }
    let mut displayed_maps = displayed_family_maps(&catalog.available_maps, selected_family_id)
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    let mut displayed_map_ids = displayed_maps
        .iter()
        .map(|view| view.id.clone())
        .collect::<HashSet<_>>();
    displayed_maps.extend(
        background_maps(&catalog.available_maps)
            .into_iter()
            .filter(|view| displayed_map_ids.insert(view.id.clone()))
            .cloned(),
    );
    catalog.displayed_maps = displayed_maps;
}

fn displayed_family_maps<'a>(
    map_views: &'a [RasterMapViewOption],
    family_id: &str,
) -> Vec<&'a RasterMapViewOption> {
    if matches!(family_id, "tac" | "flyway") {
        return map_views
            .iter()
            .filter(|view| {
                let chart_family = view.map_view.chart_family.as_str();
                chart_family == "sec" || chart_family == family_id
            })
            .collect();
    }
    map_views
        .iter()
        .filter(|view| view.map_view.chart_family == family_id)
        .collect()
}

fn background_maps(map_views: &[RasterMapViewOption]) -> Vec<&RasterMapViewOption> {
    map_views
        .iter()
        .filter(|view| view.map_view.chart_family == "world-basemap")
        .collect()
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
        min_zoom: effective_min_zoom(&selected_map.map_view),
        max_zoom: selected_map.map_view.max_zoom,
        initial_viewport: CORE_INITIAL_VIEWPORT,
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
    let total_started_at = core_clock_ms();
    if width_px <= 0.0 || height_px <= 0.0 {
        return RasterTilePlan {
            selected_map_id: catalog.selected_map_id.clone(),
            tiles: Vec::new(),
            chart_reference_action: chart_reference_action(catalog, viewport, 0.0, 0.0),
            debug_timing: None,
        };
    }
    let device_pixel_ratio =
        if options.device_pixel_ratio.is_finite() && options.device_pixel_ratio > 0.0 {
            options.device_pixel_ratio
        } else {
            1.0
        };
    let display_zoom = viewport.zoom;
    let planning_viewport = if (device_pixel_ratio - 1.0).abs() > f64::EPSILON {
        MapViewport {
            center: viewport.center,
            zoom: viewport.zoom + device_pixel_ratio.log2(),
            rotation_deg: viewport.rotation_deg,
            pitch_deg: viewport.pitch_deg,
        }
    } else {
        *viewport
    };
    let planning_width_px = width_px * device_pixel_ratio;
    let planning_height_px = height_px * device_pixel_ratio;
    let group_started_at = core_clock_ms();
    let mut by_family: HashMap<String, Vec<(String, RasterMapViewOption)>> = HashMap::new();
    for view in &catalog.displayed_maps {
        by_family
            .entry(view.map_view.chart_family.clone())
            .or_default()
            .push((view.id.clone(), view.clone()));
    }
    let group_ms = elapsed_ms(group_started_at);
    let mut planned = Vec::new();
    let selected_region_id = catalog
        .displayed_maps
        .iter()
        .find(|view| view.id == catalog.selected_map_id)
        .map(|view| view.region_id.as_str());
    let render_started_at = core_clock_ms();
    for family_views in by_family.values() {
        planned.extend(render_tiles_for_family(
            family_views,
            &planning_viewport,
            planning_width_px,
            planning_height_px,
            display_zoom,
            selected_region_id,
            options,
        ));
    }
    let render_ms = elapsed_ms(render_started_at);
    let planned_count = planned.len();
    let dedupe_started_at = core_clock_ms();
    let deduped = dedupe_tiles(planned);
    let deduped_count = deduped.len();
    let dedupe_ms = elapsed_ms(dedupe_started_at);
    let draw_started_at = core_clock_ms();
    let mut tiles = deduped
        .into_iter()
        .map(|tile| planned_tile_to_draw(tile, options))
        .collect::<Vec<_>>();
    let draw_ms = elapsed_ms(draw_started_at);
    let sort_started_at = core_clock_ms();
    tiles.sort_by(|left, right| {
        left.z_order
            .cmp(&right.z_order)
            .then(left.y_tms.cmp(&right.y_tms))
            .then(left.x.cmp(&right.x))
            .then(left.draw_key.cmp(&right.draw_key))
    });
    let sort_ms = elapsed_ms(sort_started_at);
    let debug_timing = total_started_at.map(|started_at| RasterTilePlanDebugTiming {
        planner_total_ms: elapsed_ms(Some(started_at)),
        planner_group_ms: group_ms,
        planner_render_ms: render_ms,
        planner_dedupe_ms: dedupe_ms,
        planner_draw_ms: draw_ms,
        planner_sort_ms: sort_ms,
        planner_families: by_family.len(),
        planner_displayed_maps: catalog.displayed_maps.len(),
        planner_planned_tiles: planned_count,
        planner_deduped_tiles: deduped_count,
        planner_tiles: tiles.len(),
        session_total_ms: None,
        session_lock_ms: None,
        session_advance_ms: None,
        session_freshness_ms: None,
        session_catalog_filter_ms: None,
        session_source_displayed_maps: None,
        session_source_available_maps: None,
        session_displayed_maps: None,
    });
    RasterTilePlan {
        selected_map_id: catalog.selected_map_id.clone(),
        tiles,
        chart_reference_action: chart_reference_action(catalog, viewport, width_px, height_px),
        debug_timing,
    }
}

fn chart_reference_action(
    catalog: &RasterMapCatalog,
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
) -> Option<RasterChartReferenceAction> {
    if catalog.selected_map_id == NO_RASTER_SELECTED_MAP_ID {
        return None;
    }
    let selected_map = catalog
        .available_maps
        .iter()
        .find(|view| view.id == catalog.selected_map_id)
        .or(catalog.selected_map.as_ref())?;
    if selected_map.reference_assets.is_empty() {
        return None;
    }
    let viewport_bounds = viewport_geo_bounds(viewport, width_px, height_px);
    let suggested_chart_ids = selected_map
        .reference_assets
        .iter()
        .filter(|asset| asset.kind == "inset")
        .filter(|asset| {
            asset
                .source_coverage
                .is_some_and(|coverage| coverage_intersects(coverage, viewport_bounds))
        })
        .map(|asset| asset.id.clone())
        .collect();
    Some(RasterChartReferenceAction {
        family_id: selected_map.map_view.chart_family.clone(),
        suggested_chart_ids,
    })
}

fn viewport_geo_bounds(
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
) -> RasterChartReferenceCoverage {
    let center = lat_lon_to_world(viewport.center);
    let scale = scale_for_zoom(viewport.zoom);
    let half_extent = width_px.hypot(height_px) / 2.0 / scale;
    let (lat_max, lon_min) = world_to_lat_lon(center.0 - half_extent, center.1 - half_extent);
    let (lat_min, lon_max) = world_to_lat_lon(center.0 + half_extent, center.1 + half_extent);
    RasterChartReferenceCoverage {
        lat_min,
        lat_max,
        lon_min,
        lon_max,
    }
}

fn coverage_intersects(
    left: RasterChartReferenceCoverage,
    right: RasterChartReferenceCoverage,
) -> bool {
    left.lon_min < right.lon_max
        && left.lon_max > right.lon_min
        && left.lat_min < right.lat_max
        && left.lat_max > right.lat_min
}

fn elapsed_ms(started_at: Option<f64>) -> u64 {
    let Some(started_at) = started_at else {
        return 0;
    };
    let Some(now_ms) = core_clock_ms() else {
        return 0;
    };
    (now_ms - started_at).max(0.0).round() as u64
}

fn render_tiles_for_family(
    family_views: &[(String, RasterMapViewOption)],
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    display_zoom: f64,
    selected_region_id: Option<&str>,
    options: RasterTilePlanOptions,
) -> Vec<PlannedTile> {
    if let Some(tiles) = render_wide_angle_tiles_for_family(
        family_views,
        viewport,
        width_px,
        height_px,
        display_zoom,
        selected_region_id,
        options,
    ) {
        return tiles;
    }

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
        let levels = levels_for_map_view(map_view, display_zoom, viewport.zoom, options);
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
                    if !level_contains_tile(&level, x, y_tms) {
                        continue;
                    }
                    let candidates = if is_full_coverage_level {
                        let Some(representative_id) = full_coverage_representative(
                            family_views,
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

fn render_wide_angle_tiles_for_family(
    family_views: &[(String, RasterMapViewOption)],
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    display_zoom: f64,
    selected_region_id: Option<&str>,
    options: RasterTilePlanOptions,
) -> Option<Vec<PlannedTile>> {
    let (map_view_id, option, wide_angle) = family_views
        .iter()
        .filter_map(|(map_view_id, option)| {
            option
                .map_view
                .wide_angle
                .as_ref()
                .map(|wide_angle| (map_view_id, option, wide_angle))
        })
        .filter(|(_, option, wide_angle)| {
            display_zoom <= wide_angle_max_display_zoom(&option.map_view, wide_angle, options)
        })
        .max_by_key(|(_, option, _)| {
            if Some(option.region_id.as_str()) == selected_region_id {
                1_i64
            } else {
                0
            }
        })?;
    let mut map_view = option.map_view.clone();
    map_view.tile_url_root = wide_angle.tile_url_root.clone();
    map_view.tile_path_template = wide_angle.tile_path_template.clone();
    map_view.min_zoom = wide_angle_min_zoom(&map_view, wide_angle);
    map_view.max_source_zoom = wide_angle.max_zoom.floor() as i64;
    map_view.max_display_zoom = wide_angle_max_display_zoom(&map_view, wide_angle, options);
    map_view.package_name = Some(wide_angle.package_name.clone());
    map_view.package_relative_path = wide_angle.package_relative_path.clone();
    map_view.levels = wide_angle.levels.clone();
    map_view.full_coverage_zoom = None;
    map_view.wide_angle = None;

    let synthetic_option = RasterMapViewOption {
        id: format!("{}:{}", map_view.chart_family, wide_angle.region_id),
        label: option.label.clone(),
        region_id: wide_angle.region_id.clone(),
        reference_assets: option.reference_assets.clone(),
        map_view,
    };
    Some(render_tiles_for_single_map_view(
        map_view_id,
        &synthetic_option,
        viewport,
        width_px,
        height_px,
        display_zoom,
        options,
    ))
}

fn effective_min_zoom(map_view: &RasterMapView) -> f64 {
    map_view
        .wide_angle
        .as_ref()
        .map(|wide_angle| wide_angle_min_zoom(map_view, wide_angle))
        .unwrap_or(map_view.min_zoom)
}

fn wide_angle_min_zoom(map_view: &RasterMapView, wide_angle: &RasterWideAngleMapView) -> f64 {
    wide_angle
        .levels
        .iter()
        .map(|level| level.zoom)
        .min()
        .map(|zoom| (zoom as f64).min(map_view.min_zoom))
        .unwrap_or(map_view.min_zoom)
}

fn wide_angle_max_display_zoom(
    map_view: &RasterMapView,
    wide_angle: &RasterWideAngleMapView,
    options: RasterTilePlanOptions,
) -> f64 {
    let multiplier = tile_display_multiplier(options);
    let max_display_size = map_tile_display_size(map_view) * multiplier;
    wide_angle.max_zoom + (max_display_size / WORLD_SIZE).log2()
}

fn render_tiles_for_single_map_view(
    map_view_id: &str,
    option: &RasterMapViewOption,
    viewport: &MapViewport,
    width_px: f64,
    height_px: f64,
    display_zoom: f64,
    options: RasterTilePlanOptions,
) -> Vec<PlannedTile> {
    let map_view = &option.map_view;
    let scale = scale_for_zoom(viewport.zoom);
    let center_world = lat_lon_to_world(viewport.center);
    let min_world_x = center_world.0 - width_px / 2.0 / scale;
    let max_world_x = center_world.0 + width_px / 2.0 / scale;
    let min_world_y = center_world.1 - height_px / 2.0 / scale;
    let max_world_y = center_world.1 + height_px / 2.0 / scale;
    let mut tiles = Vec::new();

    for level in levels_for_map_view(map_view, display_zoom, viewport.zoom, options) {
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
                if !level_contains_tile(&level, x, y_tms) {
                    continue;
                }
                let left_px =
                    (display_x as f64 * tile_world_size - center_world.0) * scale + width_px / 2.0;
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
                    map_view_id: map_view_id.to_string(),
                    map_view: map_view.clone(),
                    candidate_map_views: vec![(map_view_id.to_string(), map_view.clone())],
                });
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

fn planned_tile_to_draw(tile: PlannedTile, options: RasterTilePlanOptions) -> RasterTileDraw {
    let mut sources = tile
        .candidate_map_views
        .iter()
        .map(|(map_view_id, map_view)| {
            tile_source(
                map_view_id,
                map_view,
                tile.zoom,
                tile.x,
                tile.y_tms,
                options.resource_mode,
            )
        })
        .collect::<Vec<_>>();
    if sources.is_empty() {
        sources.push(tile_source(
            &tile.map_view_id,
            &tile.map_view,
            tile.zoom,
            tile.x,
            tile.y_tms,
            options.resource_mode,
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
    resource_mode: RasterResourceMode,
) -> RasterTileSource {
    let relative_path = tile_relative_path(map_view, zoom, x, y_tms);
    let resource = tile_source_resource(map_view, &relative_path, resource_mode);
    RasterTileSource {
        map_view_id: map_view_id.to_string(),
        package_name: map_view.package_name.clone(),
        storage_kind: map_view.storage_kind.clone(),
        relative_path,
        resource,
    }
}

fn tile_source_resource(
    map_view: &RasterMapView,
    relative_path: &str,
    resource_mode: RasterResourceMode,
) -> RasterTileResource {
    // TASK-25 raster exception: raster tile plans carry package/member identity
    // because web and Android need a high-throughput tile-specific fetch/decode
    // path. This is not the general resource model. New resource consumers
    // should ask core for opaque CoreResourceRequest values instead; see
    // resolve_chart_asset_resource_in_session and the terrain/NEXRAD overlays.
    let package_name = map_view.package_name.clone().unwrap_or_else(|| {
        panic!(
            "raster map view {} is missing package_name",
            map_view.chart_name
        )
    });
    let member_path = tile_package_member_path(map_view, relative_path);
    match resource_mode {
        RasterResourceMode::InstalledPackage => RasterTileResource::InstalledPackage {
            package_name,
            member_path,
        },
        RasterResourceMode::PublicUnpacked => RasterTileResource::PublicUnpacked {
            package_name,
            member_path,
        },
    }
}

fn tile_package_member_path(map_view: &RasterMapView, relative_path: &str) -> String {
    let tile_root = map_view.tile_root.trim_matches('/');
    if tile_root.is_empty() {
        relative_path.trim_start_matches('/').to_string()
    } else {
        format!("{tile_root}/{}", relative_path.trim_start_matches('/'))
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
    display_zoom: f64,
    source_zoom: f64,
    options: RasterTilePlanOptions,
) -> Vec<RasterTileLevel> {
    if display_zoom < map_view.min_zoom || display_zoom > map_view.max_display_zoom {
        return Vec::new();
    }
    let Some(desired_level) = pick_level(map_view, source_zoom, options) else {
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
        tile_display_multiplier(options)
    } else {
        1.0
    };
    let max_display_size = map_tile_display_size(map_view) * multiplier;
    let max_reasonable_source_zoom = zoom.ceil() as i64;
    let eligible_levels = map_view.levels.iter().filter(|level| {
        level.zoom <= map_view.max_source_zoom && level.zoom <= max_reasonable_source_zoom
    });
    eligible_levels
        .clone()
        .filter(|level| {
            let tile_world_size = WORLD_SIZE / 2_f64.powi(level.zoom as i32);
            tile_world_size * scale_for_zoom(zoom) <= max_display_size
        })
        .min_by_key(|level| level.zoom)
        .or_else(|| eligible_levels.max_by_key(|level| level.zoom))
}

fn tile_display_multiplier(options: RasterTilePlanOptions) -> f64 {
    if options.max_tile_display_multiplier.is_finite() {
        options.max_tile_display_multiplier.max(1.0)
    } else {
        1.0
    }
}

fn map_tile_display_size(map_view: &RasterMapView) -> f64 {
    if map_view.tile_size > 0 {
        map_view.tile_size as f64
    } else {
        WORLD_SIZE
    }
}

fn positive_mod_i64(value: i64, modulus: i64) -> i64 {
    ((value % modulus) + modulus) % modulus
}

fn level_contains(map_view: &RasterMapView, zoom: i64, x: i64, y_tms: i64) -> bool {
    map_view
        .levels
        .iter()
        .any(|level| level.zoom == zoom && level_contains_tile(level, x, y_tms))
}

fn level_contains_tile(level: &RasterTileLevel, x: i64, y_tms: i64) -> bool {
    level.boxes.iter().any(|bbox| {
        x >= bbox.x_min && x <= bbox.x_max && y_tms >= bbox.y_tms_min && y_tms <= bbox.y_tms_max
    })
}

fn raster_tile_z_order(zoom: i64, family: &str) -> i64 {
    zoom * 10 + chart_family_render_priority(family)
}

fn chart_family_render_priority(family: &str) -> i64 {
    match family {
        "world-basemap" => -1000,
        "shaded-relief" => -10,
        "tac" | "flyway" => 1,
        _ => 0,
    }
}

fn lat_lon_to_world(point: LatLon) -> (f64, f64) {
    let clamped_lat = point.lat.clamp(-MAX_LATITUDE, MAX_LATITUDE);
    let x = ((point.lon + 180.0) / 360.0) * WORLD_SIZE;
    let y =
        ((1.0 - clamped_lat.to_radians().tan().asinh() / std::f64::consts::PI) / 2.0) * WORLD_SIZE;
    (x, y)
}

fn world_to_lat_lon(x: f64, y: f64) -> (f64, f64) {
    let lon = x / WORLD_SIZE * 360.0 - 180.0;
    let mercator = std::f64::consts::PI * (1.0 - 2.0 * y / WORLD_SIZE);
    let lat = mercator.sinh().atan().to_degrees();
    (lat, lon)
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
            boxes: vec![RasterTileBounds {
                x_min,
                x_max,
                y_tms_min: y_min,
                y_tms_max: y_max,
            }],
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
            reference_assets: Vec::new(),
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
                package_relative_path: Some(format!("{package}.zip")),
                package_effective_date: None,
                package_expiration_date: None,
                full_coverage_zoom: Some(7.0),
                wide_angle: None,
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
    fn none_family_displays_no_raster_tiles_or_basemap() {
        let mut catalog = RasterMapCatalog {
            selected_map_id: "sec:nw".to_string(),
            selected_map: Some(option(
                "sec:nw",
                "sec",
                "NW_SEC_2604",
                vec![level(4, 0, 15, 0, 15)],
            )),
            available_maps: vec![
                option("sec:nw", "sec", "NW_SEC_2604", vec![level(4, 0, 15, 0, 15)]),
                option(
                    "world",
                    "world-basemap",
                    "WORLD_BASEMAP",
                    vec![level(0, 0, 0, 0, 0)],
                ),
            ],
            displayed_maps: Vec::new(),
            geometry: RasterDisplayGeometry::default(),
            family_options: vec![
                RasterMapFamilyOption {
                    id: "none".to_string(),
                    label: "NONE".to_string(),
                    launcher_label: "NONE".to_string(),
                    enabled: true,
                    disabled_reason: None,
                    active: false,
                    has_references: false,
                },
                RasterMapFamilyOption {
                    id: "sec".to_string(),
                    label: "SECTIONAL".to_string(),
                    launcher_label: "SEC".to_string(),
                    enabled: true,
                    disabled_reason: None,
                    active: true,
                    has_references: false,
                },
            ],
        };
        select_map_family_in_catalog(&mut catalog, "none");

        let plan = raster_tile_plan_with_options(
            &catalog,
            &MapViewport {
                center: LatLon {
                    lat: 45.0,
                    lon: -122.0,
                },
                zoom: 4.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            512.0,
            512.0,
            RasterTilePlanOptions::default(),
        );

        assert_eq!(catalog.selected_map_id, "none");
        assert!(catalog.displayed_maps.is_empty());
        assert!(catalog.family_options[0].active);
        assert_eq!(plan.selected_map_id, "none");
        assert!(plan.tiles.is_empty());
    }

    #[test]
    fn flyway_family_displays_sectionals_and_flyway_but_not_tac() {
        let mut catalog = RasterMapCatalog {
            selected_map_id: "tac:nw".to_string(),
            selected_map: Some(option("tac:nw", "tac", "NW_TAC", vec![])),
            available_maps: vec![
                option("sec:nw", "sec", "NW_SEC", vec![]),
                option("tac:nw", "tac", "NW_TAC", vec![]),
                option("flyway:nw", "flyway", "NW_TAC", vec![]),
            ],
            displayed_maps: Vec::new(),
            geometry: RasterDisplayGeometry::default(),
            family_options: vec![RasterMapFamilyOption {
                id: "flyway".to_string(),
                label: "FLYWAY".to_string(),
                launcher_label: "FLY".to_string(),
                enabled: true,
                disabled_reason: None,
                active: false,
                has_references: true,
            }],
        };

        select_map_family_in_catalog(&mut catalog, "flyway");

        assert_eq!(catalog.selected_map_id, "flyway:nw");
        assert_eq!(
            catalog
                .displayed_maps
                .iter()
                .map(|view| view.id.as_str())
                .collect::<Vec<_>>(),
            vec!["sec:nw", "flyway:nw"]
        );
    }

    #[test]
    fn raster_tile_level_contains_tile_from_any_box() {
        let level = RasterTileLevel {
            zoom: 8,
            boxes: vec![
                RasterTileBounds {
                    x_min: 7,
                    x_max: 7,
                    y_tms_min: 117,
                    y_tms_max: 117,
                },
                RasterTileBounds {
                    x_min: 231,
                    x_max: 231,
                    y_tms_min: 137,
                    y_tms_max: 137,
                },
            ],
        };

        assert!(level_contains_tile(&level, 7, 117));
        assert!(level_contains_tile(&level, 231, 137));
        assert!(!level_contains_tile(&level, 119, 127));
    }

    #[test]
    fn public_unpacked_tile_sources_resolve_from_package_metadata() {
        let catalog = RasterMapCatalog {
            selected_map_id: "sec:nw".to_string(),
            selected_map: None,
            available_maps: Vec::new(),
            displayed_maps: vec![option(
                "sec:nw",
                "sec",
                "NW_SEC_2604",
                vec![level(4, 0, 15, 0, 15)],
            )],
            geometry: RasterDisplayGeometry::default(),
            family_options: Vec::new(),
        };

        let plan = raster_tile_plan_with_options(
            &catalog,
            &MapViewport {
                center: LatLon {
                    lat: 45.0,
                    lon: -122.0,
                },
                zoom: 4.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            256.0,
            256.0,
            RasterTilePlanOptions {
                resource_mode: RasterResourceMode::PublicUnpacked,
                ..RasterTilePlanOptions::default()
            },
        );

        let source = &plan.tiles.first().expect("planned tile").primary;
        let RasterTileResource::PublicUnpacked {
            package_name,
            member_path,
        } = &source.resource
        else {
            panic!("expected public unpacked resource");
        };
        assert_eq!(package_name, "NW_SEC_2604");
        assert_eq!(member_path, &format!("tiles/{}", source.relative_path));
    }

    #[test]
    fn installed_package_tile_sources_name_zip_member_explicitly() {
        let catalog = RasterMapCatalog {
            selected_map_id: "sec:nw".to_string(),
            selected_map: None,
            available_maps: Vec::new(),
            displayed_maps: vec![option(
                "sec:nw",
                "sec",
                "NW_SEC_2604",
                vec![level(4, 0, 15, 0, 15)],
            )],
            geometry: RasterDisplayGeometry::default(),
            family_options: Vec::new(),
        };

        let plan = raster_tile_plan(
            &catalog,
            &MapViewport {
                center: LatLon {
                    lat: 45.0,
                    lon: -122.0,
                },
                zoom: 4.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            256.0,
            256.0,
        );

        let source = &plan.tiles.first().expect("planned tile").primary;
        let RasterTileResource::InstalledPackage {
            package_name,
            member_path,
        } = &source.resource
        else {
            panic!("expected installed package resource");
        };
        assert_eq!(package_name, "NW_SEC_2604");
        assert_eq!(member_path, &format!("tiles/{}", source.relative_path));
    }

    #[test]
    fn sacramento_viewport_does_not_use_alaska_low_zoom_representative() {
        let catalog = RasterMapCatalog {
            selected_map_id: "tac:nw".to_string(),
            selected_map: None,
            available_maps: Vec::new(),
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
            available_maps: Vec::new(),
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
    fn missing_low_zoom_chart_tiles_suppress_layer_instead_of_flooding_high_zoom_tiles() {
        let catalog = RasterMapCatalog {
            selected_map_id: "sec:nw".to_string(),
            selected_map: None,
            available_maps: Vec::new(),
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
        let wide_view = raster_tile_plan(
            &catalog,
            &MapViewport {
                center: LatLon {
                    lat: 38.13483035117734,
                    lon: -121.95686691849119,
                },
                zoom: 2.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            786.0,
            708.0,
        );

        assert!(wide_view.tiles.is_empty());
    }

    #[test]
    fn wide_angle_source_replaces_regional_source_at_low_zoom() {
        let mut regional = option(
            "sec:nw",
            "sec",
            "NW_SEC_2604",
            vec![level(8, 40, 43, 155, 158), level(9, 80, 86, 310, 316)],
        );
        regional.map_view.min_zoom = 5.2;
        regional.map_view.max_source_zoom = 12;
        regional.map_view.max_display_zoom = 12.5;
        regional.map_view.wide_angle = Some(RasterWideAngleMapView {
            region_id: "wide".to_string(),
            max_zoom: 7.0,
            package_name: "SEC_WIDE_2604".to_string(),
            package_relative_path: Some("sec_wide_2604_sample.zip".to_string()),
            package_effective_date: None,
            package_expiration_date: None,
            tile_url_root: "tiles".to_string(),
            tile_path_template: "{z}/{x}/{y}.webp".to_string(),
            levels: vec![
                level(0, 0, 0, 0, 0),
                level(1, 0, 1, 0, 1),
                level(7, 0, 127, 0, 127),
            ],
        });
        let catalog = RasterMapCatalog {
            selected_map_id: "sec:nw".to_string(),
            selected_map: Some(regional.clone()),
            available_maps: Vec::new(),
            displayed_maps: vec![regional],
            geometry: RasterDisplayGeometry::default(),
            family_options: Vec::new(),
        };

        let low_zoom = raster_tile_plan(
            &catalog,
            &MapViewport {
                center: LatLon {
                    lat: 38.13483035117734,
                    lon: -121.95686691849119,
                },
                zoom: 6.8,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            786.0,
            708.0,
        );
        assert!(!low_zoom.tiles.is_empty());
        assert!(low_zoom
            .tiles
            .iter()
            .all(|tile| tile.primary.package_name.as_deref() == Some("SEC_WIDE_2604")));
        assert!(low_zoom.tiles.iter().all(|tile| tile.source_zoom <= 7));

        let far_zoomed_out = raster_tile_plan(
            &catalog,
            &MapViewport {
                center: LatLon {
                    lat: 38.13483035117734,
                    lon: -121.95686691849119,
                },
                zoom: 3.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            786.0,
            708.0,
        );
        assert!(!far_zoomed_out.tiles.is_empty());
        assert!(far_zoomed_out
            .tiles
            .iter()
            .all(|tile| tile.primary.package_name.as_deref() == Some("SEC_WIDE_2604")));
        assert!(far_zoomed_out
            .tiles
            .iter()
            .all(|tile| tile.source_zoom <= 3));

        let fractional_overview_zoom = raster_tile_plan(
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
        assert!(!fractional_overview_zoom.tiles.is_empty());
        assert!(fractional_overview_zoom.tiles.iter().all(|tile| tile
            .primary
            .package_name
            .as_deref()
            == Some("SEC_WIDE_2604")));
        assert!(fractional_overview_zoom
            .tiles
            .iter()
            .all(|tile| tile.source_zoom == 7));

        let high_zoom = raster_tile_plan(
            &catalog,
            &MapViewport {
                center: LatLon {
                    lat: 38.13483035117734,
                    lon: -121.95686691849119,
                },
                zoom: 8.2,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            786.0,
            708.0,
        );
        assert!(!high_zoom.tiles.is_empty());
        assert!(high_zoom
            .tiles
            .iter()
            .all(|tile| tile.primary.package_name.as_deref() == Some("NW_SEC_2604")));
        assert!(high_zoom.tiles.iter().all(|tile| tile.source_zoom >= 8));
    }

    #[test]
    fn raster_map_ui_state_min_zoom_includes_wide_angle_source() {
        let mut regional = option(
            "sec:nw",
            "sec",
            "NW_SEC_2604",
            vec![level(8, 40, 43, 155, 158), level(9, 80, 86, 310, 316)],
        );
        regional.map_view.min_zoom = 5.2;
        regional.map_view.wide_angle = Some(RasterWideAngleMapView {
            region_id: "wide".to_string(),
            max_zoom: 7.0,
            package_name: "SEC_WIDE_2604".to_string(),
            package_relative_path: Some("sec_wide_2604_sample.zip".to_string()),
            package_effective_date: None,
            package_expiration_date: None,
            tile_url_root: "tiles".to_string(),
            tile_path_template: "{z}/{x}/{y}.webp".to_string(),
            levels: vec![level(0, 0, 0, 0, 0), level(7, 0, 127, 0, 127)],
        });
        let catalog = RasterMapCatalog {
            selected_map_id: "sec:nw".to_string(),
            selected_map: Some(regional.clone()),
            available_maps: Vec::new(),
            displayed_maps: vec![regional],
            geometry: RasterDisplayGeometry::default(),
            family_options: Vec::new(),
        };

        let ui_state = raster_map_ui_state(&catalog).expect("raster map ui state");

        assert_eq!(ui_state.min_zoom, 0.0);
    }

    #[test]
    fn raster_map_ui_state_initial_viewport_is_core_startup_policy() {
        let regional = option(
            "sec:nw",
            "sec",
            "NW_SEC_2604",
            vec![level(8, 40, 43, 155, 158)],
        );
        let catalog = RasterMapCatalog {
            selected_map_id: "sec:nw".to_string(),
            selected_map: Some(regional.clone()),
            available_maps: Vec::new(),
            displayed_maps: vec![regional],
            geometry: RasterDisplayGeometry::default(),
            family_options: Vec::new(),
        };

        let ui_state = raster_map_ui_state(&catalog).expect("raster map ui state");

        assert_eq!(ui_state.initial_viewport, CORE_INITIAL_VIEWPORT);
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
            available_maps: Vec::new(),
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
            available_maps: Vec::new(),
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
            available_maps: Vec::new(),
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
            available_maps: Vec::new(),
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
            available_maps: Vec::new(),
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
    fn high_dpi_does_not_exhaust_logical_display_zoom_budget() {
        let catalog = RasterMapCatalog {
            selected_map_id: "tac:nw".to_string(),
            selected_map: None,
            available_maps: Vec::new(),
            displayed_maps: vec![option(
                "tac:nw",
                "tac",
                "NW_TAC",
                vec![level(12, 0, 4095, 0, 4095)],
            )],
            geometry: RasterDisplayGeometry::default(),
            family_options: Vec::new(),
        };

        let plan = raster_tile_plan_with_options(
            &catalog,
            &MapViewport {
                center: LatLon {
                    lat: 47.5,
                    lon: -122.3,
                },
                // High-DPI displays may prefer sharper source tiles, but DPR
                // must not make a chart disappear before the user-visible zoom
                // crosses the chart's max_display_zoom.
                zoom: 11.7,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            411.421875,
            760.0,
            RasterTilePlanOptions {
                device_pixel_ratio: 2.625,
                ..RasterTilePlanOptions::default()
            },
        );

        assert!(!plan.tiles.is_empty());
        assert!(plan.tiles.iter().all(|tile| tile.source_zoom == 12));
    }

    #[test]
    fn fast_tile_option_allows_two_x_overscaling_before_next_zoom() {
        let catalog = RasterMapCatalog {
            selected_map_id: "sec:nw".to_string(),
            selected_map: None,
            available_maps: Vec::new(),
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
                ..RasterTilePlanOptions::default()
            },
        );
        assert!(!fast.tiles.is_empty());
        assert!(fast
            .tiles
            .iter()
            .all(|tile| tile.source_zoom == 9 && tile.size_px <= 1024.0));
        assert!(fast.tiles.len() < normal.tiles.len());
    }

    #[test]
    fn raster_plan_suggests_all_reference_insets_overlapping_viewport() {
        let mut selected = option(
            "tac:sw",
            "tac",
            "SW_TAC_2607",
            vec![level(8, 0, 255, 0, 255)],
        );
        selected.reference_assets = vec![
            RasterChartReferenceAsset {
                id: "legend".to_string(),
                kind: "legend".to_string(),
                source_coverage: None,
            },
            RasterChartReferenceAsset {
                id: "la-one".to_string(),
                kind: "inset".to_string(),
                source_coverage: Some(RasterChartReferenceCoverage {
                    lat_min: 33.0,
                    lat_max: 35.0,
                    lon_min: -119.0,
                    lon_max: -117.0,
                }),
            },
            RasterChartReferenceAsset {
                id: "la-two".to_string(),
                kind: "inset".to_string(),
                source_coverage: Some(RasterChartReferenceCoverage {
                    lat_min: 33.5,
                    lat_max: 34.5,
                    lon_min: -118.5,
                    lon_max: -117.5,
                }),
            },
        ];
        let catalog = RasterMapCatalog {
            selected_map_id: selected.id.clone(),
            selected_map: Some(selected.clone()),
            available_maps: vec![selected.clone()],
            displayed_maps: vec![selected],
            geometry: RasterDisplayGeometry::default(),
            family_options: vec![],
        };
        let plan = raster_tile_plan(
            &catalog,
            &MapViewport {
                center: LatLon {
                    lat: 34.0,
                    lon: -118.0,
                },
                zoom: 9.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            800.0,
            600.0,
        );

        let action = plan.chart_reference_action.expect("reference action");
        assert_eq!(action.family_id, "tac");
        assert_eq!(action.suggested_chart_ids, vec!["la-one", "la-two"]);
    }
}
