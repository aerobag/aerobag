#!/usr/bin/env python3
import argparse
import json
import math
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

import numpy as np
from osgeo import gdal
from PIL import Image

RADIUS = 6378137.0
ORIGIN_SHIFT = math.pi * RADIUS
FEET_PER_METER = 3.280839895

COLOR_STOPS = [
    (0.0, (198, 221, 154)),
    (1000.0, (164, 195, 117)),
    (2000.0, (216, 204, 151)),
    (3000.0, (197, 166, 112)),
    (5000.0, (148, 111, 73)),
    (9000.0, (92, 64, 42)),
]

WORKER_DS = None
WORKER_TILES_ROOT = None
WORKER_ZOOM = None
WORKER_TILE_SIZE = None
WORKER_RESOLUTION = None


def mercator(lon, lat):
    lat = max(min(lat, 85.05112878), -85.05112878)
    mx = lon * ORIGIN_SHIFT / 180.0
    my = math.log(math.tan((90.0 + lat) * math.pi / 360.0)) * RADIUS
    return mx, my


def tile_bounds(x, y, z, tile_size):
    initial_resolution = (2.0 * math.pi * RADIUS) / tile_size
    resolution = initial_resolution / (2**z)
    minx = x * tile_size * resolution - ORIGIN_SHIFT
    maxx = (x + 1) * tile_size * resolution - ORIGIN_SHIFT
    miny = y * tile_size * resolution - ORIGIN_SHIFT
    maxy = (y + 1) * tile_size * resolution - ORIGIN_SHIFT
    return minx, miny, maxx, maxy


def tile_range(west, south, east, north, z, tile_size):
    resolution = ((2.0 * math.pi * RADIUS) / tile_size) / (2**z)
    west_m, south_m = mercator(west, south)
    east_m, north_m = mercator(east, north)
    x0 = math.floor((west_m + ORIGIN_SHIFT) / resolution / tile_size)
    x1 = math.floor((east_m + ORIGIN_SHIFT) / resolution / tile_size)
    y0 = math.floor((south_m + ORIGIN_SHIFT) / resolution / tile_size)
    y1 = math.floor((north_m + ORIGIN_SHIFT) / resolution / tile_size)
    return range(x0, x1 + 1), range(y0, y1 + 1)


def colorize(elev_ft, invalid):
    rgb = np.zeros((*elev_ft.shape, 3), dtype=np.float64)
    for threshold, color in COLOR_STOPS:
        mask = elev_ft >= threshold
        rgb[mask, :] = color
    below = elev_ft < 0.0
    rgb[below, :] = COLOR_STOPS[0][1]
    rgb[invalid, :] = 0
    return rgb


def hillshade(elev_m, invalid, pixel_size_m):
    filled = elev_m.astype(np.float64).copy()
    if np.any(invalid):
        valid_values = filled[~invalid]
        fill_value = float(np.nanmean(valid_values)) if valid_values.size else 0.0
        filled[invalid] = fill_value
    dy, dx = np.gradient(filled, pixel_size_m, pixel_size_m)
    nx = -dx
    ny = -dy
    nz = np.ones_like(filled)
    norm = np.sqrt(nx * nx + ny * ny + nz * nz)
    nx /= norm
    ny /= norm
    nz /= norm
    azimuth = math.radians(315.0)
    altitude = math.radians(45.0)
    sx = math.sin(azimuth) * math.cos(altitude)
    sy = math.cos(azimuth) * math.cos(altitude)
    sz = math.sin(altitude)
    shade = np.clip(nx * sx + ny * sy + nz * sz, 0.0, 1.0)
    return 0.62 + 0.46 * shade


def write_png(path, rgba):
    path.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(rgba, mode="RGBA").save(path, optimize=False)


def init_worker(vrt_path, tiles_root, zoom, tile_size, resolution):
    global WORKER_DS, WORKER_TILES_ROOT, WORKER_ZOOM, WORKER_TILE_SIZE, WORKER_RESOLUTION
    WORKER_DS = gdal.Open(vrt_path)
    if WORKER_DS is None:
        raise RuntimeError(f"failed to open {vrt_path}")
    WORKER_TILES_ROOT = Path(tiles_root)
    WORKER_ZOOM = zoom
    WORKER_TILE_SIZE = tile_size
    WORKER_RESOLUTION = resolution


