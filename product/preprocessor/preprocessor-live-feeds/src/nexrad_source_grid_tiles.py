import argparse
import gzip
import hashlib
import json
import shutil
from pathlib import Path

import numpy as np
from osgeo import gdal
from PIL import Image, ImageDraw

TRANSPARENT_INDEX = 0
POOR_COLOR_MATCH_THRESHOLD = 8


def open_source(source_gz, output_dir):
    tif_path = output_dir / 'source.tif'
    with gzip.open(source_gz, 'rb') as src, open(tif_path, 'wb') as dst:
        shutil.copyfileobj(src, dst)
    dataset = gdal.Open(str(tif_path))
    if dataset is None:
        raise SystemExit(f'failed to open {tif_path}')
    return dataset


def band_to_uint8(values):
    if values.dtype == np.uint8:
        return values
    values = values.astype(np.float32)
    max_value = float(np.nanmax(values)) if values.size else 0.0
    if max_value <= 255.0:
        return np.clip(values, 0, 255).astype(np.uint8)
    return np.clip(values * (255.0 / max_value), 0, 255).astype(np.uint8)


def read_rgba(dataset):
    if dataset.RasterCount == 1:
        band = dataset.GetRasterBand(1)
        values = band.ReadAsArray()
        color_table = band.GetRasterColorTable()
        if color_table is not None:
            count = color_table.GetCount()
            lut = np.zeros((max(count, 256), 4), dtype=np.uint8)
            for index in range(count):
                red, green, blue, alpha = color_table.GetColorEntry(index)
                lut[index] = [red, green, blue, alpha]
            indices = np.clip(values.astype(np.int64), 0, lut.shape[0] - 1)
            rgba = lut[indices]
        else:
            gray = band_to_uint8(values)
            alpha = np.where(gray == 0, 0, 255).astype(np.uint8)
            rgba = np.dstack([gray, gray, gray, alpha])
    else:
        red = band_to_uint8(dataset.GetRasterBand(1).ReadAsArray())
        green = band_to_uint8(dataset.GetRasterBand(2).ReadAsArray())
        blue = band_to_uint8(dataset.GetRasterBand(3).ReadAsArray())
        if dataset.RasterCount >= 4:
            alpha = band_to_uint8(dataset.GetRasterBand(4).ReadAsArray())
        else:
            alpha = np.full(red.shape, 255, dtype=np.uint8)
        blank = (red == 0) & (green == 0) & (blue == 0)
        alpha = np.where(blank, 0, alpha).astype(np.uint8)
        rgba = np.dstack([red, green, blue, alpha])
    return np.ascontiguousarray(rgba, dtype=np.uint8)


def load_fixed_palette(path):
    opaque_palette = np.asarray(json.loads(Path(path).read_text()), dtype=np.uint8)
    if opaque_palette.shape != (255, 3):
        raise SystemExit(f'expected 255 RGB palette entries in {path}, got {opaque_palette.shape}')
    palette = np.zeros((256, 3), dtype=np.uint8)
    palette[1:, :] = opaque_palette
    flat_palette = palette.reshape(-1).tolist()
    transparency = bytes([0] + [255] * 255)
    palette_sha256 = hashlib.sha256(Path(path).read_bytes()).hexdigest()
    return palette, flat_palette, transparency, palette_sha256


def quantize_rgba_to_fixed_palette_indices(rgba, palette):
    indices = np.zeros(rgba.shape[:2], dtype=np.uint8)
    opaque = rgba[:, :, 3] > 0
    if not np.any(opaque):
        return indices, {
            'palette_error_max': 0.0,
            'palette_error_p95': 0.0,
            'poor_color_match_count': 0,
        }
    colors = rgba[:, :, :3][opaque]
    unique, inverse = np.unique(colors.reshape(-1, 3), axis=0, return_inverse=True)
    palette_i16 = palette[1:, :].astype(np.int16)
    unique_i16 = unique.astype(np.int16)
    distances = np.max(np.abs(unique_i16[:, None, :] - palette_i16[None, :, :]), axis=2)
    nearest = np.argmin(distances, axis=1)
    nearest_errors = distances[np.arange(distances.shape[0]), nearest]
    pixel_errors = nearest_errors[inverse]
    mapped_unique = (nearest + 1).astype(np.uint8)
    indices[opaque] = mapped_unique[inverse]
    return indices, {
        'palette_error_max': float(np.max(pixel_errors)),
        'palette_error_p95': float(np.percentile(pixel_errors, 95)),
        'poor_color_match_count': int(np.count_nonzero(pixel_errors > POOR_COLOR_MATCH_THRESHOLD)),
    }


