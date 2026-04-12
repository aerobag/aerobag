#!/usr/bin/env python3
from __future__ import annotations

import errno
import json
import os
import shutil
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TARGET_ROOT_FILE = ROOT / "ui" / "target-root.txt"
ARTIFACT_ROOT_CONFIG = ROOT / ".aerobag-artifact-root"
BUNDLE_MANIFEST_DIR = Path("product-builds") / "production"


def resolve_ui_target_root() -> Path:
    env_value = os.environ.get("AEROBAG_UI_TARGET_ROOT")
    if env_value:
        return Path(env_value).expanduser()
    return (ROOT / TARGET_ROOT_FILE.read_text().strip()).resolve()


def resolve_artifact_root() -> Path:
    env_value = os.environ.get("AEROBAG_ARTIFACT_ROOT")
    if env_value:
        candidate = Path(env_value).expanduser()
        if any(candidate.joinpath(BUNDLE_MANIFEST_DIR).glob("bundle_*.json")):
            return candidate
    configured = ARTIFACT_ROOT_CONFIG.read_text().strip()
    candidate = Path(configured)
    if not candidate.is_absolute():
        candidate = (ROOT / candidate).resolve()
    if any(candidate.joinpath(BUNDLE_MANIFEST_DIR).glob("bundle_*.json")):
        return candidate
    fallback = Path("/root/aerobag-artifacts")
    if any(fallback.joinpath(BUNDLE_MANIFEST_DIR).glob("bundle_*.json")):
        return fallback
    return candidate


ARTIFACT_ROOT = resolve_artifact_root()
UI_TARGET_ROOT = resolve_ui_target_root()
WEB_STATIC_ROOT = UI_TARGET_ROOT / "web" / "generated-static"


def latest_bundle_manifest(root: Path) -> Path:
    manifests = sorted(root.joinpath(BUNDLE_MANIFEST_DIR).glob("bundle_*.json"))
    if not manifests:
        raise RuntimeError(f"missing bundle_*.json under {root.joinpath(BUNDLE_MANIFEST_DIR)}")
    return manifests[-1]


PRODUCT_BUILD_FILE = latest_bundle_manifest(ARTIFACT_ROOT)


def load_product_build() -> dict:
    return json.loads(PRODUCT_BUILD_FILE.read_text())


PRODUCT_BUILD = load_product_build()


def resolve_product_build_output(node_name: str, output_name: str) -> Path:
    for node in PRODUCT_BUILD.get("nodes", []):
        if not isinstance(node, dict) or node.get("name") != node_name:
            continue
        outputs = node.get("outputs")
        if not isinstance(outputs, dict):
            break
        raw_path = outputs.get(output_name)
        if not isinstance(raw_path, str) or not raw_path:
            break
        resolved = ARTIFACT_ROOT / raw_path
        if resolved.exists():
            return resolved
        raise RuntimeError(f"missing product build output {node_name}.{output_name}: {resolved}")
    raise RuntimeError(f"missing product build output {node_name}.{output_name} in {PRODUCT_BUILD_FILE}")


def resolve_product_build_node(node_name: str) -> dict:
    for node in PRODUCT_BUILD.get("nodes", []):
        if isinstance(node, dict) and node.get("name") == node_name:
            return node
    raise RuntimeError(f"missing product build node {node_name} in {PRODUCT_BUILD_FILE}")


RESOURCE_INDEX_PATH = resolve_product_build_output("resource-index", "resource_index")
VECTOR_ROOT = ARTIFACT_ROOT / "product-builds" / "shared" / "work" / "vectors-2604" / "output"
NAV_DB_PATH = resolve_product_build_output("data", "main_db")


def reset_dir(path: Path) -> None:
    if path.exists() or path.is_symlink():
        if path.is_dir() and not path.is_symlink():
            shutil.rmtree(path, ignore_errors=True)
        else:
            try:
                path.unlink()
            except FileNotFoundError:
                pass
    path.mkdir(parents=True, exist_ok=True)


def ensure_hard_link(source: Path, target: Path) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists():
        try:
            if source.stat().st_ino == target.stat().st_ino and source.stat().st_dev == target.stat().st_dev:
                return
        except FileNotFoundError:
            pass
        target.unlink()
    try:
        os.link(source, target)
    except OSError as exc:
        if exc.errno == errno.EXDEV:
            shutil.copy2(source, target)
            return
        raise


def ensure_symlink(source: Path, target: Path) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists() or target.is_symlink():
        target.unlink()
    target.symlink_to(source)


def load_resource_index() -> dict:
    payload = json.loads(RESOURCE_INDEX_PATH.read_text())
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


