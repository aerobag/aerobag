#!/usr/bin/env python3
from __future__ import annotations

import json
import math
import os
import shutil
import zipfile
from dataclasses import dataclass
from pathlib import Path

from osgeo import osr


ROOT = Path(__file__).resolve().parents[2]
UI_DIR = ROOT / "ui"
RESOURCE_INDEX = ROOT / "rust-runs" / "resource-index" / "resource-index.json"
CANONICAL_OUT = UI_DIR / "shared-fixtures" / "content-prototype" / "content_fixture.json"
CANONICAL_RESOURCE_INDEX_OUT = UI_DIR / "shared-fixtures" / "content-prototype" / "resource-index.json"
WEB_OUT = UI_DIR / "web-app" / "src" / "domain" / "generated" / "contentFixture.json"
WEB_RESOURCE_INDEX_OUT = UI_DIR / "web-app" / "src" / "domain" / "generated" / "resourceIndex.json"
ANDROID_OUT = UI_DIR / "android-app" / "app" / "src" / "main" / "assets" / "fixtures" / "contentFixture.json"
ANDROID_RESOURCE_INDEX_OUT = UI_DIR / "android-app" / "app" / "src" / "main" / "assets" / "fixtures" / "resource-index.json"
CANONICAL_TILE_ROOT = UI_DIR / "shared-fixtures" / "content-prototype" / "tiles"
WEB_TILE_ROOT = UI_DIR / "web-app" / "public" / "prototype-tiles"
ANDROID_TILE_ROOT = UI_DIR / "android-app" / "app" / "src" / "main" / "assets" / "tiles"
WEB_SECTIONAL_ROOT = UI_DIR / "web-app" / "generated-static" / "sectional-packages"
WEB_CHART_ASSET_ROOT = UI_DIR / "web-app" / "generated-static" / "chart-assets"
ANDROID_CHART_ASSET_ROOT = UI_DIR / "android-app" / "app" / "src" / "main" / "assets" / "chart-assets"
TPP_ROOT = ROOT / "runs" / "20260406T032350Z-validation" / "native" / "tpp-ne" / "work" / "tpp-ne"
CSUP_ROOT = ROOT / "runs" / "20260405T154700Z" / "work" / "csup"
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
SECTIONAL_REGIONS = ["nw", "sw"]
TAC_REGIONS = ["nw", "sw"]

WEB_MERCATOR = osr.SpatialReference()
WEB_MERCATOR.ImportFromEPSG(3857)
WGS84 = osr.SpatialReference()
WGS84.ImportFromEPSG(4326)
WEB_MERCATOR.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
WGS84.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
TO_WGS84 = osr.CoordinateTransformation(WEB_MERCATOR, WGS84)
TILE_SIZE = 256
TAC_TILE_LEVELS = {
    9: 6,
    10: 10,
}


@dataclass(frozen=True)
class SectionalPackage:
    manifest: str
    region: str
    artifact_path: Path
    zip_sha256: str


@dataclass(frozen=True)
class TacPackage:
    manifest: str
    region: str
    artifact_path: Path
    zip_sha256: str


def load_resource_index() -> dict:
    return json.loads(RESOURCE_INDEX.read_text())


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


def clear_tac_tile_roots() -> None:
    for destination_root in [CANONICAL_TILE_ROOT, WEB_TILE_ROOT, ANDROID_TILE_ROOT]:
        target_root = destination_root / "charts-tac" / "1"
        if target_root.exists():
            shutil.rmtree(target_root)


