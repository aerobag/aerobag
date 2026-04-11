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
UI_TARGET_ROOT_FILE = ROOT / "ui" / "target-root.txt"
ARTIFACT_ROOT_CONFIG = ROOT / ".aerobag-artifact-root"


def resolve_artifact_root() -> Path:
    manifest_relative = Path("product-builds") / "production" / "product-build.json"
    env_value = os.environ.get("AEROBAG_ARTIFACT_ROOT")
    if env_value:
        candidate = Path(env_value).expanduser()
        if candidate.joinpath(manifest_relative).exists():
            return candidate
    configured = ARTIFACT_ROOT_CONFIG.read_text().strip()
    path = Path(configured)
    candidate = path if path.is_absolute() else (ROOT / path).resolve()
    if candidate.joinpath(manifest_relative).exists():
        return candidate
    fallback = Path("/root/aerobag-artifacts")
    if fallback.joinpath(manifest_relative).exists():
        return fallback
    return candidate


ARTIFACT_ROOT = resolve_artifact_root()
UI_TARGET_ROOT = Path(
    os.environ.get(
        "AEROBAG_UI_TARGET_ROOT",
        (ROOT / UI_TARGET_ROOT_FILE.read_text().strip()).resolve(),
    ),
).expanduser()
UI_DIR = ROOT / "ui"
PRODUCT_BUILD = ARTIFACT_ROOT / "product-builds" / "production" / "product-build.json"
UI_THEME = UI_DIR / "shared-fixtures" / "ui-theme.json"
SHARED_TARGET_ROOT = UI_TARGET_ROOT / "shared" / "content-prototype"
WEB_TARGET_ROOT = UI_TARGET_ROOT / "web"
ANDROID_TARGET_ROOT = UI_TARGET_ROOT / "android"
CANONICAL_OUT = SHARED_TARGET_ROOT / "content_fixture.json"
CANONICAL_RESOURCE_INDEX_OUT = SHARED_TARGET_ROOT / "resource-index.json"
CANONICAL_THEME_OUT = SHARED_TARGET_ROOT / "ui-theme.json"
WEB_OUT = WEB_TARGET_ROOT / "generated" / "contentFixture.json"
WEB_RESOURCE_INDEX_OUT = WEB_TARGET_ROOT / "generated" / "resourceIndex.json"
WEB_THEME_OUT = WEB_TARGET_ROOT / "generated" / "uiTheme.json"
ANDROID_OUT = ANDROID_TARGET_ROOT / "assets" / "fixtures" / "contentFixture.json"
ANDROID_RESOURCE_INDEX_OUT = ANDROID_TARGET_ROOT / "assets" / "fixtures" / "resource-index.json"
ANDROID_THEME_OUT = ANDROID_TARGET_ROOT / "assets" / "fixtures" / "ui-theme.json"
CANONICAL_TILE_ROOT = SHARED_TARGET_ROOT / "tiles"
WEB_TILE_ROOT = WEB_TARGET_ROOT / "prototype-tiles"
ANDROID_TILE_ROOT = ANDROID_TARGET_ROOT / "assets" / "tiles"
WEB_SECTIONAL_ROOT = WEB_TARGET_ROOT / "generated-static" / "sectional-packages"
WEB_CHART_ASSET_ROOT = WEB_TARGET_ROOT / "generated-static" / "chart-assets"
WEB_CHART_ASSET_MANIFEST = WEB_TARGET_ROOT / "generated-static" / "chart-assets-manifest.json"
WEB_VECTOR_ROOT = WEB_TARGET_ROOT / "generated-static" / "vectors"
ANDROID_CHART_ASSET_ROOT = ANDROID_TARGET_ROOT / "generated-seed" / "chart-assets"
ANDROID_LEGACY_CHART_ASSET_ROOT = ANDROID_TARGET_ROOT / "assets" / "chart-assets"
WEB_NAV_DB_ROOT = WEB_TARGET_ROOT / "generated-static" / "nav-db"
ANDROID_NAV_DB_ROOT = ANDROID_TARGET_ROOT / "assets" / "nav-db"
VECTOR_OUTPUT_ROOT = ARTIFACT_ROOT / "product-builds" / "shared" / "work" / "vectors-2604" / "output"
FIX_VECTOR_TILE_ROOT = VECTOR_OUTPUT_ROOT / "points" / "fix" / "9"
CHARTS_TAC_BUILD_RECORD = ARTIFACT_ROOT / "product-builds" / "shared" / "work" / "charts-tac-2603" / "build-record.json"

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
SUPPORTED_TILED_FAMILIES = ("sectional", "tac", "ifr_low", "ifr_high")

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
class TiledPackage:
    family_id: str
    manifest: str
    region: str
    artifact_path: Path
    zip_sha256: str


