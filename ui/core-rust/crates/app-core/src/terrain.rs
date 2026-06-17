use std::collections::BTreeSet;
use std::io::Read;

use serde::{Deserialize, Serialize};

const ABT2_MAGIC: &[u8; 4] = b"ABT2";
const GZIP_MAGIC: &[u8; 2] = b"\x1f\x8b";
const HEADER_BYTES: usize = 20;
const MIN_TERRAIN_ZOOM: u32 = 0;
const MAX_TERRAIN_ZOOM: u32 = product_contracts::TERRAIN_TER2_MAX_ZOOM;
const TERRAIN_COVERAGE_ZOOM: u32 = 10;
const TERRAIN_FULL_COVERAGE_ZOOM: u32 = 7;
const TERRAIN_ALTITUDE_BUCKET_FT: f64 = 200.0;
const TERRAIN_NODATA: i16 = -32768;
const TERRAIN_WIDE_BASE_ID: &str = "terrain-wide";
const WORLD_SIZE: f64 = 256.0;
const MAX_LATITUDE: f64 = 85.051_128_78;
const MAX_TERRAIN_TILES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TerrainProductCoverage {
    base_id: &'static str,
    x_min: u32,
    x_max: u32,
    y_tms_min: u32,
    y_tms_max: u32,
}