def copy_tac_tile_subset(tile_windows: dict[int, tuple[int, int, int]]) -> list[dict]:
    clear_tac_tile_roots()
    levels = []
    for zoom, (center_x, center_y_tms, radius) in tile_windows.items():
        tile_pairs = tile_paths(center_x, center_y_tms, radius)
        available_pairs = []
        for destination_root in [CANONICAL_TILE_ROOT, WEB_TILE_ROOT, ANDROID_TILE_ROOT]:
            for x, y in tile_pairs:
                source = BOSTON_TAC_TILE_ROOT / str(zoom) / str(x) / f"{y}.webp"
                if not source.exists():
                    continue
                target = destination_root / "charts-tac" / "1" / str(zoom) / str(x) / f"{y}.webp"
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, target)

        for x, y in tile_pairs:
            source = BOSTON_TAC_TILE_ROOT / str(zoom) / str(x) / f"{y}.webp"
            if source.exists():
                available_pairs.append((x, y))

        levels.append(
            {
                "zoom": zoom,
                "x_min": min(x for x, _ in available_pairs),
                "x_max": max(x for x, _ in available_pairs),
                "y_tms_min": min(y for _, y in available_pairs),
                "y_tms_max": max(y for _, y in available_pairs),
            }
        )

    return sorted(levels, key=lambda item: item["zoom"])


def pick_bos_chart_assets(resource_index: dict) -> tuple[dict, dict]:
    bos_plates = [record for record in resource_index["plates"] if record["airport_id"] == "BOS"]
    bos_csups = [record for record in resource_index["csups"] if record["airport_id"] == "BOS"]
    plate_record = next(
        (record for record in bos_plates if record["label"] == "IAP-MA-ILS OR LOC RWY 04R"),
        bos_plates[0] if bos_plates else None,
    )
    if plate_record is None:
        raise RuntimeError("no BOS plate records found in resource index")
    csup_record = bos_csups[0] if bos_csups else None
    if csup_record is None:
        raise RuntimeError("no BOS csup records found in resource index")
    return plate_record, csup_record


def stage_chart_assets(plate_record: dict, csup_record: dict) -> tuple[dict, dict]:
    plate_source = TPP_ROOT / plate_record["asset_path"]
    csup_source = CSUP_ROOT / csup_record["asset_path"]
    if not plate_source.exists():
        raise RuntimeError(f"missing plate asset {plate_source}")
    if not csup_source.exists():
        raise RuntimeError(f"missing csup asset {csup_source}")

    destinations = [
        WEB_CHART_ASSET_ROOT / "BOS",
        ANDROID_CHART_ASSET_ROOT / "BOS",
    ]
    for directory in destinations:
        directory.mkdir(parents=True, exist_ok=True)
        shutil.copy2(plate_source, directory / plate_source.name)
        shutil.copy2(csup_source, directory / csup_source.name)

    return (
        {
            "id": "plate:bos-ils04r",
            "airport_id": "BOS",
            "label": plate_record["label"],
            "kind": "plate",
            "asset_path": f"chart-assets/BOS/{plate_source.name}",
            "asset_url": f"/chart-assets/BOS/{plate_source.name}",
        },
        {
            "id": "csup:bos",
            "airport_id": "BOS",
            "label": "CSup",
            "kind": "csup",
            "asset_path": f"chart-assets/BOS/{csup_source.name}",
            "asset_url": f"/chart-assets/BOS/{csup_source.name}",
        },
    )


def load_selected_sectional_packages() -> list[SectionalPackage]:
    resource_index = load_resource_index()
    selected = []
    for entry in resource_index["packages"]:
        if entry["family_id"] != "sectional":
            continue
        region = entry["region_id"].lower()
        if region not in SECTIONAL_REGIONS:
            continue
        selected.append(
            SectionalPackage(
                manifest=entry["id"],
                region=region,
                artifact_path=Path(entry["artifact_path"]),
                zip_sha256=entry["checksum_sha256"],
            )
        )
    selected.sort(key=lambda package: SECTIONAL_REGIONS.index(package.region))
    if len(selected) != len(SECTIONAL_REGIONS):
        raise RuntimeError(f"expected {SECTIONAL_REGIONS}, got {[package.region for package in selected]}")
    return selected


def load_selected_tac_packages() -> list[TacPackage]:
    resource_index = load_resource_index()
    selected = []
    for entry in resource_index["packages"]:
        if entry["family_id"] != "tac":
            continue
        region = entry["region_id"].lower()
        if region not in TAC_REGIONS:
            continue
        selected.append(
            TacPackage(
                manifest=entry["id"],
                region=region,
                artifact_path=Path(entry["artifact_path"]),
                zip_sha256=entry["checksum_sha256"],
            )
        )
    selected.sort(key=lambda package: TAC_REGIONS.index(package.region))
    if len(selected) != len(TAC_REGIONS):
        raise RuntimeError(f"expected {TAC_REGIONS}, got {[package.region for package in selected]}")
    return selected


