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
WEBP_QUALITY = 75
WEBP_METHOD = 4

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
RASTER_TILE_SUFFIXES = {".png", ".webp"}


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


def save_webp(path, image):
    path.parent.mkdir(parents=True, exist_ok=True)
    image.save(
        path,
        format="WEBP",
        quality=WEBP_QUALITY,
        method=WEBP_METHOD,
    )


def write_webp(path, rgba):
    save_webp(path, Image.fromarray(rgba, mode="RGBA"))


def resampling_filter():
    return getattr(getattr(Image, "Resampling", Image), "LANCZOS")


def build_parent_tile(tiles_root, z, x, y, tile_size):
    half = tile_size // 2
    parent = Image.new("RGBA", (tile_size, tile_size), (0, 0, 0, 0))
    children = [
        (x * 2, y * 2 + 1, 0, 0),
        (x * 2 + 1, y * 2 + 1, half, 0),
        (x * 2, y * 2, 0, half),
        (x * 2 + 1, y * 2, half, half),
    ]
    for child_x, child_y, dst_x, dst_y in children:
        child_path = tiles_root / str(z + 1) / str(child_x) / f"{child_y}.webp"
        if child_path.exists():
            child = Image.open(child_path).convert("RGBA")
            parent.paste(child.resize((half, half), resampling_filter()), (dst_x, dst_y))
    save_webp(tiles_root / str(z) / str(x) / f"{y}.webp", parent)


def build_parent_pyramid(tiles_root, max_zoom, tile_size):
    counts = {max_zoom: sum(1 for _ in (tiles_root / str(max_zoom)).glob("*/*.webp"))}
    for z in range(max_zoom - 1, -1, -1):
        child_root = tiles_root / str(z + 1)
        parents = set()
        for child_path in child_root.glob("*/*.webp"):
            child_x = int(child_path.parent.name)
            child_y = int(child_path.stem)
            parents.add((child_x // 2, child_y // 2))
        for x, y in sorted(parents):
            build_parent_tile(tiles_root, z, x, y, tile_size)
        counts[z] = len(parents)
    return counts


def scan_tile_levels(tiles_root):
    levels = []
    for z_dir in sorted((path for path in tiles_root.iterdir() if path.is_dir()), key=lambda path: int(path.name)):
        zoom = int(z_dir.name)
        coords = []
        for x_dir in z_dir.iterdir():
            if not x_dir.is_dir():
                continue
            x = int(x_dir.name)
            for tile_path in x_dir.iterdir():
                if tile_path.suffix.lower() not in RASTER_TILE_SUFFIXES:
                    continue
                coords.append((x, int(tile_path.stem)))
        if not coords:
            continue
        xs = [x for x, _ in coords]
        ys = [y for _, y in coords]
        levels.append({
            "zoom": zoom,
            "tile_count": len(coords),
            "x_min": min(xs),
            "x_max": max(xs),
            "y_tms_min": min(ys),
            "y_tms_max": max(ys),
        })
    return levels


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
    write_webp(WORKER_TILES_ROOT / str(WORKER_ZOOM) / str(x) / f"{y}.webp", rgba)
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
    level_counts = build_parent_pyramid(tiles_root, args.zoom, args.tile_size)
    levels = scan_tile_levels(tiles_root)

    manifest = {
        "schema_version": 1,
        "product": "shaded-relief",
        "region": args.region,
        "version_label": args.version_label,
        "min_zoom": 0,
        "max_zoom": args.zoom,
        "base_zoom": args.zoom,
        "tile_size": args.tile_size,
        "tile_format": "webp_rgba",
        "tile_content_encoding": "identity",
        "zip_member_compression": "stored_webp",
        "webp_quality": WEBP_QUALITY,
        "webp_method": WEBP_METHOD,
        "parent_tile_policy": "alpha-preserving RGBA downsample from child tiles",
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
        "base_tile_count": count,
        "tile_count": sum(level_counts.values()),
        "levels": levels,
        "files": {"tiles": "tiles"},
    }
    with open(root / "manifest.json", "w") as f:
        json.dump(manifest, f, indent=2, sort_keys=True)


if __name__ == "__main__":
    main()