def render_tile(task):
    x, y = task
    minx, miny, maxx, maxy = tile_bounds(x, y, WORKER_ZOOM, WORKER_TILE_SIZE)
    margin = WORKER_RESOLUTION
    warped = gdal.Warp(
        "",
        WORKER_DS,
        format="MEM",
        dstSRS="EPSG:3857",
        outputBounds=[minx - margin, miny - margin, maxx + margin, maxy + margin],
        width=WORKER_TILE_SIZE + 2,
        height=WORKER_TILE_SIZE + 2,
        resampleAlg="bilinear",
        dstNodata=-999999.0,
    )
    arr = warped.ReadAsArray().astype(np.float64)
    invalid = (arr <= -999998.0) | np.isnan(arr)
    elev_ft = arr * FEET_PER_METER
    rgb = colorize(elev_ft, invalid)
    shade = hillshade(arr, invalid, WORKER_RESOLUTION)
    lit = np.clip(rgb * shade[:, :, None], 0, 255).astype(np.uint8)
    alpha = np.where(invalid, 0, 255).astype(np.uint8)
    rgba = np.dstack([lit, alpha])[1:-1, 1:-1, :]
    write_png(WORKER_TILES_ROOT / str(WORKER_ZOOM) / str(x) / f"{y}.png", rgba)
    return 1


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--vrt", required=True)
    ap.add_argument("--output-dir", required=True)
    ap.add_argument("--region", required=True)
    ap.add_argument("--bbox", required=True)
    ap.add_argument("--zoom", required=True, type=int)
    ap.add_argument("--tile-size", required=True, type=int)
    ap.add_argument("--version-label", required=True)
    ap.add_argument("--source-count", required=True, type=int)
    ap.add_argument("--missing-cells", default="")
    ap.add_argument("--workers", required=True, type=int)
    args = ap.parse_args()
    west, south, east, north = [float(x) for x in args.bbox.split(",")]
    root = Path(args.output_dir)
    tiles_root = root / "tiles"
    ds = gdal.Open(args.vrt)
    if ds is None:
        raise SystemExit(f"failed to open {args.vrt}")
    x_range, y_range = tile_range(west, south, east, north, args.zoom, args.tile_size)
    resolution = ((2.0 * math.pi * RADIUS) / args.tile_size) / (2**args.zoom)
    tasks = [(x, y) for x in x_range for y in y_range]
    workers = max(1, args.workers)
    if workers == 1:
        init_worker(args.vrt, str(tiles_root), args.zoom, args.tile_size, resolution)
        count = sum(render_tile(task) for task in tasks)
    else:
        with ProcessPoolExecutor(
            max_workers=workers,
            initializer=init_worker,
            initargs=(args.vrt, str(tiles_root), args.zoom, args.tile_size, resolution),
        ) as pool:
            count = sum(pool.map(render_tile, tasks, chunksize=8))

    manifest = {
        "schema_version": 1,
        "product": "shaded-relief",
        "region": args.region,
        "version_label": args.version_label,
        "zoom": args.zoom,
        "tile_size": args.tile_size,
        "tile_format": "png_rgba",
        "tile_content_encoding": "identity",
        "zip_member_compression": "stored_png",
        "worker_count": workers,
        "source_dem": "USGS 3DEP 1 arc-second DEM",
        "source_dem_vertical_datum": "source tile metadata; generally NAVD88 in CONUS",
        "color_table": [
            {"min_feet": 0, "rgb": COLOR_STOPS[0][1], "label": "light green"},
            {"min_feet": 1000, "rgb": COLOR_STOPS[1][1], "label": "darker green"},
            {"min_feet": 2000, "rgb": COLOR_STOPS[2][1], "label": "beige"},
            {"min_feet": 3000, "rgb": COLOR_STOPS[3][1], "label": "tan"},
            {"min_feet": 5000, "rgb": COLOR_STOPS[4][1], "label": "mid brown"},
            {"min_feet": 9000, "rgb": COLOR_STOPS[5][1], "label": "dark brown"},
        ],
        "hillshade": {
            "azimuth_degrees": 315,
            "altitude_degrees": 45,
            "note": "first-cut DEM hillshade multiplied over elevation color buckets",
        },
        "water_glacier_mask": "not applied in first cut",
        "source_dem_count": args.source_count,
        "missing_dem_cells": [cell for cell in args.missing_cells.split(",") if cell],
        "nodata": "transparent alpha",
        "tile_count": count,
        "files": {"tiles": "tiles"},
    }
    with open(root / "manifest.json", "w") as f:
        json.dump(manifest, f, indent=2, sort_keys=True)


if __name__ == "__main__":
    main()
