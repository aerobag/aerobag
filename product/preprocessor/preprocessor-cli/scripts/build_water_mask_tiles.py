#!/usr/bin/env python3
import argparse
import hashlib
import json
import math
import socket
import time
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ProcessPoolExecutor, ThreadPoolExecutor
from pathlib import Path

from PIL import Image, ImageDraw

RADIUS = 6378137.0
ORIGIN_SHIFT = math.pi * RADIUS
NHD_SERVICE = "https://hydro.nationalmap.gov/arcgis/rest/services/nhd/MapServer"
NHD_LAYERS = [
    (9, "Area - Large Scale"),
    (12, "Waterbody - Large Scale"),
]
PAGE_SIZE = 1000
FETCH_ATTEMPTS = 5
RETRYABLE_HTTP_STATUSES = {429, 500, 502, 503, 504}

WORKER_FEATURES = None
WORKER_TILES_ROOT = None
WORKER_ZOOM = None
WORKER_TILE_SIZE = None


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


def query_url(layer, params):
    return f"{NHD_SERVICE}/{layer}/query?{urllib.parse.urlencode(params)}"


def fetch_json(url):
    last_error = None
    for attempt in range(1, FETCH_ATTEMPTS + 1):
        try:
            with urllib.request.urlopen(url, timeout=120) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            if error.code not in RETRYABLE_HTTP_STATUSES:
                raise
            last_error = error
        except (urllib.error.URLError, TimeoutError, socket.timeout) as error:
            last_error = error
        if attempt < FETCH_ATTEMPTS:
            time.sleep(min(30, 2**attempt))
    raise RuntimeError(f"failed to fetch {url} after {FETCH_ATTEMPTS} attempts") from last_error


def count_layer(layer, bbox):
    params = {
        "where": "1=1",
        "geometry": bbox,
        "geometryType": "esriGeometryEnvelope",
        "inSR": "4326",
        "spatialRel": "esriSpatialRelIntersects",
        "returnCountOnly": "true",
        "f": "json",
    }
    value = fetch_json(query_url(layer, params))
    return int(value.get("count", 0))


def fetch_layer_page(layer, bbox, offset):
    params = {
        "where": "1=1",
        "outFields": "FTYPE,FCODE,GNIS_NAME",
        "geometry": bbox,
        "geometryType": "esriGeometryEnvelope",
        "inSR": "4326",
        "spatialRel": "esriSpatialRelIntersects",
        "outSR": "4326",
        "returnGeometry": "true",
        "f": "geojson",
        "resultRecordCount": str(PAGE_SIZE),
        "resultOffset": str(offset),
        "orderByFields": "OBJECTID",
    }
    value = fetch_json(query_url(layer, params))
    features = value.get("features", [])
    for feature in features:
        feature.setdefault("properties", {})["_nhd_layer"] = layer
    return offset, features


def fetch_region_features(bbox, fetch_workers):
    all_features = []
    layer_counts = {}
    with ThreadPoolExecutor(max_workers=fetch_workers) as pool:
        count_futures = {
            pool.submit(count_layer, layer, bbox): (layer, name) for layer, name in NHD_LAYERS
        }
        for future, (layer, name) in count_futures.items():
            layer_counts[str(layer)] = {"name": name, "count": future.result()}
        page_futures = []
        for layer, _name in NHD_LAYERS:
            count = layer_counts[str(layer)]["count"]
            for offset in range(0, count, PAGE_SIZE):
                page_futures.append(pool.submit(fetch_layer_page, layer, bbox, offset))
        for future in page_futures:
            _offset, features = future.result()
            all_features.extend(features)
    all_features.sort(
        key=lambda feature: (
            feature.get("properties", {}).get("_nhd_layer", 0),
            json.dumps(feature.get("geometry"), sort_keys=True, separators=(",", ":")),
        )
    )
    return all_features, layer_counts


def feature_bbox(feature):
    xs = []
    ys = []

    def visit(value):
        if isinstance(value, list) and value and isinstance(value[0], (int, float)):
            lon, lat = value[:2]
            mx, my = mercator(lon, lat)
            xs.append(mx)
            ys.append(my)
        elif isinstance(value, list):
            for item in value:
                visit(item)

    visit(feature.get("geometry", {}).get("coordinates", []))
    if not xs:
        return None
    return min(xs), min(ys), max(xs), max(ys)