def clear_sectional_web_root() -> None:
    WEB_SECTIONAL_ROOT.mkdir(parents=True, exist_ok=True)


def clear_directory(root: Path) -> None:
    if not root.exists():
        return
    for child in sorted(root.iterdir(), reverse=True):
        if child.is_dir() and not child.is_symlink():
            shutil.rmtree(child, ignore_errors=True)
            if child.exists():
                clear_directory(child)
                os.rmdir(child)
        else:
            child.unlink(missing_ok=True)


def extract_zip_for_web(package: SectionalPackage) -> None:
    target_dir = WEB_SECTIONAL_ROOT / package.manifest
    target_dir.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(package.artifact_path) as archive:
        extract_zip(archive, target_dir)


def extract_tac_zip_for_web(package: TacPackage) -> None:
    target_dir = WEB_SECTIONAL_ROOT / package.manifest
    target_dir.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(package.artifact_path) as archive:
        extract_zip(archive, target_dir)


def extract_zip(archive: zipfile.ZipFile, target_dir: Path) -> None:
    for member in archive.infolist():
        target_path = target_dir / member.filename
        if member.is_dir():
            target_path.mkdir(parents=True, exist_ok=True)
            continue
        target_path.parent.mkdir(parents=True, exist_ok=True)
        with archive.open(member) as source, target_path.open("wb") as target:
            shutil.copyfileobj(source, target)


def compute_level_bounds_from_zip(package: SectionalPackage, chart_index: int = 0) -> list[dict]:
    zoom_levels: dict[int, dict[str, list[int]]] = {}
    with zipfile.ZipFile(package.artifact_path) as archive:
        for name in archive.namelist():
            parts = name.split("/")
            if len(parts) != 5 or parts[0] != "tiles" or parts[1] != str(chart_index) or not parts[-1].endswith(".webp"):
                continue
            zoom = int(parts[2])
            x = int(parts[3])
            y_tms = int(parts[4].removesuffix(".webp"))
            zoom_levels.setdefault(zoom, {"x": [], "y": []})
            zoom_levels[zoom]["x"].append(x)
            zoom_levels[zoom]["y"].append(y_tms)

    return [
        {
            "zoom": zoom,
            "x_min": min(values["x"]),
            "x_max": max(values["x"]),
            "y_tms_min": min(values["y"]),
            "y_tms_max": max(values["y"]),
        }
        for zoom, values in sorted(zoom_levels.items())
    ]


def compute_level_bounds_from_tac_zip(package: TacPackage, chart_index: int = 1) -> list[dict]:
    zoom_levels: dict[int, dict[str, list[int]]] = {}
    with zipfile.ZipFile(package.artifact_path) as archive:
        for name in archive.namelist():
            parts = name.split("/")
            if len(parts) != 5 or parts[0] != "tiles" or parts[1] != str(chart_index) or not parts[-1].endswith(".webp"):
                continue
            zoom = int(parts[2])
            x = int(parts[3])
            y_tms = int(parts[4].removesuffix(".webp"))
            zoom_levels.setdefault(zoom, {"x": [], "y": []})
            zoom_levels[zoom]["x"].append(x)
            zoom_levels[zoom]["y"].append(y_tms)

    return [
        {
            "zoom": zoom,
            "x_min": min(values["x"]),
            "x_max": max(values["x"]),
            "y_tms_min": min(values["y"]),
            "y_tms_max": max(values["y"]),
        }
        for zoom, values in sorted(zoom_levels.items())
    ]


def inverse_web_mercator(world_x: float, world_y: float) -> tuple[float, float]:
    lon = (world_x / TILE_SIZE) * 360.0 - 180.0
    n = math.pi - (2.0 * math.pi * world_y) / TILE_SIZE
    lat = math.degrees(math.atan(math.sinh(n)))
    return lat, lon