const TERRAIN_PRODUCTS: &[TerrainProductCoverage] = &[
    TerrainProductCoverage {
        base_id: "terrain-ak",
        x_min: 0,
        x_max: 153,
        y_tms_min: 681,
        y_tms_max: 803,
    },
    TerrainProductCoverage {
        base_id: "terrain-pac",
        x_min: 51,
        x_max: 79,
        y_tms_min: 564,
        y_tms_max: 582,
    },
    TerrainProductCoverage {
        base_id: "terrain-sw",
        x_min: 156,
        x_max: 219,
        y_tms_min: 555,
        y_tms_max: 636,
    },
    TerrainProductCoverage {
        base_id: "terrain-nw",
        x_min: 156,
        x_max: 219,
        y_tms_min: 637,
        y_tms_max: 676,
    },
    TerrainProductCoverage {
        base_id: "terrain-sc",
        x_min: 199,
        x_max: 256,
        y_tms_min: 555,
        y_tms_max: 625,
    },
    TerrainProductCoverage {
        base_id: "terrain-nc",
        x_min: 213,
        x_max: 256,
        y_tms_min: 626,
        y_tms_max: 676,
    },
    TerrainProductCoverage {
        base_id: "terrain-se",
        x_min: 257,
        x_max: 341,
        y_tms_min: 555,
        y_tms_max: 625,
    },
    TerrainProductCoverage {
        base_id: "terrain-ec",
        x_min: 257,
        x_max: 284,
        y_tms_min: 626,
        y_tms_max: 676,
    },
    TerrainProductCoverage {
        base_id: "terrain-ne",
        x_min: 285,
        x_max: 341,
        y_tms_min: 626,
        y_tms_max: 676,
    },
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainTileInfo {
    pub width: u16,
    pub height: u16,
    pub nodata: i16,
    pub scale: f32,
    pub offset: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainOverlayQueryResult {
    pub status: TerrainOverlayStatus,
    pub tile_requests: Vec<TerrainOverlayTileRequest>,
    pub altitude_bucket_ft: Option<f64>,
    pub frame_key: Option<String>,
    pub schedule: TerrainOverlayScheduleDecision,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainOverlayScheduleDecision {
    pub cached_count: usize,
    pub in_flight_count: usize,
    pub missing_count: usize,
    pub frame_complete: bool,
    pub work_batch: Vec<TerrainOverlayTileRequest>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TerrainOverlayStatus {
    Hidden,
    NoPosition,
    NoAltitude,
    TooManyTiles { count: usize },
    Unavailable { reason: String },
    Ready { count: usize },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainOverlayTileRequest {
    pub key: String,
    pub cache_key: String,
    pub product_id: String,
    pub path: String,
    pub source_tiles: Vec<TerrainOverlaySourceTile>,
    pub z: u32,
    pub x: u32,
    pub y_tms: u32,
    pub left: f64,
    pub top: f64,
    pub size: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainOverlaySourceTile {
    pub product_id: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<crate::CoreResourceRequest>,
}

pub fn query_terrain_overlay(
    viewport: &crate::MapViewport,
    width_px: f64,
    height_px: f64,
    has_position: bool,
    has_altitude: bool,
) -> TerrainOverlayQueryResult {
    query_terrain_overlay_with_available_packages(
        viewport,
        width_px,
        height_px,
        has_position,
        has_altitude,
        &all_terrain_package_ids(),
    )
}

pub fn query_terrain_overlay_with_available_packages(
    viewport: &crate::MapViewport,
    width_px: f64,
    height_px: f64,
    has_position: bool,
    has_altitude: bool,
    available_package_ids: &BTreeSet<String>,
) -> TerrainOverlayQueryResult {
    if !has_position {
        return TerrainOverlayQueryResult {
            status: TerrainOverlayStatus::NoPosition,
            tile_requests: Vec::new(),
            altitude_bucket_ft: None,
            frame_key: None,
            schedule: empty_terrain_overlay_schedule(),
        };
    }
    if !has_altitude {
        return TerrainOverlayQueryResult {
            status: TerrainOverlayStatus::NoAltitude,
            tile_requests: Vec::new(),
            altitude_bucket_ft: None,
            frame_key: None,
            schedule: empty_terrain_overlay_schedule(),
        };
    }
    let tile_requests = terrain_tile_requests_with_available_packages(
        viewport,
        width_px,
        height_px,
        available_package_ids,
    );
    let count = tile_requests.len();
    if count > MAX_TERRAIN_TILES {
        return TerrainOverlayQueryResult {
            status: TerrainOverlayStatus::TooManyTiles { count },
            tile_requests: Vec::new(),
            altitude_bucket_ft: None,
            frame_key: None,
            schedule: empty_terrain_overlay_schedule(),
        };
    }
    if count == 0 {
        return TerrainOverlayQueryResult {
            status: TerrainOverlayStatus::Unavailable {
                reason: "no installed terrain packages cover the viewport".to_string(),
            },
            tile_requests: Vec::new(),
            altitude_bucket_ft: None,
            frame_key: None,
            schedule: empty_terrain_overlay_schedule(),
        };
    }
    TerrainOverlayQueryResult {
        status: TerrainOverlayStatus::Ready { count },
        tile_requests,
        altitude_bucket_ft: None,
        frame_key: None,
        schedule: empty_terrain_overlay_schedule(),
    }
}

pub fn terrain_altitude_bucket_ft(altitude_ft: Option<f64>) -> Option<f64> {
    altitude_ft
        .filter(|altitude| altitude.is_finite())
        .map(|altitude| {
            (altitude / TERRAIN_ALTITUDE_BUCKET_FT).round() * TERRAIN_ALTITUDE_BUCKET_FT
        })
}

pub fn prepare_terrain_overlay_frame(
    query: &mut TerrainOverlayQueryResult,
    altitude_ft: Option<f64>,
    sort_origin: Option<crate::LatLon>,
    viewport: &crate::MapViewport,
    width_px: f64,
    height_px: f64,
) {
    let altitude_bucket_ft = terrain_altitude_bucket_ft(altitude_ft);
    query.altitude_bucket_ft = altitude_bucket_ft;
    if let Some(origin) = sort_origin {
        sort_terrain_tile_requests_by_distance(query, origin, viewport, width_px, height_px);
    }
    for request in &mut query.tile_requests {
        request.cache_key = terrain_cache_key(&request.key, altitude_bucket_ft);
    }
    query.frame_key = if matches!(query.status, TerrainOverlayStatus::Ready { .. }) {
        Some(terrain_frame_key(&query.tile_requests, altitude_bucket_ft))
    } else {
        None
    };
}

pub fn schedule_terrain_overlay_frame(
    query: &mut TerrainOverlayQueryResult,
    decoded_cache_keys: &BTreeSet<String>,
    in_flight_cache_keys: &BTreeSet<String>,
) {
    if !matches!(query.status, TerrainOverlayStatus::Ready { .. }) {
        query.schedule = empty_terrain_overlay_schedule();
        return;
    }
    let mut cached_count = 0;
    let mut in_flight_count = 0;
    let mut missing_count = 0;
    let mut work_batch = Vec::new();
    for request in &query.tile_requests {
        if decoded_cache_keys.contains(&request.cache_key) {
            cached_count += 1;
        } else if in_flight_cache_keys.contains(&request.cache_key) {
            in_flight_count += 1;
        } else {
            missing_count += 1;
            work_batch.push(request.clone());
        }
    }
    query.schedule = TerrainOverlayScheduleDecision {
        cached_count,
        in_flight_count,
        missing_count,
        frame_complete: cached_count == query.tile_requests.len(),
        work_batch,
    };
}

pub fn render_terrain_warning_png(
    tile_bytes: &[u8],
    aircraft_altitude_ft: f64,
) -> Result<Vec<u8>, String> {
    let (info, rgba) = render_terrain_warning_rgba(tile_bytes, aircraft_altitude_ft)?;
    encode_terrain_warning_png(&info, &rgba)
}

pub fn render_terrain_warning_png_from_tiles(
    tile_bytes_list: &[&[u8]],
    aircraft_altitude_ft: f64,
) -> Result<Vec<u8>, String> {
    let (info, rgba) =
        render_terrain_warning_rgba_from_tiles(tile_bytes_list, aircraft_altitude_ft)?;
    encode_terrain_warning_png(&info, &rgba)
}

pub fn render_terrain_warning_rgba_from_tiles(
    tile_bytes_list: &[&[u8]],
    aircraft_altitude_ft: f64,
) -> Result<(TerrainTileInfo, Vec<u8>), String> {
    if tile_bytes_list.is_empty() {
        return Err("no terrain source tiles supplied".to_string());
    }
    let parsed_tiles = tile_bytes_list
        .iter()
        .map(|tile_bytes| parse_abt2_tile(tile_bytes))
        .collect::<Result<Vec<_>, _>>()?;
    let (info, samples) = composite_terrain_samples(&parsed_tiles)?;
    let rgba = render_terrain_warning_samples(&info, &samples, aircraft_altitude_ft);
    Ok((info, rgba))
}

pub fn render_terrain_warning_raw_rgba_from_tiles(
    tile_bytes_list: &[&[u8]],
    aircraft_altitude_ft: f64,
) -> Result<Vec<u8>, String> {
    let (info, rgba) =
        render_terrain_warning_rgba_from_tiles(tile_bytes_list, aircraft_altitude_ft)?;
    Ok(pack_raw_rgba(info.width, info.height, &rgba))
}

fn encode_terrain_warning_png(info: &TerrainTileInfo, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, info.width as u32, info.height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::NoCompression);
        encoder.set_filter(png::Filter::NoFilter);
        let mut writer = encoder.write_header().map_err(|err| err.to_string())?;
        writer
            .write_image_data(&rgba)
            .map_err(|err| err.to_string())?;
    }
    Ok(png_bytes)
}

pub fn render_terrain_warning_rgba(
    tile_bytes: &[u8],
    aircraft_altitude_ft: f64,
) -> Result<(TerrainTileInfo, Vec<u8>), String> {
    let (info, samples) = parse_abt2_tile(tile_bytes)?;
    Ok((
        info.clone(),
        render_terrain_warning_samples(&info, &samples, aircraft_altitude_ft),
    ))
}

fn render_terrain_warning_samples(
    info: &TerrainTileInfo,
    samples: &[i16],
    aircraft_altitude_ft: f64,
) -> Vec<u8> {
    let width = info.width as usize;
    let height = info.height as usize;
    let mut rgba = vec![0_u8; width * height * 4];
    for (index, sample) in samples.iter().copied().enumerate() {
        let pixel = index * 4;
        if sample == info.nodata {
            let x = index % width;
            let y = index / width;
            if (x + y) % 12 < 6 {
                rgba[pixel] = 0;
                rgba[pixel + 1] = 82;
                rgba[pixel + 2] = 150;
                rgba[pixel + 3] = 70;
            }
            continue;
        }
        let elevation_ft = sample as f64 * info.scale as f64 + info.offset as f64;
        let clearance_ft = aircraft_altitude_ft - elevation_ft;
        if clearance_ft <= 0.0 {
            rgba[pixel] = 185;
            rgba[pixel + 1] = 0;
            rgba[pixel + 2] = 45;
            rgba[pixel + 3] = 190;
        } else if clearance_ft <= 1000.0 {
            rgba[pixel] = 255;
            rgba[pixel + 1] = 220;
            rgba[pixel + 2] = 0;
            rgba[pixel + 3] = 125;
        }
    }
    rgba
}

fn pack_raw_rgba(width: u16, height: u16, rgba: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(4 + rgba.len());
    output.extend_from_slice(&width.to_le_bytes());
    output.extend_from_slice(&height.to_le_bytes());
    output.extend_from_slice(rgba);
    output
}

fn composite_terrain_samples(
    parsed_tiles: &[(TerrainTileInfo, Vec<i16>)],
) -> Result<(TerrainTileInfo, Vec<i16>), String> {
    let (first_info, first_samples) = parsed_tiles
        .first()
        .ok_or_else(|| "no terrain source tiles supplied".to_string())?;
    let sample_count = first_samples.len();
    let mut composite = vec![first_info.nodata; sample_count];
    let mut composite_elevations = vec![f64::NEG_INFINITY; sample_count];

    for (info, samples) in parsed_tiles {
        if info.width != first_info.width || info.height != first_info.height {
            return Err("terrain source tile dimensions do not match".to_string());
        }
        if samples.len() != sample_count {
            return Err("terrain source tile sample counts do not match".to_string());
        }
        for (index, sample) in samples.iter().copied().enumerate() {
            if sample == info.nodata {
                continue;
            }
            let elevation_ft = sample as f64 * info.scale as f64 + info.offset as f64;
            if elevation_ft > composite_elevations[index] {
                let first_scale = first_info.scale as f64;
                let first_offset = first_info.offset as f64;
                let encoded_sample = ((elevation_ft - first_offset) / first_scale)
                    .round()
                    .clamp(i16::MIN as f64, i16::MAX as f64)
                    as i16;
                composite[index] = encoded_sample;
                composite_elevations[index] = elevation_ft;
            }
        }
    }

    Ok((first_info.clone(), composite))
}

pub fn parse_abt2_tile(tile_bytes: &[u8]) -> Result<(TerrainTileInfo, Vec<i16>), String> {
    if tile_bytes.len() < HEADER_BYTES || &tile_bytes[0..4] != ABT2_MAGIC {
        return Err("invalid ABT2 terrain tile header".to_string());
    }
    let width = u16::from_le_bytes([tile_bytes[4], tile_bytes[5]]);
    let height = u16::from_le_bytes([tile_bytes[6], tile_bytes[7]]);
    let nodata = i16::from_le_bytes([tile_bytes[8], tile_bytes[9]]);
    let _reserved = i16::from_le_bytes([tile_bytes[10], tile_bytes[11]]);
    let scale = f32::from_le_bytes([
        tile_bytes[12],
        tile_bytes[13],
        tile_bytes[14],
        tile_bytes[15],
    ]);
    let offset = f32::from_le_bytes([
        tile_bytes[16],
        tile_bytes[17],
        tile_bytes[18],
        tile_bytes[19],
    ]);
    if nodata != TERRAIN_NODATA
        || scale != product_contracts::TERRAIN_TER2_HEIGHT_QUANTIZATION_FT as f32
        || offset != 0.0
    {
        return Err("ABT2 terrain tile header does not match TER2 contract".to_string());
    }
    let sample_count = width as usize * height as usize;
    let expected_bytes = HEADER_BYTES + sample_count * 2;
    if tile_bytes.len() != expected_bytes {
        return Err(format!(
            "invalid ABT2 terrain tile length: expected {expected_bytes} bytes, got {}",
            tile_bytes.len()
        ));
    }
    let mut samples = Vec::with_capacity(sample_count);
    for (index, chunk) in tile_bytes[HEADER_BYTES..expected_bytes]
        .chunks_exact(2)
        .enumerate()
    {
        let residual = u16::from_le_bytes([chunk[0], chunk[1]]);
        let x = index % width as usize;
        let y = index / width as usize;
        let prediction = match (x, y) {
            (0, 0) => 0_u16,
            (_, 0) => samples[index - 1] as u16,
            (0, _) => samples[index - width as usize] as u16,
            _ => (samples[index - 1] as u16)
                .wrapping_add(samples[index - width as usize] as u16)
                .wrapping_sub(samples[index - width as usize - 1] as u16),
        };
        samples.push(prediction.wrapping_add(residual) as i16);
    }
    Ok((
        TerrainTileInfo {
            width,
            height,
            nodata,
            scale,
            offset,
        },
        samples,
    ))
}

pub(crate) fn terrain_source_payload_to_abt2_bytes(payload: &[u8]) -> Result<Vec<u8>, String> {
    if !payload.starts_with(GZIP_MAGIC) {
        return Err("terrain source payload is not gzip-compressed ABT2".to_string());
    }
    let mut decoder = flate2::read::GzDecoder::new(payload);
    let mut abt2_bytes = Vec::new();
    decoder
        .read_to_end(&mut abt2_bytes)
        .map_err(|err| format!("failed to gzip-decode terrain tile payload: {err}"))?;
    parse_abt2_tile(&abt2_bytes)?;
    Ok(abt2_bytes)
}

fn terrain_tile_requests_with_available_packages(
    viewport: &crate::MapViewport,
    width_px: f64,
    height_px: f64,
    available_package_ids: &BTreeSet<String>,
) -> Vec<TerrainOverlayTileRequest> {
    if width_px <= 0.0 || height_px <= 0.0 {
        return Vec::new();
    }
    let center_world = lat_lon_to_world(viewport.center.lat, viewport.center.lon);
    let scale = scale_for_zoom(viewport.zoom);
    let min_world_x = center_world.0 - width_px / 2.0 / scale;
    let max_world_x = center_world.0 + width_px / 2.0 / scale;
    let min_world_y = center_world.1 - height_px / 2.0 / scale;
    let max_world_y = center_world.1 + height_px / 2.0 / scale;
    let terrain_zoom = terrain_zoom_for_viewport(viewport.zoom);
    let tiles_at_zoom = 2_u32.pow(terrain_zoom);
    let tile_world_size = WORLD_SIZE / tiles_at_zoom as f64;
    let tile_screen_size = tile_world_size * scale;
    let x_start = (min_world_x / tile_world_size).floor().max(0.0) as u32;
    let x_end = (max_world_x / tile_world_size)
        .floor()
        .min((tiles_at_zoom - 1) as f64)
        .max(0.0) as u32;
    let y_start = (min_world_y / tile_world_size).floor().max(0.0) as u32;
    let y_end = (max_world_y / tile_world_size)
        .floor()
        .min((tiles_at_zoom - 1) as f64)
        .max(0.0) as u32;
    if x_end < x_start || y_end < y_start {
        return Vec::new();
    }
    let mut requests = Vec::new();
    for y_xyz in y_start..=y_end {
        for x in x_start..=x_end {
            let y_tms = (tiles_at_zoom - 1) - y_xyz;
            let product_ids = terrain_product_ids_for_tile_with_available_packages(
                terrain_zoom,
                x,
                y_tms,
                available_package_ids,
            );
            if let Some(product_id) = product_ids.first() {
                let path = format!("tiles/{terrain_zoom}/{x}/{y_tms}.terrain");
                let key = format!("terrain/{path}");
                requests.push(TerrainOverlayTileRequest {
                    cache_key: terrain_cache_key(&key, None),
                    key,
                    product_id: product_id.clone(),
                    path: path.clone(),
                    source_tiles: product_ids
                        .iter()
                        .map(|source_product_id| TerrainOverlaySourceTile {
                            product_id: source_product_id.clone(),
                            path: path.clone(),
                            resource: None,
                        })
                        .collect(),
                    z: terrain_zoom,
                    x,
                    y_tms,
                    left: (x as f64 * tile_world_size - center_world.0) * scale + width_px / 2.0,
                    top: (y_xyz as f64 * tile_world_size - center_world.1) * scale
                        + height_px / 2.0,
                    size: tile_screen_size,
                });
            }
        }
    }
    requests
}

fn terrain_cache_key(tile_key: &str, altitude_bucket_ft: Option<f64>) -> String {
    match altitude_bucket_ft {
        Some(altitude_bucket_ft) => format!("{tile_key}@{altitude_bucket_ft:.0}"),
        None => format!("{tile_key}@no-alt"),
    }
}

fn empty_terrain_overlay_schedule() -> TerrainOverlayScheduleDecision {
    TerrainOverlayScheduleDecision {
        cached_count: 0,
        in_flight_count: 0,
        missing_count: 0,
        frame_complete: false,
        work_batch: Vec::new(),
    }
}

fn terrain_frame_key(
    requests: &[TerrainOverlayTileRequest],
    altitude_bucket_ft: Option<f64>,
) -> String {
    let altitude = altitude_bucket_ft
        .map(|altitude| format!("{altitude:.0}"))
        .unwrap_or_else(|| "no-alt".to_string());
    let tile_keys = requests
        .iter()
        .map(|request| request.key.as_str())
        .collect::<Vec<_>>()
        .join("|");
    format!("{altitude}:{tile_keys}")
}

fn sort_terrain_tile_requests_by_distance(
    query: &mut TerrainOverlayQueryResult,
    origin: crate::LatLon,
    viewport: &crate::MapViewport,
    width_px: f64,
    height_px: f64,
) {
    if width_px <= 0.0 || height_px <= 0.0 {
        return;
    }
    let center_world = lat_lon_to_world(viewport.center.lat, viewport.center.lon);
    let origin_world = lat_lon_to_world(origin.lat, origin.lon);
    let scale = scale_for_zoom(viewport.zoom);
    let origin_x = (origin_world.0 - center_world.0) * scale + width_px / 2.0;
    let origin_y = (origin_world.1 - center_world.1) * scale + height_px / 2.0;
    query.tile_requests.sort_by(|left, right| {
        let left_distance = terrain_request_distance_squared(left, origin_x, origin_y);
        let right_distance = terrain_request_distance_squared(right, origin_x, origin_y);
        left_distance.total_cmp(&right_distance)
    });
}

fn terrain_request_distance_squared(
    request: &TerrainOverlayTileRequest,
    target_x: f64,
    target_y: f64,
) -> f64 {
    let center_x = request.left + request.size / 2.0;
    let center_y = request.top + request.size / 2.0;
    (center_x - target_x).powi(2) + (center_y - target_y).powi(2)
}

fn terrain_zoom_for_viewport(view_zoom: f64) -> u32 {
    (view_zoom - 1.0)
        .floor()
        .clamp(MIN_TERRAIN_ZOOM as f64, MAX_TERRAIN_ZOOM as f64) as u32
}

#[cfg(test)]
fn terrain_product_ids_for_tile(zoom: u32, x: u32, y_tms: u32) -> Vec<String> {
    let available = all_terrain_package_ids();
    terrain_product_ids_for_tile_with_available_packages(zoom, x, y_tms, &available)
}

fn terrain_product_ids_for_tile_with_available_packages(
    zoom: u32,
    x: u32,
    y_tms: u32,
    available_package_ids: &BTreeSet<String>,
) -> Vec<String> {
    if zoom <= TERRAIN_FULL_COVERAGE_ZOOM {
        let wide_package_id = terrain_package_id(TERRAIN_WIDE_BASE_ID);
        if available_package_ids.contains(&wide_package_id) {
            return vec![wide_package_id];
        }
        return Vec::new();
    }

    let zoom_delta = TERRAIN_COVERAGE_ZOOM.saturating_sub(zoom);
    let x_min = x << zoom_delta;
    let x_max = ((x + 1) << zoom_delta).saturating_sub(1);
    let y_tms_min = y_tms << zoom_delta;
    let y_tms_max = ((y_tms + 1) << zoom_delta).saturating_sub(1);

    let mut products = TERRAIN_PRODUCTS
        .iter()
        .filter_map(|coverage| {
            let product_id = terrain_package_id(coverage.base_id);
            let overlap_x_min = coverage.x_min.max(x_min);
            let overlap_x_max = coverage.x_max.min(x_max);
            let overlap_y_min = coverage.y_tms_min.max(y_tms_min);
            let overlap_y_max = coverage.y_tms_max.min(y_tms_max);
            if overlap_x_min > overlap_x_max || overlap_y_min > overlap_y_max {
                return None;
            }
            if !available_package_ids.contains(&product_id) {
                return None;
            }
            let overlap_x = overlap_x_max - overlap_x_min + 1;
            let overlap_y = overlap_y_max - overlap_y_min + 1;
            Some((u64::from(overlap_x) * u64::from(overlap_y), product_id))
        })
        .collect::<Vec<_>>();
    products.sort_by(|(left_area, left_id), (right_area, right_id)| {
        right_area
            .cmp(left_area)
            .then_with(|| left_id.cmp(right_id))
    });
    products
        .into_iter()
        .map(|(_, product_id)| product_id)
        .collect()
}

fn all_terrain_package_ids() -> BTreeSet<String> {
    std::iter::once(terrain_package_id(TERRAIN_WIDE_BASE_ID))
        .chain(
            TERRAIN_PRODUCTS
                .iter()
                .map(|coverage| terrain_package_id(coverage.base_id)),
        )
        .collect()
}

fn terrain_package_id(base_id: &str) -> String {
    format!("{base_id}_{}", product_contracts::TERRAIN_CONTRACT_ID)
}

fn scale_for_zoom(zoom: f64) -> f64 {
    2.0_f64.powf(zoom)
}

fn lat_lon_to_world(lat: f64, lon: f64) -> (f64, f64) {
    let clamped_lat = lat.clamp(-MAX_LATITUDE, MAX_LATITUDE);
    let lat_rad = clamped_lat.to_radians();
    (
        ((lon + 180.0) / 360.0) * WORLD_SIZE,
        ((1.0 - lat_rad.tan().asinh() / std::f64::consts::PI) / 2.0) * WORLD_SIZE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package_ids(base_ids: &[&str]) -> Vec<String> {
        base_ids
            .iter()
            .map(|base_id| terrain_package_id(base_id))
            .collect()
    }

    fn package_id_set(base_ids: &[&str]) -> BTreeSet<String> {
        package_ids(base_ids).into_iter().collect()
    }

    fn terrain_tile_bytes(width: u16, height: u16, samples: &[i16]) -> Vec<u8> {
        assert_eq!(samples.len(), width as usize * height as usize);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"ABT2");
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes.extend_from_slice(&TERRAIN_NODATA.to_le_bytes());
        bytes.extend_from_slice(&0_i16.to_le_bytes());
        bytes.extend_from_slice(
            &(product_contracts::TERRAIN_TER2_HEIGHT_QUANTIZATION_FT as f32).to_le_bytes(),
        );
        bytes.extend_from_slice(&0.0_f32.to_le_bytes());
        for (index, sample) in samples.iter().copied().enumerate() {
            let x = index % width as usize;
            let y = index / width as usize;
            let prediction = match (x, y) {
                (0, 0) => 0_u16,
                (_, 0) => samples[index - 1] as u16,
                (0, _) => samples[index - width as usize] as u16,
                _ => (samples[index - 1] as u16)
                    .wrapping_add(samples[index - width as usize] as u16)
                    .wrapping_sub(samples[index - width as usize - 1] as u16),
            };
            let residual = (sample as u16).wrapping_sub(prediction);
            bytes.extend_from_slice(&residual.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn classifies_abt2_tile_against_aircraft_altitude() {
        let bytes = terrain_tile_bytes(2, 2, &[16, 25, 41, TERRAIN_NODATA]);

        let (_, rgba) = render_terrain_warning_rgba(&bytes, 2000.0).expect("render terrain");
        assert_eq!(&rgba[0..4], &[255, 220, 0, 125]);
        assert_eq!(&rgba[4..8], &[255, 220, 0, 125]);
        assert_eq!(&rgba[8..12], &[185, 0, 45, 190]);
        assert_eq!(&rgba[12..16], &[0, 82, 150, 70]);
    }

    #[test]
    fn decodes_gzip_terrain_source_payload_to_abt2() {
        use std::io::Write;

        let bytes = terrain_tile_bytes(1, 1, &[40]);

        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&bytes).expect("write gzip payload");
        let gzip_bytes = encoder.finish().expect("finish gzip payload");

        assert_eq!(
            terrain_source_payload_to_abt2_bytes(&gzip_bytes).expect("decode terrain payload"),
            bytes
        );
        assert!(terrain_source_payload_to_abt2_bytes(&bytes).is_err());
    }

    #[test]
    fn prepares_core_owned_terrain_frame_identity() {
        let request = |key: &str, left: f64, top: f64| TerrainOverlayTileRequest {
            key: key.to_string(),
            cache_key: String::new(),
            product_id: "terrain-sw".to_string(),
            path: key.strip_prefix("terrain/").unwrap_or(key).to_string(),
            source_tiles: Vec::new(),
            z: 9,
            x: 0,
            y_tms: 0,
            left,
            top,
            size: 10.0,
        };
        let mut query = TerrainOverlayQueryResult {
            status: TerrainOverlayStatus::Ready { count: 2 },
            tile_requests: vec![
                request("terrain/tiles/9/1/2.terrain", 1_000.0, 1_000.0),
                request("terrain/tiles/9/3/4.terrain", 45.0, 45.0),
            ],
            altitude_bucket_ft: None,
            frame_key: None,
            schedule: empty_terrain_overlay_schedule(),
        };
        prepare_terrain_overlay_frame(
            &mut query,
            Some(1_190.0),
            Some(crate::LatLon { lat: 0.0, lon: 0.0 }),
            &crate::MapViewport {
                center: crate::LatLon { lat: 0.0, lon: 0.0 },
                zoom: 0.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            100.0,
            100.0,
        );

        assert_eq!(query.altitude_bucket_ft, Some(1_200.0));
        assert_eq!(query.tile_requests[0].key, "terrain/tiles/9/3/4.terrain");
        assert_eq!(
            query.tile_requests[0].cache_key,
            "terrain/tiles/9/3/4.terrain@1200"
        );
        assert_eq!(
            query.frame_key.as_deref(),
            Some("1200:terrain/tiles/9/3/4.terrain|terrain/tiles/9/1/2.terrain")
        );

        let mut decoded_cache_keys = BTreeSet::new();
        decoded_cache_keys.insert("terrain/tiles/9/3/4.terrain@1200".to_string());
        let in_flight_cache_keys = BTreeSet::new();
        schedule_terrain_overlay_frame(&mut query, &decoded_cache_keys, &in_flight_cache_keys);

        assert_eq!(query.schedule.cached_count, 1);
        assert_eq!(query.schedule.in_flight_count, 0);
        assert_eq!(query.schedule.missing_count, 1);
        assert!(!query.schedule.frame_complete);
        assert_eq!(
            query
                .schedule
                .work_batch
                .iter()
                .map(|request| request.key.as_str())
                .collect::<Vec<_>>(),
            vec!["terrain/tiles/9/1/2.terrain"]
        );
    }

    #[test]
    fn writes_warning_png() {
        let bytes = terrain_tile_bytes(1, 1, &[40]);

        let png = render_terrain_warning_png(&bytes, 2000.0).expect("render png");
        assert_eq!(&png[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }

    #[test]
    fn selects_southwest_terrain_for_palo_alto_tile() {
        assert_eq!(
            terrain_product_ids_for_tile(9, 82, 313),
            package_ids(&["terrain-sw"])
        );
    }

    #[test]
    fn selects_view_zoom_instead_of_always_base_zoom() {
        assert_eq!(terrain_zoom_for_viewport(8.4), 7);
        assert_eq!(terrain_zoom_for_viewport(8.6), 7);
        assert_eq!(terrain_zoom_for_viewport(10.0), 9);
        assert_eq!(terrain_zoom_for_viewport(12.0), MAX_TERRAIN_ZOOM);
    }

    #[test]
    fn selects_southwest_terrain_for_palo_alto_parent_tile() {
        assert_eq!(
            terrain_product_ids_for_tile(8, 41, 156),
            package_ids(&["terrain-sw"])
        );
    }

    #[test]
    fn low_zoom_terrain_uses_family_wide_package() {
        assert_eq!(
            terrain_product_ids_for_tile(7, 27, 78),
            package_ids(&["terrain-wide"])
        );
        assert_eq!(
            terrain_product_ids_for_tile(6, 13, 39),
            package_ids(&["terrain-wide"])
        );
    }

    #[test]
    fn low_zoom_terrain_requires_wide_package_even_when_regions_exist() {
        let available = package_id_set(&["terrain-nw"]);
        assert!(
            terrain_product_ids_for_tile_with_available_packages(6, 13, 39, &available).is_empty()
        );
    }

    #[test]
    fn available_wide_package_covers_distant_regions_at_low_zoom() {
        let available = package_id_set(&["terrain-wide", "terrain-nw"]);
        assert_eq!(
            terrain_product_ids_for_tile_with_available_packages(6, 13, 39, &available),
            package_ids(&["terrain-wide"])
        );
        assert_eq!(
            terrain_product_ids_for_tile_with_available_packages(6, 19, 39, &available),
            package_ids(&["terrain-wide"])
        );
    }

    #[test]
    fn high_zoom_terrain_uses_only_available_regional_packages() {
        let available = package_id_set(&["terrain-wide", "terrain-nw"]);
        assert_eq!(
            terrain_product_ids_for_tile_with_available_packages(8, 41, 164, &available),
            package_ids(&["terrain-nw"])
        );
        assert!(
            terrain_product_ids_for_tile_with_available_packages(8, 76, 164, &available).is_empty()
        );
    }

    #[test]
    fn terrain_overlay_reports_unavailable_without_installed_package() {
        let viewport = crate::MapViewport {
            center: crate::LatLon {
                lat: 37.5,
                lon: -122.1,
            },
            zoom: 12.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let result = query_terrain_overlay_with_available_packages(
            &viewport,
            1024.0,
            1024.0,
            true,
            true,
            &BTreeSet::new(),
        );
        assert!(matches!(
            result.status,
            TerrainOverlayStatus::Unavailable { .. }
        ));
        assert!(result.tile_requests.is_empty());
    }

    #[test]
    fn terrain_overlay_uses_installed_package_availability() {
        let viewport = crate::MapViewport {
            center: crate::LatLon {
                lat: 37.5,
                lon: -122.1,
            },
            zoom: 12.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        let result = query_terrain_overlay_with_available_packages(
            &viewport,
            1024.0,
            1024.0,
            true,
            true,
            &package_id_set(&["terrain-sw"]),
        );
        assert!(matches!(result.status, TerrainOverlayStatus::Ready { .. }));
        assert!(result.tile_requests.iter().all(|request| request
            .source_tiles
            .iter()
            .all(|source| source.product_id == terrain_package_id("terrain-sw"))));
    }

    #[test]
    fn orders_overlapping_high_zoom_parent_tile_by_dominant_product() {
        assert_eq!(
            terrain_product_ids_for_tile(8, 54, 156),
            package_ids(&["terrain-sw", "terrain-nc", "terrain-sc"])
        );
    }

    #[test]
    fn composites_overlapping_terrain_tiles_by_valid_highest_sample() {
        let tile = |samples: [i16; 4]| terrain_tile_bytes(2, 2, &samples);
        let southwest = tile([2, TERRAIN_NODATA, 5, TERRAIN_NODATA]);
        let northwest = tile([TERRAIN_NODATA, 3, 4, TERRAIN_NODATA]);

        let (info, samples) = composite_terrain_samples(&[
            parse_abt2_tile(&southwest).expect("parse southwest"),
            parse_abt2_tile(&northwest).expect("parse northwest"),
        ])
        .expect("composite terrain");

        assert_eq!(info.nodata, TERRAIN_NODATA);
        assert_eq!(samples, vec![2, 3, 5, TERRAIN_NODATA]);
    }
}