def tile_range_for_mercator_bbox(minx, miny, maxx, maxy, z, tile_size):
    resolution = ((2.0 * math.pi * RADIUS) / tile_size) / (2**z)
    x0 = math.floor((minx + ORIGIN_SHIFT) / resolution / tile_size)
    x1 = math.floor((maxx + ORIGIN_SHIFT) / resolution / tile_size)
    y0 = math.floor((miny + ORIGIN_SHIFT) / resolution / tile_size)
    y1 = math.floor((maxy + ORIGIN_SHIFT) / resolution / tile_size)
    return range(x0, x1 + 1), range(y0, y1 + 1)


def tile_feature_map(features, x_range, y_range, z, tile_size):
    x_values = set(x_range)
    y_values = set(y_range)
    by_tile = {}
    for index, feature in enumerate(features):
        bbox = feature_bbox(feature)
        if bbox is None:
            continue
        feature_x_range, feature_y_range = tile_range_for_mercator_bbox(*bbox, z, tile_size)
        for x in feature_x_range:
            if x not in x_values:
                continue
            for y in feature_y_range:
                if y in y_values:
                    by_tile.setdefault((x, y), []).append(index)
    return by_tile


def init_worker(features, tiles_root, zoom, tile_size):
    global WORKER_FEATURES, WORKER_TILES_ROOT, WORKER_ZOOM, WORKER_TILE_SIZE
    WORKER_FEATURES = features
    WORKER_TILES_ROOT = Path(tiles_root)
    WORKER_ZOOM = zoom
    WORKER_TILE_SIZE = tile_size


def pixel_for_lonlat(lon, lat, minx, maxy, resolution):
    mx, my = mercator(lon, lat)
    return (mx - minx) / resolution, (maxy - my) / resolution


def draw_ring(draw, ring, fill, minx, maxy, resolution):
    points = [pixel_for_lonlat(lon, lat, minx, maxy, resolution) for lon, lat, *_ in ring]
    if len(points) >= 3:
        draw.polygon(points, fill=fill)


def draw_polygon(draw, rings, minx, maxy, resolution):
    if not rings:
        return
    draw_ring(draw, rings[0], 255, minx, maxy, resolution)
    for hole in rings[1:]:
        draw_ring(draw, hole, 0, minx, maxy, resolution)


def draw_feature(draw, feature, minx, maxy, resolution):
    geometry = feature.get("geometry") or {}
    coords = geometry.get("coordinates") or []
    if geometry.get("type") == "Polygon":
        draw_polygon(draw, coords, minx, maxy, resolution)
    elif geometry.get("type") == "MultiPolygon":
        for polygon in coords:
            draw_polygon(draw, polygon, minx, maxy, resolution)


def render_base_tile(task):
    x, y, feature_indices = task
    image = Image.new("L", (WORKER_TILE_SIZE, WORKER_TILE_SIZE), 0)
    draw = ImageDraw.Draw(image)
    minx, miny, maxx, maxy = tile_bounds(x, y, WORKER_ZOOM, WORKER_TILE_SIZE)
    resolution = (maxx - minx) / WORKER_TILE_SIZE
    for index in feature_indices:
        draw_feature(draw, WORKER_FEATURES[index], minx, maxy, resolution)
    path = WORKER_TILES_ROOT / str(WORKER_ZOOM) / str(x) / f"{y}.water.png"
    path.parent.mkdir(parents=True, exist_ok=True)
    image.save(path, format="PNG", optimize=True)
    return 1


def resampling_filter():
    return getattr(getattr(Image, "Resampling", Image), "BOX")


def build_parent_tile(tiles_root, z, x, y, tile_size):
    mosaic = Image.new("L", (tile_size * 2, tile_size * 2), 0)
    children = [
        (x * 2, y * 2 + 1, 0, 0),
        (x * 2 + 1, y * 2 + 1, tile_size, 0),
        (x * 2, y * 2, 0, tile_size),
        (x * 2 + 1, y * 2, tile_size, tile_size),
    ]
    for child_x, child_y, dst_x, dst_y in children:
        child_path = tiles_root / str(z + 1) / str(child_x) / f"{child_y}.water.png"
        if child_path.exists():
            child = Image.open(child_path).convert("L")
            mosaic.paste(child, (dst_x, dst_y))
    parent = mosaic.resize((tile_size, tile_size), resampling_filter())
    path = tiles_root / str(z) / str(x) / f"{y}.water.png"
    path.parent.mkdir(parents=True, exist_ok=True)
    parent.save(path, format="PNG", optimize=True)


