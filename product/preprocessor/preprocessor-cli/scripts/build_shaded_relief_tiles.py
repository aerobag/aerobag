#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import argparse
import json
import math
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

import numpy as np
from osgeo import gdal, ogr
from PIL import Image, ImageDraw

RADIUS = 6378137.0
ORIGIN_SHIFT = math.pi * RADIUS
FEET_PER_METER = 3.280839895
WEBP_QUALITY = 75
WEBP_METHOD = 4
WATER_RGB = (104, 154, 185)
GLACIER_RGB = (242, 242, 242)
MASK_ICE_MIN = 64
MASK_WATER_MIN = 192

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
WORKER_WATER_MASK_ROOT = None
WORKER_ZOOM = None
WORKER_TILE_SIZE = None
WORKER_RESOLUTION = None
RASTER_TILE_SUFFIXES = {".png", ".webp"}
STATE_BORDER_RGBA = (128, 128, 128, 204)
PRIMARY_ROAD_RGBA = (91, 111, 122, 153)


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


def pixel_for_lonlat(lon, lat, bounds, tile_size):
    mx, my = mercator(lon, lat)
    minx, miny, maxx, maxy = bounds
    return (
        (mx - minx) / (maxx - minx) * tile_size,
        (maxy - my) / (maxy - miny) * tile_size,
    )


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
        exact=True,
        alpha_quality=100,
    )


def write_webp(path, rgba):
    save_webp(path, Image.fromarray(rgba, mode="RGBA"))


def read_water_mask(x, y):
    if WORKER_WATER_MASK_ROOT is None:
        return None
    path = WORKER_WATER_MASK_ROOT / str(WORKER_ZOOM) / str(x) / f"{y}.water.png"
    if not path.exists():
        return None
    return np.array(Image.open(path).convert("L"))


def resampling_filter():
    return getattr(getattr(Image, "Resampling", Image), "BOX")


def build_parent_tile(tiles_root, z, x, y, tile_size):
    mosaic = Image.new("RGBA", (tile_size * 2, tile_size * 2), (0, 0, 0, 0))
    children = [
        (x * 2, y * 2 + 1, 0, 0),
        (x * 2 + 1, y * 2 + 1, tile_size, 0),
        (x * 2, y * 2, 0, tile_size),
        (x * 2 + 1, y * 2, tile_size, tile_size),
    ]
    for child_x, child_y, dst_x, dst_y in children:
        child_path = tiles_root / str(z + 1) / str(child_x) / f"{child_y}.webp"
        if child_path.exists():
            child = Image.open(child_path).convert("RGBA")
            mosaic.paste(child, (dst_x, dst_y))
    parent = mosaic.resize((tile_size, tile_size), resampling_filter())
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
            "boxes": [{
                "x_min": min(xs),
                "x_max": max(xs),
                "y_tms_min": min(ys),
                "y_tms_max": max(ys),
            }],
        })
    return levels


def load_line_geometries(path, bounds_lonlat=None):
    if not path:
        return []
    dataset = ogr.Open(str(path))
    if dataset is None:
        raise RuntimeError(f"failed to open {path}")
    layer = dataset.GetLayer(0)
    lines = []
    min_lon = min_lat = max_lon = max_lat = None
    if bounds_lonlat:
        min_lon, min_lat, max_lon, max_lat = bounds_lonlat
    for feature in layer:
        geom = feature.GetGeometryRef()
        if geom is None:
            continue
        for line in iter_lines(geom):
            points = [
                (line.GetX(index), line.GetY(index))
                for index in range(line.GetPointCount())
            ]
            for segment in split_discontinuous_line(points):
                if len(segment) < 2:
                    continue
                lons = [lon for lon, _lat in segment]
                lats = [lat for _lon, lat in segment]
                if bounds_lonlat and (
                    max(lons) < min_lon
                    or min(lons) > max_lon
                    or max(lats) < min_lat
                    or min(lats) > max_lat
                ):
                    continue
                lines.append(segment)
    return lines


def iter_lines(geom):
    name = geom.GetGeometryName().upper()
    if name == "LINESTRING":
        yield geom
    elif name in ("MULTILINESTRING", "GEOMETRYCOLLECTION"):
        for index in range(geom.GetGeometryCount()):
            yield from iter_lines(geom.GetGeometryRef(index))