RESOURCE_INDEX = load_resource_index()


def family_tiles_roots() -> dict[str, Path]:
    return {
        "sectional": resolve_product_build_output("charts-sec-render", "tiles_root"),
        "tac": resolve_product_build_output("charts-tac-render", "tiles_root"),
        "ifr_low": resolve_product_build_output("charts-enr-l-render", "tiles_root"),
        "ifr_high": resolve_product_build_output("charts-enr-h-render", "tiles_root"),
    }


def package_region(package_id: str) -> str:
    return package_id.split("_", 1)[0].lower()


def tpp_work_dir(region: str) -> Path:
    node = resolve_product_build_node(f"tpp-{region}-package")
    outputs = node.get("outputs", {})
    if isinstance(outputs, dict):
        raw = outputs.get("work_dir")
        if isinstance(raw, str) and raw:
            return ARTIFACT_ROOT / raw
        raw = outputs.get("manifest")
        if isinstance(raw, str) and raw:
            return (ARTIFACT_ROOT / raw).parent
    raise RuntimeError(f"missing tpp work dir for region {region}")


def csup_work_dir(region: str) -> Path:
    node = resolve_product_build_node(f"csup-package-{region}")
    outputs = node.get("outputs", {})
    if isinstance(outputs, dict):
        raw = outputs.get("work_dir")
        if isinstance(raw, str) and raw:
            return ARTIFACT_ROOT / raw
        raw = outputs.get("manifest")
        if isinstance(raw, str) and raw:
            return (ARTIFACT_ROOT / raw).parent
        raw = outputs.get("zip")
        if isinstance(raw, str) and raw:
            return (ARTIFACT_ROOT / raw).parent
    raise RuntimeError(f"missing csup work dir for region {region}")


def source_chart_asset_path(record: dict, kind: str, asset_key: str) -> Path:
    if asset_key == "thumbnail_path":
        return RESOURCE_INDEX_PATH.parent / record[asset_key]
    region = package_region(record["package_id"])
    if kind == "plate":
        return tpp_work_dir(region) / record[asset_key]
    return csup_work_dir(region) / record[asset_key]


def stage_sectional_packages() -> None:
    package_root = WEB_STATIC_ROOT / "sectional-packages"
    reset_dir(package_root)
    tiles_by_family = family_tiles_roots()
    for package in RESOURCE_INDEX["packages"]:
        family_id = package["family_id"]
        if family_id not in tiles_by_family:
            continue
        ensure_symlink(tiles_by_family[family_id], package_root / package["id"] / "tiles")


def stage_chart_assets() -> None:
    chart_asset_root = WEB_STATIC_ROOT / "chart-assets"
    chart_thumbnail_root = WEB_STATIC_ROOT / "chart-thumbnails"
    reset_dir(chart_asset_root)
    reset_dir(chart_thumbnail_root)
    for kind, records in (("plate", RESOURCE_INDEX.get("plates", [])), ("csup", RESOURCE_INDEX.get("csups", []))):
        for record in records:
            source = source_chart_asset_path(record, kind, "asset_path")
            if not source.is_file():
                raise RuntimeError(f"missing {kind} asset source {source} for {record['package_id']}")
            ensure_hard_link(source, chart_asset_root / record["airport_id"] / Path(record["asset_path"]).name)
            thumbnail_path = record.get("thumbnail_path")
            if thumbnail_path:
                thumbnail_source = source_chart_asset_path(record, kind, "thumbnail_path")
                if not thumbnail_source.is_file():
                    raise RuntimeError(f"missing {kind} thumbnail source {thumbnail_source} for {record['package_id']}")
                ensure_hard_link(thumbnail_source, chart_thumbnail_root / record["airport_id"] / Path(thumbnail_path).name)


def stage_vectors() -> None:
    target = WEB_STATIC_ROOT / "vectors"
    if target.exists() or target.is_symlink():
        if target.is_dir() and not target.is_symlink():
            shutil.rmtree(target)
        else:
            target.unlink()
    target.parent.mkdir(parents=True, exist_ok=True)
    target.symlink_to(VECTOR_ROOT)


def stage_nav_db() -> None:
    nav_root = WEB_STATIC_ROOT / "nav-db"
    reset_dir(nav_root)
    ensure_hard_link(NAV_DB_PATH, nav_root / "main.db")


def main() -> None:
    WEB_STATIC_ROOT.mkdir(parents=True, exist_ok=True)
    stage_sectional_packages()
    stage_chart_assets()
    stage_vectors()
    stage_nav_db()


if __name__ == "__main__":
    main()