def resolve_charts_tac_work_dir() -> Path:
    if CHARTS_TAC_BUILD_RECORD.exists():
        payload = json.loads(CHARTS_TAC_BUILD_RECORD.read_text())
        outputs = payload.get("outputs")
        if isinstance(outputs, dict):
            work_dir = outputs.get("work_dir")
            if isinstance(work_dir, str) and work_dir:
                candidate = ARTIFACT_ROOT / work_dir
                if candidate.is_dir():
                    return candidate
    fallback = ARTIFACT_ROOT / "product-builds" / "shared" / "work" / "charts-tac-2603" / "work" / "charts-tac"
    if fallback.is_dir():
        return fallback
    raise RuntimeError(f"missing charts-tac work dir from {CHARTS_TAC_BUILD_RECORD}")


def load_product_build_manifest() -> dict:
    if not PRODUCT_BUILD.exists():
        raise RuntimeError(f"missing product build manifest {PRODUCT_BUILD}")
    payload = json.loads(PRODUCT_BUILD.read_text())
    nodes = payload.get("nodes")
    if not isinstance(nodes, list):
        raise RuntimeError(f"invalid product build manifest {PRODUCT_BUILD}: missing nodes[]")
    return payload


def resolve_product_build_output(node_name: str, output_name: str) -> Path:
    payload = load_product_build_manifest()
    for node in payload["nodes"]:
        if not isinstance(node, dict) or node.get("name") != node_name:
            continue
        outputs = node.get("outputs")
        if not isinstance(outputs, dict):
            break
        value = outputs.get(output_name)
        if not isinstance(value, str) or not value:
            break
        path = ARTIFACT_ROOT / value
        if not path.exists():
            raise RuntimeError(f"missing {node_name}.{output_name} output {path}")
        return path
    raise RuntimeError(f"{node_name} node missing outputs.{output_name} in {PRODUCT_BUILD}")


CHARTS_TAC_WORK_DIR = resolve_charts_tac_work_dir()
BOSTON_TAC_GEOJSON = CHARTS_TAC_WORK_DIR / "TAC" / "Boston TAC.geojson"
BOSTON_TAC_TILE_ROOT = CHARTS_TAC_WORK_DIR / "tiles" / "1"
PRODUCT_MAIN_DB = resolve_product_build_output("data", "main_db")


def load_resource_index() -> dict:
    payload = json.loads(resolve_resource_index_path().read_text())
    family_map = {
        "sec": "sectional",
        "enr-l": "ifr_low",
        "enr-h": "ifr_high",
    }
    for family in payload.get("families", []):
        family["id"] = family_map.get(family["id"], family["id"])
    for package in payload.get("packages", []):
        package["family_id"] = family_map.get(package["family_id"], package["family_id"])
    for collection in payload.get("chart_collections", []):
        collection["family_id"] = family_map.get(collection["family_id"], collection["family_id"])
    return payload


def resolve_resource_index_path() -> Path:
    return resolve_product_build_output("resource-index", "resource_index")


def resolve_package_artifact_path(path_value: str) -> Path:
    path = Path(path_value)
    if path.is_absolute():
        if path.is_file():
            return path
        marker = f"{os.sep}product-builds{os.sep}"
        raw = str(path).replace("\\", os.sep)
        marker_index = raw.find(marker)
        if marker_index >= 0:
            relative = raw[marker_index + len(marker):]
            rebased = ARTIFACT_ROOT / "product-builds" / relative
            candidates = [rebased]
            normalized = relative.replace("\\", "/")
            if normalized.startswith("shared/"):
                suffix = normalized.removeprefix("shared/")
                candidates.append(ARTIFACT_ROOT / "product-builds" / "validation" / suffix)
                candidates.append(ARTIFACT_ROOT / "product-builds" / "production" / suffix)
            elif normalized.startswith("validation/"):
                suffix = normalized.removeprefix("validation/")
                candidates.append(ARTIFACT_ROOT / "product-builds" / "shared" / suffix)
            elif normalized.startswith("production/"):
                suffix = normalized.removeprefix("production/")
                candidates.append(ARTIFACT_ROOT / "product-builds" / "shared" / suffix)
            for candidate in candidates:
                if candidate.is_file():
                    return candidate
        return path
    return ARTIFACT_ROOT / "product-builds" / path


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
            shutil.rmtree(target_root, ignore_errors=True)


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

        if not available_pairs:
            continue

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