def save_fixed_palette_png(indices, path, palette):
    used = np.unique(indices)
    if used.size == 0:
        used = np.asarray([TRANSPARENT_INDEX], dtype=np.uint8)
    if TRANSPARENT_INDEX in used:
        local_indices = [TRANSPARENT_INDEX] + [int(index) for index in used if int(index) != TRANSPARENT_INDEX]
    else:
        local_indices = [int(index) for index in used]
    remap = np.zeros(256, dtype=np.uint8)
    for local_index, global_index in enumerate(local_indices):
        remap[global_index] = local_index
    compact = remap[indices]
    local_palette = palette[np.asarray(local_indices, dtype=np.uint8)]
    flat_palette = local_palette.reshape(-1).tolist()
    transparency = None
    if local_indices and local_indices[0] == TRANSPARENT_INDEX:
        transparency = bytes([0] + [255] * (len(local_indices) - 1))

    path.parent.mkdir(parents=True, exist_ok=True)
    image = Image.fromarray(compact, 'P')
    image.putpalette(flat_palette)
    if transparency is None:
        image.save(path, 'PNG', optimize=True)
    else:
        image.save(path, 'PNG', optimize=True, transparency=transparency)


def pixel_for_lon_lat(lon, lat, geo_transform):
    origin_lon, pixel_lon, rot_x, origin_lat, rot_y, pixel_lat = geo_transform
    if abs(rot_x) > 1e-12 or abs(rot_y) > 1e-12:
        raise SystemExit('debug lat-lon grid only supports north-up source grids')
    if pixel_lon == 0 or pixel_lat == 0:
        raise SystemExit('debug lat-lon grid source has invalid geotransform')
    return (
        (lon - origin_lon) / pixel_lon,
        (lat - origin_lat) / pixel_lat,
    )


def composite_lat_lon_grid_under_radar(rgba, geo_transform):
    height, width = rgba.shape[:2]
    origin_lon, pixel_lon, _rot_x, origin_lat, _rot_y, pixel_lat = geo_transform
    east_lon = origin_lon + pixel_lon * width
    south_lat = origin_lat + pixel_lat * height
    west = min(origin_lon, east_lon)
    east = max(origin_lon, east_lon)
    south = min(origin_lat, south_lat)
    north = max(origin_lat, south_lat)
    grid = np.zeros((height, width, 4), dtype=np.uint8)
    lon_start = int(np.ceil(west))
    lon_end = int(np.floor(east))
    lat_start = int(np.ceil(south))
    lat_end = int(np.floor(north))
    major_lon = (255, 255, 255, 210)
    minor_lon = (30, 80, 255, 190)
    major_lat = (255, 255, 255, 210)
    minor_lat = (255, 70, 20, 190)

    def paint_boundary_column(x, color, width_px):
        half = width_px / 2.0
        start = max(0, int(np.floor(x - half)))
        end = min(width, int(np.ceil(x + half)))
        for col in range(start, end):
            coverage = max(0.0, min(col + 1.0, x + half) - max(col, x - half))
            if coverage <= 0.0:
                continue
            alpha = int(round(color[3] * min(coverage, 1.0)))
            stronger = alpha > grid[:, col, 3]
            grid[stronger, col, 0] = color[0]
            grid[stronger, col, 1] = color[1]
            grid[stronger, col, 2] = color[2]
            grid[stronger, col, 3] = alpha

    def paint_boundary_row(y, color, width_px):
        half = width_px / 2.0
        start = max(0, int(np.floor(y - half)))
        end = min(height, int(np.ceil(y + half)))
        for row in range(start, end):
            coverage = max(0.0, min(row + 1.0, y + half) - max(row, y - half))
            if coverage <= 0.0:
                continue
            alpha = int(round(color[3] * min(coverage, 1.0)))
            stronger = alpha > grid[row, :, 3]
            grid[row, stronger, 0] = color[0]
            grid[row, stronger, 1] = color[1]
            grid[row, stronger, 2] = color[2]
            grid[row, stronger, 3] = alpha

    for lon in range(lon_start, lon_end + 1):
        x, _y = pixel_for_lon_lat(lon, south, geo_transform)
        color = major_lon if lon % 5 == 0 else minor_lon
        width_px = 3 if lon % 5 == 0 else 1
        paint_boundary_column(x, color, width_px)
    for lat in range(lat_start, lat_end + 1):
        _x, y = pixel_for_lon_lat(west, lat, geo_transform)
        color = major_lat if lat % 5 == 0 else minor_lat
        width_px = 3 if lat % 5 == 0 else 1
        paint_boundary_row(y, color, width_px)
    radar = Image.fromarray(rgba, 'RGBA')
    return np.asarray(Image.alpha_composite(Image.fromarray(grid, 'RGBA'), radar), dtype=np.uint8)