def split_discontinuous_line(points, max_jump_degrees=10.0):
    current = []
    previous = None
    for point in points:
        if previous is not None and (
            abs(point[0] - previous[0]) > max_jump_degrees
            or abs(point[1] - previous[1]) > max_jump_degrees
        ):
            if len(current) >= 2:
                yield current
            current = []
        current.append(point)
        previous = point
    if len(current) >= 2:
        yield current


def draw_dashed_line(draw, points, fill, width, dash=8, gap=6):
    for start, end in zip(points, points[1:]):
        x0, y0 = start
        x1, y1 = end
        dx = x1 - x0
        dy = y1 - y0
        length = math.hypot(dx, dy)
        if length <= 0:
            continue
        distance = 0.0
        while distance < length:
            dash_end = min(distance + dash, length)
            sx = x0 + dx * (distance / length)
            sy = y0 + dy * (distance / length)
            ex = x0 + dx * (dash_end / length)
            ey = y0 + dy * (dash_end / length)
            draw.line([(sx, sy), (ex, ey)], fill=fill, width=width)
            distance += dash + gap


def offset_polyline(points, offset):
    shifted = []
    for index, (x, y) in enumerate(points):
        normals = []
        if index > 0:
            px, py = points[index - 1]
            dx = x - px
            dy = y - py
            length = math.hypot(dx, dy)
            if length > 0:
                normals.append((-dy / length, dx / length))
        if index + 1 < len(points):
            nx, ny = points[index + 1]
            dx = nx - x
            dy = ny - y
            length = math.hypot(dx, dy)
            if length > 0:
                normals.append((-dy / length, dx / length))
        if normals:
            ox = sum(normal[0] for normal in normals) / len(normals)
            oy = sum(normal[1] for normal in normals) / len(normals)
            length = math.hypot(ox, oy)
            if length > 0:
                shifted.append((x + ox / length * offset, y + oy / length * offset))
                continue
        shifted.append((x, y))
    return shifted


def draw_paired_line(draw, points, fill, width, separation):
    offset = separation / 2.0
    draw.line(offset_polyline(points, -offset), fill=fill, width=width)
    draw.line(offset_polyline(points, offset), fill=fill, width=width)


def line_tile_range(points, z, tile_size):
    lons = [lon for lon, _lat in points]
    lats = [lat for _lon, lat in points]
    x_range, y_range = tile_range(
        min(lons),
        min(lats),
        max(lons),
        max(lats),
        z,
        tile_size,
    )
    limit = (2**z) - 1
    x_range = range(max(0, min(x_range.start, limit)), max(0, min(x_range.stop - 1, limit)) + 1)
    y_range = range(max(0, min(y_range.start, limit)), max(0, min(y_range.stop - 1, limit)) + 1)
    return x_range, y_range


def build_overlay_index(lines, min_zoom, max_zoom, tile_size):
    index = {}
    for points in lines:
        for z in range(min_zoom, max_zoom + 1):
            x_range, y_range = line_tile_range(points, z, tile_size)
            for x in x_range:
                for y in y_range:
                    index.setdefault((z, x, y), []).append(points)
    return index


def draw_line_geometries(draw, lines, bounds, tile_size, style, z):
    margin = 24
    for lonlat_points in lines:
        points = [
            pixel_for_lonlat(lon, lat, bounds, tile_size)
            for lon, lat in lonlat_points
        ]
        if len(points) < 2:
            continue
        if not any(
            -margin <= x <= tile_size + margin and -margin <= y <= tile_size + margin
            for x, y in points
        ):
            continue
        if style == "state-border":
            width = 1 if z < 8 else 2
            draw_dashed_line(draw, points, STATE_BORDER_RGBA, width)
        elif style == "primary-road":
            draw_paired_line(
                draw,
                points,
                PRIMARY_ROAD_RGBA,
                1,
                2 if z < 8 else 3,
            )


def draw_overlays_on_tile(tile_path, z, x, y, tile_size, state_index, road_index):
    image = Image.open(tile_path).convert("RGBA")
    draw = ImageDraw.Draw(image, "RGBA")
    bounds = tile_bounds(x, y, z, tile_size)
    key = (z, x, y)
    draw_line_geometries(draw, road_index.get(key, []), bounds, tile_size, "primary-road", z)
    draw_line_geometries(draw, state_index.get(key, []), bounds, tile_size, "state-border", z)
    save_webp(tile_path, image)