def chart_asset(record: dict, kind: str) -> dict:
    airport_id = record["airport_id"]
    source_path = Path(record["asset_path"])
    thumbnail_path = record.get("thumbnail_path")
    thumbnail_name = Path(thumbnail_path).name if thumbnail_path else None
    label = "CSup" if kind == "csup" else record["label"]
    return {
        "id": f"{kind}:{airport_id}:{source_path.name}",
        "airport_id": airport_id,
        "label": label,
        "kind": kind,
        "asset_path": f"chart-assets/{airport_id}/{source_path.name}",
        "asset_url": f"/chart-assets/{airport_id}/{source_path.name}",
        "thumbnail_path": f"chart-thumbnails/{airport_id}/{thumbnail_name}" if thumbnail_name else None,
        "thumbnail_url": f"/chart-thumbnails/{airport_id}/{thumbnail_name}" if thumbnail_name else None,
    }


def package_region(package_id: str) -> str:
    return package_id.split("_", 1)[0].lower()


def package_family(package_id: str) -> str:
    return package_id.split("_", 1)[1].lower()


def plate_root_candidates(package_id: str) -> list[Path]:
    region = package_region(package_id)
    return [
        ARTIFACT_ROOT / "product-builds" / "shared" / "work" / f"tpp-{region}" / "work" / f"tpp-{region}",
        ARTIFACT_ROOT / "product-builds" / "production" / "work" / f"tpp-{region}" / "work" / f"tpp-{region}",
        ARTIFACT_ROOT / "product-builds" / "validation" / "work" / f"tpp-{region}" / "work" / f"tpp-{region}",
    ]


def thumbnail_root_candidates() -> list[Path]:
    resource_index_root = resolve_resource_index_path().parent
    return [
        resource_index_root,
        resource_index_root.parent,
        ARTIFACT_ROOT / "product-builds" / "production" / "work" / "resource-index",
        ARTIFACT_ROOT / "product-builds" / "validation" / "work" / "resource-index",
    ]


def csup_root_candidates() -> list[Path]:
    return [
        ARTIFACT_ROOT / "product-builds" / "shared" / "work" / "csup" / "work" / "csup",
        ARTIFACT_ROOT / "product-builds" / "production" / "work" / "csup" / "work" / "csup",
        ARTIFACT_ROOT / "product-builds" / "validation" / "work" / "csup" / "work" / "csup",
    ]


def resolve_chart_record_source(record: dict, kind: str, asset_key: str = "asset_path") -> Path:
    if asset_key == "thumbnail_path":
        candidates = thumbnail_root_candidates()
    else:
        candidates = plate_root_candidates(record["package_id"]) if kind == "plate" else csup_root_candidates()
    for root in candidates:
        source = root / record[asset_key]
        if source.exists():
            return source
    raise RuntimeError(f"missing {kind} asset {record[asset_key]} for {record['package_id']}")


def extract_chart_record_from_package(record: dict, package_artifacts: dict[str, Path], asset_key: str = "asset_path") -> Path:
    artifact_path = package_artifacts.get(record["package_id"])
    if artifact_path is None or not artifact_path.exists():
        package_id = record["package_id"]
        if package_id.endswith("_TPP"):
            region = package_region(package_id)
            artifact_path = ARTIFACT_ROOT / "product-builds" / "shared" / "work" / f"tpp-{region}" / "work" / f"tpp-{region}" / f"{package_id}.zip"
        elif package_id.endswith("_CSUP"):
            artifact_path = ARTIFACT_ROOT / "product-builds" / "shared" / "work" / "csup" / "work" / "csup" / f"{package_id}.zip"
        if artifact_path is None or not artifact_path.exists():
            raise RuntimeError(f"missing package artifact for {record['package_id']}")
    cached_target = WEB_CHART_ASSET_ROOT / "__zipcache__" / record["package_id"] / record[asset_key]
    cached_target.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(artifact_path) as archive:
        try:
            with archive.open(record[asset_key]) as source, cached_target.open("wb") as target:
                shutil.copyfileobj(source, target)
        except KeyError as exc:
            raise RuntimeError(f"missing packaged chart asset {record[asset_key]} for {record['package_id']}") from exc
    return cached_target


