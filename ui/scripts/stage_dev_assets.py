#!/usr/bin/env python3
from __future__ import annotations

import errno
import json
import os
import shutil
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TARGET_ROOT_FILE = ROOT / "ui" / "target-root.txt"
ARTIFACT_READ_PATH_CONFIG = ROOT / ".aerobag-artifact-read-path"
PRODUCTION_MANIFEST_DIR = Path("product-builds") / "production"


def resolve_ui_target_root() -> Path:
    env_value = os.environ.get("AEROBAG_UI_TARGET_ROOT")
    if env_value:
        return Path(env_value).expanduser()
    return (ROOT / TARGET_ROOT_FILE.read_text().strip()).resolve()


def resolve_artifact_root() -> Path:
    env_value = os.environ.get("AEROBAG_ARTIFACT_READ_PATH")
    if env_value:
        candidate = Path(env_value).expanduser()
        if not any(candidate.joinpath(PRODUCTION_MANIFEST_DIR).glob("current_artifacts_*.json")):
            raise RuntimeError(
                f"AEROBAG_ARTIFACT_READ_PATH does not contain current_artifacts_*.json under "
                f"{candidate.joinpath(PRODUCTION_MANIFEST_DIR)}"
            )
        return candidate
    configured = ARTIFACT_READ_PATH_CONFIG.read_text().strip()
    candidate = Path(configured)
    if not candidate.is_absolute():
        candidate = (ROOT / candidate).resolve()
    if not any(candidate.joinpath(PRODUCTION_MANIFEST_DIR).glob("current_artifacts_*.json")):
        raise RuntimeError(
            f"configured artifact root does not contain current_artifacts_*.json under "
            f"{candidate.joinpath(PRODUCTION_MANIFEST_DIR)}"
        )
    return candidate


ARTIFACT_ROOT = resolve_artifact_root()
UI_TARGET_ROOT = resolve_ui_target_root()
WEB_STATIC_ROOT = UI_TARGET_ROOT / "web" / "generated-static"
STAGE_STAMP_PATH = WEB_STATIC_ROOT / ".stage-stamp.json"


def latest_current_artifacts(root: Path) -> Path:
    manifests = sorted(root.joinpath(PRODUCTION_MANIFEST_DIR).glob("current_artifacts_*.json"))
    if not manifests:
        raise RuntimeError(f"missing current_artifacts_*.json under {root.joinpath(PRODUCTION_MANIFEST_DIR)}")
    return manifests[-1]

CURRENT_ARTIFACTS_FILE = latest_current_artifacts(ARTIFACT_ROOT)
CURRENT_ARTIFACTS = json.loads(CURRENT_ARTIFACTS_FILE.read_text())
bundle_filename = CURRENT_ARTIFACTS["bundles"][-1]["filename"]
PRODUCT_BUILD_FILE = ARTIFACT_ROOT / PRODUCTION_MANIFEST_DIR / bundle_filename
CYCLE = PRODUCT_BUILD_FILE.stem.split("_", 1)[1]
PUBLISHED_ROOT = ARTIFACT_ROOT / "published-unpacked" / "production" / CYCLE


def load_product_build() -> dict:
    return json.loads(PRODUCT_BUILD_FILE.read_text())


PRODUCT_BUILD = load_product_build()


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
    if target.exists() or target.is_symlink():
        try:
            if source.stat().st_ino == target.stat().st_ino and source.stat().st_dev == target.stat().st_dev:
                return
        except FileNotFoundError:
            pass
        target.unlink()
    try:
        os.link(source, target)
    except OSError as exc:
        if exc.errno == errno.EEXIST:
            try:
                if source.stat().st_ino == target.stat().st_ino and source.stat().st_dev == target.stat().st_dev:
                    return
            except FileNotFoundError:
                pass
            target.unlink(missing_ok=True)
            os.link(source, target)
            return
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


def published_path_from_relative(relative_path: str) -> Path:
    return PUBLISHED_ROOT / "product-builds" / relative_path


def unpacked_dir_from_relative_zip(relative_zip_path: str) -> Path:
    relative = Path(relative_zip_path)
    if relative.suffix != ".zip":
        raise RuntimeError(f"expected zip path, got {relative_zip_path}")
    return published_path_from_relative(str(relative.with_suffix("")))


