#!/usr/bin/env python3
from __future__ import annotations

import errno
import json
import os
import shutil
import zipfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TARGET_ROOT_FILE = ROOT / "ui" / "target-root.txt"
ARTIFACT_ROOT_CONFIG = ROOT / ".aerobag-artifact-root"
PRODUCTION_MANIFEST_DIR = Path("product-builds") / "production"


def resolve_ui_target_root() -> Path:
    env_value = os.environ.get("AEROBAG_UI_TARGET_ROOT")
    if env_value:
        return Path(env_value).expanduser()
    return (ROOT / TARGET_ROOT_FILE.read_text().strip()).resolve()


def resolve_artifact_root() -> Path:
    env_value = os.environ.get("AEROBAG_ARTIFACT_ROOT")
    if env_value:
        candidate = Path(env_value).expanduser()
        if any(candidate.joinpath(PRODUCTION_MANIFEST_DIR).glob("current_artifacts_*.json")):
            return candidate
    configured = ARTIFACT_ROOT_CONFIG.read_text().strip()
    candidate = Path(configured)
    if not candidate.is_absolute():
        candidate = (ROOT / candidate).resolve()
    if any(candidate.joinpath(PRODUCTION_MANIFEST_DIR).glob("current_artifacts_*.json")):
        return candidate
    return candidate


ARTIFACT_ROOT = resolve_artifact_root()
UI_TARGET_ROOT = resolve_ui_target_root()
WEB_STATIC_ROOT = UI_TARGET_ROOT / "web" / "generated-static"


def latest_current_artifacts(root: Path) -> Path:
    manifests = sorted(root.joinpath(PRODUCTION_MANIFEST_DIR).glob("current_artifacts_*.json"))
    if not manifests:
        raise RuntimeError(f"missing current_artifacts_*.json under {root.joinpath(PRODUCTION_MANIFEST_DIR)}")
    return manifests[-1]

CURRENT_ARTIFACTS_FILE = latest_current_artifacts(ARTIFACT_ROOT)
CURRENT_ARTIFACTS = json.loads(CURRENT_ARTIFACTS_FILE.read_text())
bundle_filename = CURRENT_ARTIFACTS["bundles"][-1]["filename"]
PRODUCT_BUILD_FILE = ARTIFACT_ROOT / PRODUCTION_MANIFEST_DIR / bundle_filename
BUILD_MANIFEST_FILE = PRODUCT_BUILD_FILE.with_name(
    f"build-manifest_{PRODUCT_BUILD_FILE.stem.split('_', 1)[1]}.json",
)


def load_product_build() -> dict:
    return json.loads(PRODUCT_BUILD_FILE.read_text())


PRODUCT_BUILD = load_product_build()


def load_build_manifest() -> dict:
    return json.loads(BUILD_MANIFEST_FILE.read_text())


BUILD_MANIFEST = load_build_manifest()


def resolve_product_build_output(node_name: str, output_name: str) -> Path:
    if isinstance(PRODUCT_BUILD.get(node_name), dict):
        record = PRODUCT_BUILD[node_name]
        raw_path = record.get("relative_path")
        if isinstance(raw_path, str) and raw_path:
            resolved = ARTIFACT_ROOT / raw_path
            if resolved.exists():
                return resolved
            raise RuntimeError(f"missing product build output {node_name}: {resolved}")
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


def resolve_build_manifest_node(node_name: str) -> dict:
    for node in BUILD_MANIFEST.get("nodes", []):
        if isinstance(node, dict) and node.get("name") == node_name:
            return node
    raise RuntimeError(f"missing build manifest node {node_name} in {BUILD_MANIFEST_FILE}")


RESOURCE_INDEX_PATH = resolve_product_build_output("resource_index", "resource_index")
VECTOR_ZIP_PATH = resolve_product_build_output("vectors", "zip")
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
    return json.loads(RESOURCE_INDEX_PATH.read_text())


RESOURCE_INDEX = load_resource_index()


def family_tiles_roots() -> dict[str, Path]:
    roots: dict[str, Path] = {}
    for package in PRODUCT_BUILD.get("packages", []):
        if not isinstance(package, dict):
            continue
        family_id = package.get("family_id")
        relative_path = package.get("relative_path")
        if family_id not in {"sec", "tac", "enr-l", "enr-h"}:
            continue
        if family_id in roots:
            continue
        region = package.get("region_id")
        if not isinstance(region, str) or not region:
            continue
        node = resolve_build_manifest_node(f"charts-{family_id}-package-{region}")
        outputs = node.get("outputs", {})
        manifest_path = outputs.get("manifest") if isinstance(outputs, dict) else None
        if not isinstance(manifest_path, str) or not manifest_path:
            continue
        tiles_root = (ARTIFACT_ROOT / manifest_path).parent / "tiles"
        if tiles_root.is_dir():
            roots[family_id] = tiles_root
    missing = {"sec", "tac", "enr-l", "enr-h"} - set(roots)
    if missing:
        raise RuntimeError(f"missing tiles roots for families {sorted(missing)} in {PRODUCT_BUILD_FILE}")
    return roots


def package_region(package_id: str) -> str:
    return package_id.split("_", 1)[0].lower()


def resolve_bundle_package_dir(family_id: str, region: str) -> Path:
    for package in PRODUCT_BUILD.get("packages", []):
        if not isinstance(package, dict):
            continue
        if package.get("family_id") != family_id or package.get("region_id") != region:
            continue
        package_id = package.get("id")
        if not isinstance(package_id, str) or not package_id:
            break
        node_name = f"{family_id}-{region}-package" if family_id == "tpp" else f"{family_id}-package-{region}"
        if family_id in {"sec", "tac", "enr-l", "enr-h"}:
            node_name = f"charts-{family_id}-package-{region}"
        node = resolve_build_manifest_node(node_name)
        outputs = node.get("outputs", {})
        manifest_path = outputs.get("work_dir") if isinstance(outputs, dict) else None
        if not isinstance(manifest_path, str) or not manifest_path:
            manifest_path = outputs.get("manifest") if isinstance(outputs, dict) else None
        if not isinstance(manifest_path, str) or not manifest_path:
            break
        package_dir = (ARTIFACT_ROOT / manifest_path).parent if manifest_path.endswith(package_id) or manifest_path.endswith(".zip") else (ARTIFACT_ROOT / manifest_path)
        if package_dir.is_dir():
            return package_dir
        raise RuntimeError(f"missing {family_id} package dir {package_dir}")
    raise RuntimeError(f"missing bundle package for family={family_id} region={region} in {PRODUCT_BUILD_FILE}")


def tpp_work_dir(region: str) -> Path:
    return resolve_bundle_package_dir("tpp", region)


def csup_work_dir(region: str) -> Path:
    return resolve_bundle_package_dir("csup", region)


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
    reset_dir(target)
    with zipfile.ZipFile(VECTOR_ZIP_PATH) as bundle:
        bundle.extractall(target)


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