def build_web_chart_asset_manifest(resource_index: dict) -> None:
    package_artifacts = {
        package["id"]: resolve_package_artifact_path(package["artifact_path"])
        for package in resource_index["packages"]
    }
    manifest = {}
    for kind, records in (("plate", resource_index["plates"]), ("csup", resource_index["csups"])):
        for record in records:
            try:
                source = resolve_chart_record_source(record, kind)
            except RuntimeError:
                source = extract_chart_record_from_package(record, package_artifacts)
            airport_id = record["airport_id"]
            filename = Path(record["asset_path"]).name
            manifest[f"/chart-assets/{airport_id}/{filename}"] = str(source)
            thumbnail_path = record.get("thumbnail_path")
            if thumbnail_path:
                try:
                    thumbnail_source = resolve_chart_record_source(record, kind, "thumbnail_path")
                except RuntimeError:
                    thumbnail_source = extract_chart_record_from_package(record, package_artifacts, "thumbnail_path")
                manifest[f"/chart-thumbnails/{airport_id}/{Path(thumbnail_path).name}"] = str(thumbnail_source)
    WEB_CHART_ASSET_MANIFEST.parent.mkdir(parents=True, exist_ok=True)
    WEB_CHART_ASSET_MANIFEST.write_text(json.dumps(manifest, indent=2, sort_keys=True))


def select_chart_airports(resource_index: dict, plan_airport_ids: list[str]) -> list[str]:
    airport_resources = {entry["airport_id"]: entry for entry in resource_index["airport_resources"]}
    recent_airport_ids = []
    for airport_id in plan_airport_ids:
        if airport_id in airport_resources:
            recent_airport_ids.append(airport_id)

    first_plate_airport = next(
        (entry["airport_id"] for entry in resource_index["airport_resources"] if entry["plate_ids"]),
        None,
    )
    if first_plate_airport and first_plate_airport not in recent_airport_ids:
        recent_airport_ids.append(first_plate_airport)
    if not recent_airport_ids:
        raise RuntimeError("no chart-page airports found in resource index")
    return recent_airport_ids


def stage_nav_db() -> None:
    if not PRODUCT_MAIN_DB.exists():
        raise RuntimeError(f"missing nav db {PRODUCT_MAIN_DB}")
    for directory in [WEB_NAV_DB_ROOT, ANDROID_NAV_DB_ROOT]:
        directory.mkdir(parents=True, exist_ok=True)
        shutil.copy2(PRODUCT_MAIN_DB, directory / "main.db")


def stage_web_vectors() -> None:
    clear_directory(WEB_VECTOR_ROOT)
    WEB_VECTOR_ROOT.mkdir(parents=True, exist_ok=True)
    if not FIX_VECTOR_TILE_ROOT.exists():
        raise RuntimeError(f"missing vector tile root {FIX_VECTOR_TILE_ROOT}")
    shutil.copytree(FIX_VECTOR_TILE_ROOT, WEB_VECTOR_ROOT / "points" / "fix" / "9")


def family_display_name(resource_index: dict, family_id: str) -> str:
    return next(
        (entry["display_name"] for entry in resource_index["families"] if entry["id"] == family_id),
        family_id.upper(),
    )


def load_supported_tiled_packages(resource_index: dict) -> list[TiledPackage]:
    packages = []
    for entry in resource_index["packages"]:
        family_id = entry["family_id"]
        if family_id not in SUPPORTED_TILED_FAMILIES:
            continue
        packages.append(
            TiledPackage(
                family_id=family_id,
                manifest=entry["id"],
                region=entry["region_id"].lower(),
                artifact_path=resolve_package_artifact_path(entry["artifact_path"]),
                zip_sha256=entry["checksum_sha256"],
            )
        )
    packages.sort(key=lambda package: (SUPPORTED_TILED_FAMILIES.index(package.family_id), REGION_ORDER.index(package.region)))
    return packages