def build_parent_pyramid(tiles_root, max_zoom, tile_size):
    counts = {max_zoom: sum(1 for _ in (tiles_root / str(max_zoom)).glob("*/*.water.png"))}
    for z in range(max_zoom - 1, -1, -1):
        child_root = tiles_root / str(z + 1)
        parents = set()
        for child_path in child_root.glob("*/*.water.png"):
            child_x = int(child_path.parent.name)
            child_y = int(child_path.name.split(".")[0])
            parents.add((child_x // 2, child_y // 2))
        for x, y in sorted(parents):
            build_parent_tile(tiles_root, z, x, y, tile_size)
        counts[z] = len(parents)
    return counts


def source_fingerprint(features):
    payload = json.dumps(features, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(payload).hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--output-dir", required=True)
    ap.add_argument("--region", required=True)
    ap.add_argument("--bbox", required=True)
    ap.add_argument("--zoom", required=True, type=int)
    ap.add_argument("--tile-size", required=True, type=int)
    ap.add_argument("--fetch-workers", required=True, type=int)
    ap.add_argument("--tile-workers", required=True, type=int)
    args = ap.parse_args()
    west, south, east, north = [float(x) for x in args.bbox.split(",")]
    bbox = f"{west},{south},{east},{north}"
    root = Path(args.output_dir)
    root.mkdir(parents=True, exist_ok=True)
    tiles_root = root / "tiles"
    source_fetched_at_utc = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    features, layer_counts = fetch_region_features(bbox, max(1, args.fetch_workers))
    fingerprint = source_fingerprint(features)
    (root / "source.geojson").write_text(
        json.dumps({"type": "FeatureCollection", "features": features}, sort_keys=True)
    )
    x_range, y_range = tile_range(west, south, east, north, args.zoom, args.tile_size)
    by_tile = tile_feature_map(features, x_range, y_range, args.zoom, args.tile_size)
    tasks = [(x, y, indices) for (x, y), indices in sorted(by_tile.items())]
    workers = max(1, args.tile_workers)
    if workers == 1:
        init_worker(features, str(tiles_root), args.zoom, args.tile_size)
        base_count = sum(render_base_tile(task) for task in tasks)
    else:
        with ProcessPoolExecutor(
            max_workers=workers,
            initializer=init_worker,
            initargs=(features, str(tiles_root), args.zoom, args.tile_size),
        ) as pool:
            base_count = sum(pool.map(render_base_tile, tasks, chunksize=8))
    level_counts = build_parent_pyramid(tiles_root, args.zoom, args.tile_size)
    manifest = {
        "schema_version": 1,
        "product": "water-mask",
        "region": args.region,
        "source": "USGS National Hydrography Dataset MapServer",
        "source_service": NHD_SERVICE,
        "source_fetched_at_utc": source_fetched_at_utc,
        "source_fingerprint": fingerprint,
        "source_layers": layer_counts,
        "bbox": [west, south, east, north],
        "min_zoom": 0,
        "max_zoom": args.zoom,
        "base_zoom": args.zoom,
        "tile_size": args.tile_size,
        "tile_format": "png_l",
        "tile_content_encoding": "identity",
        "zip_member_compression": "stored_png",
        "mask_semantics": {"0": "not water", "255": "water"},
        "feature_count": len(features),
        "base_tile_count": base_count,
        "tile_count": sum(level_counts.values()),
        "levels": [{"zoom": z, "tile_count": level_counts[z]} for z in sorted(level_counts)],
        "fetch_workers": args.fetch_workers,
        "tile_workers": workers,
        "files": {"tiles": "tiles"},
    }
    with open(root / "manifest.json", "w") as f:
        json.dump(manifest, f, indent=2, sort_keys=True)


if __name__ == "__main__":
    main()