def draw_overlays(tiles_root, max_zoom, tile_size, state_borders_shp, primary_roads_shp, include_low_zoom, bounds_lonlat):
    min_zoom = 0 if include_low_zoom else 8
    state_index = build_overlay_index(
        load_line_geometries(state_borders_shp, bounds_lonlat),
        min_zoom,
        max_zoom,
        tile_size,
    )
    road_index = build_overlay_index(
        load_line_geometries(primary_roads_shp, bounds_lonlat),
        min_zoom,
        max_zoom,
        tile_size,
    )
    for z, x, y in sorted(set(state_index) | set(road_index)):
        tile_path = tiles_root / str(z) / str(x) / f"{y}.webp"
        if not tile_path.exists():
            continue
        draw_overlays_on_tile(
            tile_path,
            z,
            x,
            y,
            tile_size,
            state_index,
            road_index,
        )


def init_worker(vrt_path, tiles_root, water_mask_root, zoom, tile_size, resolution):
    global WORKER_DS, WORKER_TILES_ROOT, WORKER_WATER_MASK_ROOT, WORKER_ZOOM, WORKER_TILE_SIZE, WORKER_RESOLUTION
    WORKER_DS = gdal.Open(vrt_path)
    if WORKER_DS is None:
        raise RuntimeError(f"failed to open {vrt_path}")
    WORKER_TILES_ROOT = Path(tiles_root)
    WORKER_WATER_MASK_ROOT = Path(water_mask_root) if water_mask_root else None
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
    water_mask = read_water_mask(x, y)
    water = None
    if water_mask is not None:
        water = water_mask >= MASK_WATER_MIN
        ice = (water_mask >= MASK_ICE_MIN) & (water_mask < MASK_WATER_MIN)
        rgb_inner = rgb[1:-1, 1:-1, :]
        rgb_inner[ice, :] = GLACIER_RGB
    shade = hillshade(arr, invalid, WORKER_RESOLUTION)
    lit = np.clip(rgb * shade[:, :, None], 0, 255).astype(np.uint8)
    alpha = np.where(invalid, 0, 255).astype(np.uint8)
    rgba = np.dstack([lit, alpha])[1:-1, 1:-1, :]
    if water is not None:
        rgba[water, 0] = WATER_RGB[0]
        rgba[water, 1] = WATER_RGB[1]
        rgba[water, 2] = WATER_RGB[2]
        rgba[water, 3] = 255
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
    ap.add_argument("--water-mask-dir")
    ap.add_argument("--state-borders-shp")
    ap.add_argument("--primary-roads-shp")
    ap.add_argument("--overlay-style-version")
    ap.add_argument("--draw-low-zoom-overlays", action="store_true")
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
        init_worker(
            args.vrt,
            str(tiles_root),
            args.water_mask_dir,
            args.zoom,
            args.tile_size,
            resolution,
        )
        count = sum(render_tile(task) for task in tasks)
    else:
        with ProcessPoolExecutor(
            max_workers=workers,
            initializer=init_worker,
            initargs=(
                args.vrt,
                str(tiles_root),
                args.water_mask_dir,
                args.zoom,
                args.tile_size,
                resolution,
            ),
        ) as pool:
            count = sum(pool.map(render_tile, tasks, chunksize=8))
    level_counts = build_parent_pyramid(tiles_root, args.zoom, args.tile_size)
    if args.state_borders_shp or args.primary_roads_shp:
        draw_overlays(
            tiles_root,
            args.zoom,
            args.tile_size,
            args.state_borders_shp,
            args.primary_roads_shp,
            args.draw_low_zoom_overlays,
            (west, south, east, north),
        )
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
        "parent_tile_policy": "alpha-preserving 2x2 RGBA mosaic downsample from child tiles",
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
        "water_glacier_mask": {
            "water": "USGS NHD water mask when available",
            "rgb": WATER_RGB,
            "ice_mass": "USGS NHD Ice Mass mask when available",
            "ice_rgb": GLACIER_RGB,
            "mask_tiles": bool(args.water_mask_dir),
        },
        "overlays": {
            "style_version": args.overlay_style_version,
            "state_borders": {
                "source": "Natural Earth 50m admin-1 boundary lines",
                "stroke": "dashed 80% gray",
                "path": args.state_borders_shp,
            },
            "primary_roads": {
                "source": "U.S. Census TIGER/Line 2025 national primary roads",
                "stroke": "60% blue-gray paired strokes",
                "path": args.primary_roads_shp,
            },
            "low_zoom_overlays": args.draw_low_zoom_overlays,
        },
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