def load_supported_chart_collections(resource_index: dict) -> list[dict]:
    collections = [entry for entry in resource_index["chart_collections"] if entry["family_id"] in SUPPORTED_TILED_FAMILIES]
    collections.sort(key=lambda entry: (SUPPORTED_TILED_FAMILIES.index(entry["family_id"]), REGION_ORDER.index(entry["region_id"])))
    return collections


def package_by_id(packages: list[TiledPackage]) -> dict[str, TiledPackage]:
    return {package.manifest: package for package in packages}


def clear_sectional_web_root() -> None:
    clear_directory(WEB_SECTIONAL_ROOT)
    WEB_SECTIONAL_ROOT.mkdir(parents=True, exist_ok=True)


def clear_directory(root: Path) -> None:
    if not root.exists():
        return
    if root.is_dir() and not root.is_symlink():
        shutil.rmtree(root, ignore_errors=True)
        return
    root.unlink(missing_ok=True)


def extract_zip_for_web(package: TiledPackage) -> None:
    target_dir = WEB_SECTIONAL_ROOT / package.manifest
    shutil.rmtree(target_dir, ignore_errors=True)
    target_dir.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(package.artifact_path) as archive:
        extract_zip(archive, target_dir)


def extract_zip(archive: zipfile.ZipFile, target_dir: Path) -> None:
    for member in archive.infolist():
        target_path = target_dir / member.filename
        if member.is_dir():
            if target_path.exists() and not target_path.is_dir():
                target_path.unlink()
            target_path.mkdir(parents=True, exist_ok=True)
            continue
        if target_path.parent.exists() and not target_path.parent.is_dir():
            target_path.parent.unlink()
        target_path.parent.mkdir(parents=True, exist_ok=True)
        if target_path.exists() and target_path.is_dir():
            shutil.rmtree(target_path, ignore_errors=True)
        with archive.open(member) as source, target_path.open("wb") as destination:
            shutil.copyfileobj(source, destination)


def compute_level_bounds_from_zip(package: TiledPackage, chart_index: int = 0) -> list[dict]:
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


