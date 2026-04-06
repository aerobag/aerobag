#!/usr/bin/env python3
from __future__ import annotations

import json
import math
import shutil
from pathlib import Path
from osgeo import osr


ROOT = Path(__file__).resolve().parents[2]
UI_DIR = ROOT / "ui"
CANONICAL_OUT = UI_DIR / "shared-fixtures" / "content-prototype" / "content_fixture.json"
WEB_OUT = UI_DIR / "web-app" / "src" / "domain" / "generated" / "contentFixture.json"
ANDROID_OUT = UI_DIR / "android-app" / "app" / "src" / "main" / "assets" / "fixtures" / "contentFixture.json"
CANONICAL_TILE_ROOT = UI_DIR / "shared-fixtures" / "content-prototype" / "tiles"
WEB_TILE_ROOT = UI_DIR / "web-app" / "public" / "prototype-tiles"
ANDROID_TILE_ROOT = UI_DIR / "android-app" / "app" / "src" / "main" / "assets" / "tiles"

SEC_PACKAGE_OUTPUTS = ROOT / "rust-runs" / "sec-20260405T1619Z" / "work" / "rust-runs" / "sec-20260405T1619Z" / "meta" / "provenance" / "charts-sec" / "package_outputs.jsonl"
TPP_PLATES_DIR = ROOT / "runs" / "20260405T154700Z-tpp-retry" / "work" / "tpp-ne" / "plates" / "BOS"
BOSTON_TAC_GEOJSON = ROOT / "rust-runs" / "tac-native" / "work" / "charts-tac" / "TAC" / "Boston TAC.geojson"
BOSTON_TAC_TILE_ROOT = ROOT / "runs" / "20260406T003224Z-validation" / "native" / "charts-tac" / "work" / "charts-tac" / "tiles" / "1"

REGION_ORDER = ["ne", "nc", "nw", "se", "sc", "sw", "ec", "ak", "pac"]
REGION_DISPLAY_NAMES = {
    "ne": "Northeast",
    "nc": "North Central",
    "nw": "Northwest",
    "se": "Southeast",
    "sc": "South Central",
    "sw": "Southwest",
    "ec": "East Coast",
    "ak": "Alaska",
    "pac": "Pacific",
}

WEB_MERCATOR = osr.SpatialReference()
WEB_MERCATOR.ImportFromEPSG(3857)
WGS84 = osr.SpatialReference()
WGS84.ImportFromEPSG(4326)
WEB_MERCATOR.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
WGS84.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
TO_WGS84 = osr.CoordinateTransformation(WEB_MERCATOR, WGS84)
TILE_RADIUS = 1
TILE_ZOOM = 10
TILE_SIZE = 256