def write_tiles(rgba, output_dir, res, tile_size, geo_transform, debug_lat_lon_grid, palette):
    stride = 1 << res
    level = rgba[::stride, ::stride, :]
    if debug_lat_lon_grid:
        origin_lon, pixel_lon, rot_x, origin_lat, rot_y, pixel_lat = geo_transform
        level_geo_transform = [
            origin_lon,
            pixel_lon * stride,
            rot_x,
            origin_lat,
            rot_y,
            pixel_lat * stride,
        ]
        level = composite_lat_lon_grid_under_radar(level, level_geo_transform)
    level_indices, quality = quantize_rgba_to_fixed_palette_indices(level, palette)
    height, width = level.shape[:2]
    tile_cols = (width + tile_size - 1) // tile_size
    tile_rows = (height + tile_size - 1) // tile_size
    level_root = output_dir / 'tiles' / f'res{res}'
    for tile_y in range(tile_rows):
        y0 = tile_y * tile_size
        y1 = min(y0 + tile_size, height)
        for tile_x in range(tile_cols):
            x0 = tile_x * tile_size
            x1 = min(x0 + tile_size, width)
            tile = level_indices[y0:y1, x0:x1]
            tile_path = level_root / str(tile_x) / f'{tile_y}.png'
            save_fixed_palette_png(tile, tile_path, palette)
    return {
        'res': res,
        'width': width,
        'height': height,
        'tile_cols': tile_cols,
        'tile_rows': tile_rows,
        'quality': quality,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--palette', required=True)
    parser.add_argument('--source-gz', required=True)
    parser.add_argument('--output-dir', required=True)
    parser.add_argument('--state-id', required=True)
    parser.add_argument('--observed-at-utc', required=True)
    parser.add_argument('--source-file', required=True)
    parser.add_argument('--source-sha256', required=True)
    parser.add_argument('--tile-size', type=int, required=True)
    parser.add_argument('--res-level', type=int, action='append', required=True)
    parser.add_argument('--debug-lat-lon-grid', action='store_true')
    args = parser.parse_args()

    output_dir = Path(args.output_dir)
    palette, _flat_palette, _transparency, palette_sha256 = load_fixed_palette(Path(args.palette))
    dataset = open_source(Path(args.source_gz), output_dir)
    source_width = dataset.RasterXSize
    source_height = dataset.RasterYSize
    projection_wkt = dataset.GetProjection()
    geo_transform = list(dataset.GetGeoTransform())
    rgba = read_rgba(dataset)
    dataset = None
    source_tif = output_dir / 'source.tif'
    if source_tif.exists():
        source_tif.unlink()
    levels = [
        write_tiles(rgba, output_dir, res, args.tile_size, geo_transform, args.debug_lat_lon_grid, palette)
        for res in sorted(set(args.res_level))
    ]
    quality = {
        'palette_error_max': max((level['quality']['palette_error_max'] for level in levels), default=0.0),
        'palette_error_p95': max((level['quality']['palette_error_p95'] for level in levels), default=0.0),
        'poor_color_match_count': sum(level['quality']['poor_color_match_count'] for level in levels),
        'poor_color_match_threshold': POOR_COLOR_MATCH_THRESHOLD,
    }
    manifest = {
        'schema_version': 1,
        'product': 'nexrad',
        'state_id': args.state_id,
        'observed_at_utc': args.observed_at_utc,
        'source_file': args.source_file,
        'source_sha256': args.source_sha256,
        'tile_encoding': 'png8-fixed-palette',
        'palette': {
            'transparent_index': TRANSPARENT_INDEX,
            'opaque_indices': [1, 255],
            'sha256': palette_sha256,
        },
        'tile_size': args.tile_size,
        'quality': quality,
        'debug_lat_lon_grid': args.debug_lat_lon_grid,
        'res-levels': [level['res'] for level in levels],
        'source_grid': {
            'width': source_width,
            'height': source_height,
            'projection_wkt': projection_wkt,
            'geo_transform': geo_transform,
        },
        'tile_path_template': 'tiles/res{res}/{x}/{y}.png',
        'levels': levels,
    }
    (output_dir / 'manifest.json').write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + '\n'
    )


if __name__ == '__main__':
    main()