def center_lat_lon_for_levels(levels: list[dict]) -> tuple[float, float]:
    level = max(levels, key=lambda item: item["zoom"])
    scale = 2 ** level["zoom"]
    y_xyz_min = (scale - 1) - level["y_tms_max"]
    y_xyz_max = (scale - 1) - level["y_tms_min"]
    tile_world_size = TILE_SIZE / scale
    world_x = ((level["x_min"] + level["x_max"] + 1) / 2.0) * tile_world_size
    world_y = ((y_xyz_min + y_xyz_max + 1) / 2.0) * tile_world_size
    return inverse_web_mercator(world_x, world_y)


def build_sectional_map_option(package: SectionalPackage) -> dict:
    levels = compute_level_bounds_from_zip(package)
    center_lat, center_lon = center_lat_lon_for_levels(levels)
    label = f"{package.region.upper()} Sectional"
    return {
        "id": f"sectional:{package.region}",
        "label": label,
        "region_id": package.region,
        "map_view": {
            "chart_family": "sectional",
            "chart_name": label,
            "chart_index": 0,
            "tile_root": "tiles",
            "tile_url_root": f"/sectional-packages/{package.manifest}/tiles",
            "tile_size": TILE_SIZE,
            "min_zoom": 4.2,
            "max_zoom": 10.8,
            "storage_kind": "sectional_package",
            "package_name": package.manifest,
            "initial_viewport": {
                "lat": center_lat,
                "lon": center_lon,
                "zoom": 7.2,
            },
            "levels": levels,
        },
    }


def build_tac_map_option(package: TacPackage) -> dict:
    levels = compute_level_bounds_from_tac_zip(package)
    center_lat, center_lon = center_lat_lon_for_levels(levels)
    label = f"{package.region.upper()} TAC"
    return {
        "id": f"tac:{package.region}",
        "label": label,
        "region_id": package.region,
        "map_view": {
            "chart_family": "tac",
            "chart_name": label,
            "chart_index": 1,
            "tile_root": "tiles",
            "tile_url_root": f"/sectional-packages/{package.manifest}/tiles",
            "tile_size": TILE_SIZE,
            "min_zoom": 4.2,
            "max_zoom": 11.8,
            "storage_kind": "sectional_package",
            "package_name": package.manifest,
            "initial_viewport": {
                "lat": center_lat,
                "lon": center_lon,
                "zoom": 7.4,
            },
            "levels": levels,
        },
    }