def build_map_option(resource_index: dict, collection: dict, package: TiledPackage) -> dict:
    levels = collection["levels"]
    center_lat = collection["default_view"]["lat"]
    center_lon = collection["default_view"]["lon"]
    family_id = collection["family_id"]
    label = f"{REGION_DISPLAY_NAMES.get(package.region, package.region.upper())} {family_display_name(resource_index, family_id)}"
    max_zoom = max(level["zoom"] for level in levels) + 0.8
    min_zoom = max(1.5, min(level["zoom"] for level in levels) - 2.8)
    return {
        "id": collection["id"],
        "label": label,
        "region_id": package.region,
        "map_view": {
            "chart_family": family_id,
            "chart_name": label,
            "chart_index": collection["chart_index"],
            "tile_root": "tiles",
            "tile_url_root": f"/sectional-packages/{package.manifest}/tiles",
            "tile_size": TILE_SIZE,
            "min_zoom": min_zoom,
            "max_zoom": max_zoom,
            "storage_kind": "sectional_package",
            "package_name": package.manifest,
            "initial_viewport": {
                "lat": center_lat,
                "lon": center_lon,
                "zoom": collection["default_view"]["zoom"],
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

    supported_tiled_packages = load_supported_tiled_packages(resource_index)
    supported_chart_collections = load_supported_chart_collections(resource_index)
    packages_by_id = package_by_id(supported_tiled_packages)
    sample_plan_airports = ["KRNT", "KSEA", "KPAE", "KAWO"]
    chart_airport_ids = select_chart_airports(resource_index, sample_plan_airports)
    clear_directory(WEB_CHART_ASSET_ROOT)
    WEB_CHART_ASSET_ROOT.mkdir(parents=True, exist_ok=True)
    clear_directory(ANDROID_LEGACY_CHART_ASSET_ROOT)
    clear_directory(ANDROID_CHART_ASSET_ROOT)
    ANDROID_CHART_ASSET_ROOT.mkdir(parents=True, exist_ok=True)
    build_web_chart_asset_manifest(resource_index)
    stage_nav_db()
    stage_web_vectors()
    clear_sectional_web_root()
    for package in supported_tiled_packages:
        extract_zip_for_web(package)
    map_views = [
        build_map_option(resource_index, collection, packages_by_id[collection["package_id"]])
        for collection in supported_chart_collections
        if collection["package_id"] in packages_by_id
    ]
    default_map_view = next(
        (entry["map_view"] for entry in map_views if entry["map_view"]["chart_family"] == "sectional"),
        map_views[0]["map_view"],
    )
    default_map_level = max(default_map_view["levels"], key=lambda item: item["zoom"])
    default_sectional_package = next(
        (package for package in supported_tiled_packages if package.family_id == "sectional"),
        None,
    )

    packages = []
    regions_seen = set()
    for entry in resource_index["packages"]:
        region = entry["region_id"].lower()
        family_id = entry["family_id"]
        if family_id not in SUPPORTED_TILED_FAMILIES:
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

    initial_airport_id = next(
        (airport_id for airport_id in chart_airport_ids if any(record["airport_id"] == airport_id for record in resource_index["plates"])),
        chart_airport_ids[0],
    )
    seed_plate_record = next(
        (record for record in resource_index["plates"] if record["airport_id"] == initial_airport_id),
        None,
    )
    procedure_code = seed_plate_record["label"] if seed_plate_record else "UNKNOWN"
    procedure_name = seed_plate_record["label"] if seed_plate_record else "UNKNOWN"
    asset_base_path = str(Path(seed_plate_record["asset_path"]).with_suffix("")) if seed_plate_record else ""

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
                {
                    "id": "ifr_low",
                    "display_name": "IFR Low Enroute Charts",
                    "kind": "tiled_raster",
                    "max_zoom": 10,
                    "tile_size": 512,
                },
                {
                    "id": "ifr_high",
                    "display_name": "IFR High Enroute Charts",
                    "kind": "tiled_raster",
                    "max_zoom": 10,
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
            "plates": (
                [
                    {
                        "id": {
                            "airport_id": initial_airport_id,
                            "procedure_code": procedure_code,
                            "page": 1,
                            "cycle": cycle,
                        },
                        "airport_id": initial_airport_id,
                        "region_id": "ne",
                        "cycle": cycle,
                        "procedure_code": procedure_code,
                        "display_name": procedure_name,
                        "kind": "approach",
                        "georeferenced": True,
                        "page_count": 1,
                        "asset_base_path": asset_base_path,
                    }
                ]
                if seed_plate_record
                else []
            ),
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
        "map_view": default_map_view,
        "map_views": map_views,
        "initial_probe": {
            "family": default_map_view["chart_family"],
            "lat": default_map_view["initial_viewport"]["lat"],
            "lon": default_map_view["initial_viewport"]["lon"],
        },
        "map_tile_view": {
            "chart_family": default_map_view["chart_family"],
            "chart_name": default_map_view["chart_name"],
            "chart_index": default_map_view["chart_index"],
            "tile_root": default_map_view["tile_root"],
            "zoom": default_map_level["zoom"],
            "tile_size": TILE_SIZE,
            "radius": 0,
            "center_x": (default_map_level["x_min"] + default_map_level["x_max"]) // 2,
            "center_y_tms": (default_map_level["y_tms_min"] + default_map_level["y_tms_max"]) // 2,
            "probe_offset_x": 0.0,
            "probe_offset_y": 0.0,
        },
        "flight_plan": {
            "id": "plan-1",
            "name": "KRNT SEA PAE KAWO",
            "legs": [
                {
                    "from": {"Airport": "KRNT"},
                    "to": {"Navaid": "SEA"},
                    "airway": None,
                },
                {
                    "from": {"Navaid": "SEA"},
                    "to": {"Navaid": "PAE"},
                    "airway": None,
                },
                {
                    "from": {"Navaid": "PAE"},
                    "to": {"Airport": "KAWO"},
                    "airway": None,
                }
            ],
            "departure": "KRNT",
            "destination": "KAWO",
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
                        "region": default_sectional_package.region if default_sectional_package else "nw",
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
    ui_theme = json.loads(UI_THEME.read_text())
    write_json(CANONICAL_RESOURCE_INDEX_OUT, resource_index)
    write_json(WEB_RESOURCE_INDEX_OUT, resource_index)
    write_json(ANDROID_RESOURCE_INDEX_OUT, resource_index)
    write_json(CANONICAL_THEME_OUT, ui_theme)
    write_json(WEB_THEME_OUT, ui_theme)
    write_json(ANDROID_THEME_OUT, ui_theme)


if __name__ == "__main__":
    main()