def family_tiles_roots() -> dict[str, Path]:
    roots: dict[str, Path] = {}
    for package in RESOURCE_INDEX.get("packages", []):
        if not isinstance(package, dict):
            continue
        family_id = package.get("family_id")
        if family_id not in {"sec", "tac", "enr-l", "enr-h"}:
            continue
        package_id = package.get("id")
        if not isinstance(package_id, str) or package_id in roots:
            continue
        artifact_path = package.get("artifact_path")
        if not isinstance(artifact_path, str) or not artifact_path:
            continue
        tiles_root = unpacked_dir_from_relative_zip(artifact_path) / "tiles"
        if tiles_root.is_dir():
            roots[package_id] = tiles_root
    expected = {
        package["id"]
        for package in RESOURCE_INDEX.get("packages", [])
        if isinstance(package, dict) and package.get("family_id") in {"sec", "tac", "enr-l", "enr-h"}
    }
    missing = expected - set(roots)
    if missing:
        raise RuntimeError(f"missing tiles roots for packages {sorted(missing)} in {PUBLISHED_ROOT}")
    return roots


def unpacked_package_dirs_by_id() -> dict[str, Path]:
    directories: dict[str, Path] = {}
    for package in RESOURCE_INDEX.get("packages", []):
        package_id = package.get("id")
        artifact_path = package.get("artifact_path") or package.get("relative_path")
        if isinstance(package_id, str) and isinstance(artifact_path, str):
            directories[package_id] = unpacked_dir_from_relative_zip(artifact_path)
    return directories


def stage_sectional_packages() -> None:
    package_root = WEB_STATIC_ROOT / "sectional-packages"
    reset_dir(package_root)
    tiles_by_package = family_tiles_roots()
    for package in RESOURCE_INDEX["packages"]:
        package_id = package["id"]
        if package_id not in tiles_by_package:
            continue
        ensure_symlink(tiles_by_package[package_id], package_root / package_id / "tiles")


def stage_chart_assets() -> None:
    for relative_root in ("plates", "afd", "thumbnails"):
        reset_dir(WEB_STATIC_ROOT / relative_root)
    package_dirs = unpacked_package_dirs_by_id()
    for kind, records in (("plate", RESOURCE_INDEX.get("plates", [])), ("csup", RESOURCE_INDEX.get("csups", []))):
        for record in records:
            package_dir = package_dirs.get(record["package_id"])
            if package_dir is None or not package_dir.is_dir():
                raise RuntimeError(f"missing unpacked package dir for {record['package_id']}")
            asset_source = package_dir / record["asset_path"]
            if not asset_source.is_file():
                raise RuntimeError(f"missing staged asset {asset_source}")
            ensure_hard_link(asset_source, WEB_STATIC_ROOT / record["asset_path"])
            thumbnail_path = record.get("thumbnail_path")
            if thumbnail_path:
                thumbnail_source = package_dir / thumbnail_path
                if not thumbnail_source.is_file():
                    raise RuntimeError(f"missing staged thumbnail {thumbnail_source}")
                ensure_hard_link(thumbnail_source, WEB_STATIC_ROOT / thumbnail_path)


def stage_vectors() -> None:
    target = WEB_STATIC_ROOT / "vectors"
    reset_dir(target)
    vectors_relative_zip = str(VECTOR_ZIP_PATH.relative_to(ARTIFACT_ROOT / "product-builds"))
    vectors_root = unpacked_dir_from_relative_zip(vectors_relative_zip) / "points"
    if not vectors_root.is_dir():
        raise RuntimeError(f"missing published vector points dir {vectors_root}")
    ensure_symlink(vectors_root, target / "points")


def stage_nav_db() -> None:
    nav_root = WEB_STATIC_ROOT / "nav-db"
    reset_dir(nav_root)
    ensure_hard_link(NAV_DB_PATH, nav_root / "main.db")


def current_stage_stamp() -> dict:
    def file_stamp(path: Path) -> dict:
        stat = path.stat()
        return {
            "path": str(path),
            "size": stat.st_size,
            "mtime_ns": stat.st_mtime_ns,
        }

    return {
        "resource_index": file_stamp(RESOURCE_INDEX_PATH),
        "vectors_zip": file_stamp(VECTOR_ZIP_PATH),
        "nav_db": file_stamp(NAV_DB_PATH),
        "current_artifacts": file_stamp(CURRENT_ARTIFACTS_FILE),
        "bundle_manifest": file_stamp(PRODUCT_BUILD_FILE),
        "version": 2,
    }


def stage_is_current() -> bool:
    if not STAGE_STAMP_PATH.is_file():
        return False
    try:
        existing = json.loads(STAGE_STAMP_PATH.read_text())
    except Exception:
        return False
    return existing == current_stage_stamp()


def write_stage_stamp() -> None:
    STAGE_STAMP_PATH.write_text(json.dumps(current_stage_stamp(), indent=2, sort_keys=True) + "\n")


def main() -> None:
    WEB_STATIC_ROOT.mkdir(parents=True, exist_ok=True)
    if stage_is_current():
        return
    stage_sectional_packages()
    stage_chart_assets()
    stage_vectors()
    stage_nav_db()
    write_stage_stamp()


if __name__ == "__main__":
    main()