def build_fixture() -> dict:
    resource_index = load_resource_index()
    cycle = resource_index["cycle"]
    boston_tac_polygon = load_wgs84_polygon(BOSTON_TAC_GEOJSON)
    probe_lat, probe_lon = bounds_center(boston_tac_polygon)
    tile_windows = {}
    for zoom, radius in TAC_TILE_LEVELS.items():
        center_x, _center_y_xyz, center_y_tms, _offset_x, _offset_y = web_mercator_tile(probe_lat, probe_lon, zoom)
        tile_windows[zoom] = (center_x, center_y_tms, radius)
    tac_level_bounds = copy_tac_tile_subset(tile_windows)

    selected_sectional_packages = load_selected_sectional_packages()
    selected_tac_packages = load_selected_tac_packages()
    plate_record, csup_record = pick_bos_chart_assets(resource_index)
    plate_asset, csup_asset = stage_chart_assets(plate_record, csup_record)
    clear_sectional_web_root()
    for package in selected_sectional_packages:
        extract_zip_for_web(package)
    for package in selected_tac_packages:
        extract_tac_zip_for_web(package)
    sectional_map_views = [build_sectional_map_option(package) for package in selected_sectional_packages]
    tac_map_views = [build_tac_map_option(package) for package in selected_tac_packages]
    default_sectional_view = sectional_map_views[0]["map_view"]
    default_sectional_level = max(default_sectional_view["levels"], key=lambda item: item["zoom"])

    packages = []
    regions_seen = set()
    for entry in resource_index["packages"]:
        region = entry["region_id"].lower()
        family_id = entry["family_id"]
        if family_id not in {"sectional", "tac"}:
            continue
        if family_id == "sectional" and region not in SECTIONAL_REGIONS:
            continue
        if family_id == "tac" and region not in TAC_REGIONS:
            continue
        regions_seen.add(region)
        packages.append(
            {
                "id": {
                    "region": region,
                    "family": family_id,
                    "cycle": cycle,
                },
                "package_name": entry["id"],
                "family_id": family_id,
                "region_id": region,
                "cycle": cycle,
                "artifact_kind": "zip",
                "relative_url": f"/{cycle}/{Path(entry['artifact_path']).name}",
                "manifest_name": entry["id"],
                "size_bytes": entry["size_bytes"],
                "checksum_sha256": entry["checksum_sha256"],
            }
        )

    packages.sort(key=lambda item: (item["family_id"], REGION_ORDER.index(item["region_id"])))

    regions = [
        {
            "id": region,
            "display_name": REGION_DISPLAY_NAMES[region],
            "sort_order": REGION_ORDER.index(region),
        }
        for region in REGION_ORDER
        if region in regions_seen
    ]

    procedure_code = plate_record["label"]
    procedure_name = plate_record["label"]
    asset_base_path = str(Path(plate_record["asset_path"]).with_suffix(""))

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
        "map_view": default_sectional_view,
        "map_views": [*sectional_map_views, *tac_map_views],
        "chart_page": {
            "recent_airport_ids": ["BOS"],
            "initial_airport_id": "BOS",
            "initial_chart_id": plate_asset["id"],
            "airports": [
                {
                    "id": "BOS",
                    "label": "KBOS",
                    "charts": [plate_asset, csup_asset],
                }
            ],
        },
        "initial_probe": {
            "family": "tac",
            "lat": 42.3656,
            "lon": -71.0096,
        },
        "map_tile_view": {
            "chart_family": "sectional",
            "chart_name": default_sectional_view["chart_name"],
            "chart_index": default_sectional_view["chart_index"],
            "tile_root": default_sectional_view["tile_root"],
            "zoom": default_sectional_level["zoom"],
            "tile_size": TILE_SIZE,
            "radius": 0,
            "center_x": (default_sectional_level["x_min"] + default_sectional_level["x_max"]) // 2,
            "center_y_tms": (default_sectional_level["y_tms_min"] + default_sectional_level["y_tms_max"]) // 2,
            "probe_offset_x": 0.0,
            "probe_offset_y": 0.0,
        },
        "flight_plan": {
            "id": "plan-1",
            "name": "KRNT SEA PAE KAWO",
            "legs": [
                {
                    "from": {"Airport": "RNT"},
                    "to": {"Airport": "SEA"},
                    "airway": None,
                },
                {
                    "from": {"Airport": "SEA"},
                    "to": {"Airport": "PAE"},
                    "airway": None,
                },
                {
                    "from": {"Airport": "PAE"},
                    "to": {"Airport": "AWO"},
                    "airway": None,
                }
            ],
            "departure": "RNT",
            "destination": "AWO",
            "alternate": None,
            "cruise_altitude_ft": 3000,
            "notes": "Generated from resource-index data",
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
                        "region": selected_sectional_packages[0].region,
                        "family": "sectional",
                        "cycle": cycle,
                    },
                    "integrity_ok": True,
                }
            ],
            "cached_tilesets": [],
            "cached_plates": [],
        },
        "tac_demo": {
            "chart_name": "Boston TAC",
            "tile_root": "charts-tac",
            "tile_url_root": "/prototype-tiles/charts-tac",
            "chart_index": 1,
            "levels": tac_level_bounds,
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
    resource_index = load_resource_index()
    write_json(CANONICAL_RESOURCE_INDEX_OUT, resource_index)
    write_json(WEB_RESOURCE_INDEX_OUT, resource_index)
    write_json(ANDROID_RESOURCE_INDEX_OUT, resource_index)


if __name__ == "__main__":
    main()