def load_jsonl(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def load_wgs84_polygon(path: Path) -> list[list[float]]:
    data = json.loads(path.read_text())
    feature = data["features"][0]
    ring = feature["geometry"]["coordinates"][0]
    points = []
    for x, y in ring:
        lon, lat, _ = TO_WGS84.TransformPoint(x, y)
        points.append([lon, lat])
    return points


def bounds_center(points: list[list[float]]) -> tuple[float, float]:
    lons = [point[0] for point in points]
    lats = [point[1] for point in points]
    return (sum(lats) / len(lats), sum(lons) / len(lons))


def web_mercator_tile(lat: float, lon: float, zoom: int) -> tuple[int, int, int, float, float]:
    scale = 2**zoom
    x_float = (lon + 180.0) / 360.0 * scale
    y_float = (1.0 - math.asinh(math.tan(math.radians(lat))) / math.pi) / 2.0 * scale
    x = int(x_float)
    y_xyz = int(y_float)
    y_tms = (scale - 1) - y_xyz
    return x, y_xyz, y_tms, x_float - x, y_float - y_xyz


def tile_paths(center_x: int, center_y_tms: int, radius: int) -> list[tuple[int, int]]:
    paths = []
    for dy in range(radius, -radius - 1, -1):
        for dx in range(-radius, radius + 1):
            paths.append((center_x + dx, center_y_tms + dy))
    return paths


def copy_tile_subset(center_x: int, center_y_tms: int) -> list[dict]:
    tile_pairs = tile_paths(center_x, center_y_tms, TILE_RADIUS)
    available_tiles = []
    for destination_root in [CANONICAL_TILE_ROOT, WEB_TILE_ROOT, ANDROID_TILE_ROOT]:
        for x, y in tile_pairs:
            source = BOSTON_TAC_TILE_ROOT / str(TILE_ZOOM) / str(x) / f"{y}.webp"
            if not source.exists():
                continue
            target = destination_root / "charts-tac" / "1" / str(TILE_ZOOM) / str(x) / f"{y}.webp"
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
    for x, y in tile_pairs:
        source = BOSTON_TAC_TILE_ROOT / str(TILE_ZOOM) / str(x) / f"{y}.webp"
        if source.exists():
            available_tiles.append({"x": x, "y_tms": y})
    return available_tiles


def pick_bos_plate() -> tuple[str, str, str]:
    candidates = sorted(TPP_PLATES_DIR.glob("IAP-*.png"))
    if not candidates:
        raise RuntimeError(f"no IAP png plates found in {TPP_PLATES_DIR}")

    path = candidates[0]
    stem = path.stem
    parts = stem.split("-", 2)
    if len(parts) != 3:
        raise RuntimeError(f"unexpected plate filename format: {path.name}")

    plate_prefix, _state_code, procedure_name = parts
    procedure_code = f"{plate_prefix}-{procedure_name}"
    asset_base_path = f"plates/BOS/{stem}"
    return procedure_code, procedure_name, asset_base_path


def build_fixture() -> dict:
    sec_outputs = load_jsonl(SEC_PACKAGE_OUTPUTS)
    cycle = "2026-04-16"
    boston_tac_polygon = load_wgs84_polygon(BOSTON_TAC_GEOJSON)
    probe_lat, probe_lon = bounds_center(boston_tac_polygon)
    center_x, _center_y_xyz, center_y_tms, offset_x, offset_y = web_mercator_tile(probe_lat, probe_lon, TILE_ZOOM)
    available_tiles = copy_tile_subset(center_x, center_y_tms)

    packages = []
    regions_seen = set()
    for entry in sec_outputs:
        region = entry["region"].lower()
        regions_seen.add(region)
        package_name = entry["manifest"]
        packages.append(
            {
                "id": {
                    "region": region,
                    "family": "sectional",
                    "cycle": cycle,
                },
                "package_name": package_name,
                "family_id": "sectional",
                "region_id": region,
                "cycle": cycle,
                "artifact_kind": "zip",
                "relative_url": f"/{cycle}/{entry['zip']}",
                "manifest_name": package_name,
                "size_bytes": None,
                "checksum_sha256": entry["zip_sha256"],
            }
        )

    packages.sort(key=lambda item: REGION_ORDER.index(item["region_id"]))

    regions = [
        {
            "id": region,
            "display_name": REGION_DISPLAY_NAMES[region],
            "sort_order": REGION_ORDER.index(region),
        }
        for region in REGION_ORDER
        if region in regions_seen
    ]

    procedure_code, procedure_name, asset_base_path = pick_bos_plate()

    return {
        "catalog": {
            "schema_version": 1,
            "cycle": cycle,
            "catalog_revision": "2026-04-06T00:00:00Z",
            "families": [
                {
                    "id": "sectional",
                    "display_name": "VFR Sectional Charts",
                    "kind": "tiled_raster",
                    "max_zoom": 10,
                    "tile_size": 512,
                },
                {
                    "id": "tac",
                    "display_name": "Terminal Area Charts",
                    "kind": "tiled_raster",
                    "max_zoom": 11,
                    "tile_size": 512,
                },
            ],
            "regions": regions,
            "packages": packages,
            "charts": [
                {
                    "id": {
                        "family": "tac",
                        "name": "Boston TAC",
                        "cycle": cycle,
                    },
                    "family_id": "tac",
                    "name": "Boston TAC",
                    "display_name": "Boston TAC",
                    "cycle": cycle,
                    "region_ids": ["ne"],
                    "max_zoom": 11,
                    "tile_path_template": "tiles/1/{z}/{x}/{y}.webp",
                    "coverage": {
                        "kind": "polygon_ref",
                        "value": {
                            "polygon_id": "tac:boston",
                        },
                    },
                }
            ],
            "plates": [
                {
                    "id": {
                        "airport_id": "BOS",
                        "procedure_code": procedure_code,
                        "page": 1,
                        "cycle": cycle,
                    },
                    "airport_id": "BOS",
                    "region_id": "ne",
                    "cycle": cycle,
                    "procedure_code": procedure_code,
                    "display_name": procedure_name,
                    "kind": "approach",
                    "georeferenced": True,
                    "page_count": 1,
                    "asset_base_path": asset_base_path,
                }
            ],
            "supplements": [],
        },
        "geometry": {
            "schema_version": 1,
            "polygons": [
                {
                    "id": "tac:boston",
                    "points": boston_tac_polygon,
                }
            ],
        },
        "initial_probe": {
            "family": "tac",
            "lat": probe_lat,
            "lon": probe_lon,
        },
        "map_tile_view": {
            "chart_family": "tac",
            "chart_name": "Boston TAC",
            "chart_index": 1,
            "tile_root": "charts-tac",
            "zoom": TILE_ZOOM,
            "tile_size": TILE_SIZE,
            "radius": TILE_RADIUS,
            "center_x": center_x,
            "center_y_tms": center_y_tms,
            "probe_offset_x": offset_x,
            "probe_offset_y": offset_y,
            "available_tiles": available_tiles,
        },
        "flight_plan": {
            "id": "plan-1",
            "name": "BOS local",
            "legs": [
                {
                    "from": {"Airport": "BOS"},
                    "to": {"Airport": "BOS"},
                    "airway": None,
                }
            ],
            "departure": "BOS",
            "destination": "BOS",
            "alternate": None,
            "cruise_altitude_ft": 3000,
            "notes": "Generated from preprocessor outputs",
            "updated_at_epoch_ms": 0,
            "version": 1,
        },
        "remote_only_inventory": {
            "installed_packages": [],
            "cached_tilesets": [],
            "cached_plates": [],
        },
        "installed_inventory": {
            "installed_packages": [
                {
                    "package_id": {
                        "region": "ne",
                        "family": "sectional",
                        "cycle": cycle,
                    },
                    "integrity_ok": True,
                }
            ],
            "cached_tilesets": [],
            "cached_plates": [],
        },
    }


def write_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=False) + "\n")


def main() -> None:
    fixture = build_fixture()
    write_json(CANONICAL_OUT, fixture)
    write_json(WEB_OUT, fixture)
    write_json(ANDROID_OUT, fixture)


if __name__ == "__main__":
    main()
