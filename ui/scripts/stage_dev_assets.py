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
PACKAGED_DIR = Path("published-packaged")
UNPACKED_DIR = Path("published-unpacked")


def resolve_ui_target_root() -> Path:
    env_value = os.environ.get("AEROBAG_UI_TARGET_ROOT")
    if env_value:
        return Path(env_value).expanduser()
    return (ROOT / TARGET_ROOT_FILE.read_text().strip()).resolve()


def resolve_artifact_root() -> Path:
    env_value = os.environ.get("AEROBAG_ARTIFACT_READ_PATH")
    if env_value:
        candidate = Path(env_value).expanduser()
        if not candidate.joinpath(PACKAGED_DIR, "current_artifacts.json").is_file():
            raise RuntimeError(
                f"AEROBAG_ARTIFACT_READ_PATH does not contain current_artifacts.json under "
                f"{candidate.joinpath(PACKAGED_DIR)}"
            )
        return candidate
    configured = ARTIFACT_READ_PATH_CONFIG.read_text().strip()
    candidate = Path(configured)
    if not candidate.is_absolute():
        candidate = (ROOT / candidate).resolve()
    if not candidate.joinpath(PACKAGED_DIR, "current_artifacts.json").is_file():
        raise RuntimeError(
            f"configured artifact root does not contain current_artifacts.json under "
            f"{candidate.joinpath(PACKAGED_DIR)}"
        )
    return candidate


ARTIFACT_ROOT = resolve_artifact_root()
UI_TARGET_ROOT = resolve_ui_target_root()
WEB_STATIC_ROOT = UI_TARGET_ROOT / "web" / "generated-static"
STAGE_STAMP_PATH = WEB_STATIC_ROOT / ".stage-stamp.json"


def latest_current_artifacts(root: Path) -> Path:
    manifest = root.joinpath(PACKAGED_DIR, "current_artifacts.json")
    if not manifest.is_file():
        raise RuntimeError(f"missing current_artifacts.json under {root.joinpath(PACKAGED_DIR)}")
    return manifest

CURRENT_ARTIFACTS_FILE = latest_current_artifacts(ARTIFACT_ROOT)
CURRENT_ARTIFACTS = json.loads(CURRENT_ARTIFACTS_FILE.read_text())
bundle_entries = CURRENT_ARTIFACTS.get("bundles", [])
cycle_bundle_filename = next(
    (
        bundle["filename"]
        for bundle in bundle_entries
        if isinstance(bundle, dict) and bundle.get("bundle_type") == "cycle"
    ),
    bundle_entries[0]["filename"],
)
PRODUCT_BUILD_FILE = ARTIFACT_ROOT / PACKAGED_DIR / cycle_bundle_filename
PACKAGED_ROOT = ARTIFACT_ROOT / PACKAGED_DIR
UNPACKED_ROOT = ARTIFACT_ROOT / UNPACKED_DIR


def load_product_build() -> dict:
    return json.loads(PRODUCT_BUILD_FILE.read_text())

PRODUCT_BUILD = load_product_build()


def load_fast_bundle() -> dict:
    for bundle in CURRENT_ARTIFACTS.get("bundles", []):
        if not isinstance(bundle, dict) or bundle.get("bundle_type") != "fast":
            continue
        filename = bundle.get("filename")
        if not isinstance(filename, str) or not filename:
            continue
        return json.loads((PACKAGED_ROOT / filename).read_text())
    return {}


FAST_BUNDLE = load_fast_bundle()


def bundle_package_filenames_by_id() -> dict[str, str]:
    result: dict[str, str] = {}
    for package in PRODUCT_BUILD.get("packages", []):
        if not isinstance(package, dict):
            continue
        package_id = package.get("id")
        filename = package.get("filename")
        if isinstance(package_id, str) and isinstance(filename, str) and filename:
            result[package_id] = filename
    return result


BUNDLE_PACKAGE_FILENAMES = bundle_package_filenames_by_id()


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
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
            return
        raise


def ensure_symlink(source: Path, target: Path) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists() or target.is_symlink():
        target.unlink()
    target.symlink_to(source)


def unpacked_dir_from_relative_zip(relative_zip_path: str) -> Path:
    relative = Path(relative_zip_path)
    if relative.suffix != ".zip":
        raise RuntimeError(f"expected zip path, got {relative_zip_path}")
    if len(relative.parts) != 1:
        raise RuntimeError(f"expected flat published zip filename, got {relative_zip_path}")
    return UNPACKED_ROOT / relative.with_suffix("")


def packages_by_family() -> dict[str, list[dict]]:
    grouped: dict[str, list[dict]] = {}
    for package in PRODUCT_BUILD.get("packages", []):
        if not isinstance(package, dict):
            continue
        family_id = package.get("family_id")
        filename = package.get("filename")
        if not isinstance(family_id, str) or not isinstance(filename, str) or not filename:
            continue
        grouped.setdefault(family_id, []).append(package)
    return grouped


PACKAGE_ROWS_BY_FAMILY = packages_by_family()


def family_tiles_roots() -> dict[str, Path]:
    roots: dict[str, Path] = {}
    for family_id in ("sec", "tac", "enr-l", "enr-h"):
        for package in PACKAGE_ROWS_BY_FAMILY.get(family_id, []):
            package_id = package.get("id")
            package_filename = package.get("filename")
            if not isinstance(package_id, str) or package_id in roots:
                continue
            if not isinstance(package_filename, str) or not package_filename:
                continue
            tiles_root = unpacked_dir_from_relative_zip(package_filename) / "tiles"
            if tiles_root.is_dir():
                roots[package_id] = tiles_root
    expected = {
        package["id"]
        for family_id in ("sec", "tac", "enr-l", "enr-h")
        for package in PACKAGE_ROWS_BY_FAMILY.get(family_id, [])
        if isinstance(package.get("id"), str)
    }
    missing = expected - set(roots)
    if missing:
        raise RuntimeError(f"missing tiles roots for packages {sorted(missing)} in {UNPACKED_ROOT}")
    return roots


def unpacked_package_dirs_by_id() -> dict[str, Path]:
    directories: dict[str, Path] = {}
    for package_id, package_filename in BUNDLE_PACKAGE_FILENAMES.items():
        directories[package_id] = unpacked_dir_from_relative_zip(package_filename)
    return directories


def stage_sectional_packages() -> None:
    package_root = WEB_STATIC_ROOT / "sectional-packages"
    reset_dir(package_root)
    tiles_by_package = family_tiles_roots()
    for package_id, tiles_root in tiles_by_package.items():
        ensure_symlink(tiles_root, package_root / package_id / "tiles")


def stage_chart_assets() -> None:
    for relative_root in ("plates", "afd", "thumbnails"):
        reset_dir(WEB_STATIC_ROOT / relative_root)
    for family_id in ("tpp", "csup"):
        for package in PACKAGE_ROWS_BY_FAMILY.get(family_id, []):
            package_filename = package.get("filename")
            if not isinstance(package_filename, str) or not package_filename:
                continue
            package_dir = unpacked_dir_from_relative_zip(package_filename)
            if not package_dir.is_dir():
                raise RuntimeError(f"missing unpacked package dir for {package_filename}")
            for relative_root in ("plates", "afd", "thumbnails"):
                source_root = package_dir / relative_root
                if not source_root.is_dir():
                    continue
                for source in sorted(source_root.rglob("*")):
                    if source.is_dir():
                        continue
                    ensure_hard_link(source, WEB_STATIC_ROOT / source.relative_to(package_dir))


def stage_fast_products() -> None:
    target = WEB_STATIC_ROOT / "fast-products"
    reset_dir(target)
    for product in FAST_BUNDLE.get("packages", []):
        if not isinstance(product, dict):
            continue
        product_id = product.get("id")
        filename = product.get("filename")
        if not isinstance(product_id, str) or not isinstance(filename, str):
            continue
        product_root = unpacked_dir_from_relative_zip(filename)
        if not product_root.is_dir():
            print(f"warning: fast product unavailable {product_id}: {product_root}")
            continue
        ensure_symlink(product_root, target / product_id)


def stage_nav_kv() -> None:
    target = WEB_STATIC_ROOT / "nav-kv"
    reset_dir(target)
    nav_db_package = next(
        (
            package
            for package in PRODUCT_BUILD.get("packages", [])
            if isinstance(package, dict) and package.get("family_id") == "nav-db"
        ),
        None,
    )
    if not isinstance(nav_db_package, dict):
        raise RuntimeError("cycle bundle missing nav-db package")
    nav_db_filename = nav_db_package.get("filename")
    if not isinstance(nav_db_filename, str) or not nav_db_filename:
        raise RuntimeError("nav-db package missing filename")
    nav_db_root = unpacked_dir_from_relative_zip(nav_db_filename)
    root_candidates = [nav_db_root / "root"]
    pages = sorted(nav_db_root.glob("values_*"))
    if len(root_candidates) != 1 or not root_candidates[0].is_file():
        raise RuntimeError(f"expected one nav-kv root under {nav_db_root}, found {len(root_candidates)}")
    ensure_hard_link(root_candidates[0], target / "root")
    values_root = target / "values"
    values_root.mkdir(parents=True, exist_ok=True)
    for index, page in enumerate(pages):
        if not page.is_file():
            raise RuntimeError(f"missing nav-kv page {page}")
        ensure_hard_link(page, values_root / f"{index:04}")


def stage_bundle_manifests() -> None:
    ensure_hard_link(CURRENT_ARTIFACTS_FILE, WEB_STATIC_ROOT / "current-artifacts.json")
    ensure_hard_link(PRODUCT_BUILD_FILE, WEB_STATIC_ROOT / "cycle-bundle.json")
    build_status = ARTIFACT_ROOT / PACKAGED_DIR / "build-status.html"
    if build_status.is_file():
        ensure_hard_link(build_status, WEB_STATIC_ROOT / "build-status.html")


def stage_shaded_relief_products() -> None:
    """Symlink each shaded-relief-* package's unpacked dir into the
    staging tree under shaded-relief-products/<id>/. Previously done by
    vite's writeBundle hook; moved here so nginx can serve the staged
    tree directly without vite linking copies into dist/."""
    target = WEB_STATIC_ROOT / "shaded-relief-products"
    reset_dir(target)
    for package in PRODUCT_BUILD.get("packages", []):
        if not isinstance(package, dict):
            continue
        package_id = package.get("id")
        if not isinstance(package_id, str) or not package_id.startswith("shaded-relief-"):
            continue
        package_filename = package.get("filename")
        if not isinstance(package_filename, str) or not package_filename:
            continue
        product_root = unpacked_dir_from_relative_zip(package_filename)
        if not product_root.is_dir():
            print(f"warning: shaded-relief product unavailable {package_id}: {product_root}")
            continue
        ensure_symlink(product_root, target / package_id)


def stage_icons() -> None:
    """Mirror ui/icons/ into the staging dir, preserving subdir structure.

    Web code references icons as /icons/icons/<file>.png (one level for the
    mount, one for the inner subdir of the source tree), so we mirror the
    whole ui/icons/ directory rather than flattening its contents.
    """
    target = WEB_STATIC_ROOT / "icons"
    reset_dir(target)
    source_root = ROOT / "ui" / "icons"
    if not source_root.is_dir():
        raise RuntimeError(f"missing icons source dir {source_root}")
    for source in sorted(source_root.rglob("*")):
        if source.is_file():
            ensure_hard_link(source, target / source.relative_to(source_root))


def current_stage_stamp() -> dict:
    def file_stamp(path: Path) -> dict:
        stat = path.stat()
        return {
            "path": str(path),
            "size": stat.st_size,
            "mtime_ns": stat.st_mtime_ns,
        }

    return {
        "current_artifacts": file_stamp(CURRENT_ARTIFACTS_FILE),
        "bundle_manifest": file_stamp(PRODUCT_BUILD_FILE),
        "fast_products": [
            {
                "id": product.get("id"),
                "filename": product.get("filename"),
                "checksum_sha256": product.get("checksum_sha256"),
            }
            for product in FAST_BUNDLE.get("packages", [])
            if isinstance(product, dict)
        ],
        "packages": [
            {
                "id": package.get("id"),
                "filename": package.get("filename"),
                "checksum_sha256": package.get("checksum_sha256"),
            }
            for package in PRODUCT_BUILD.get("packages", [])
            if isinstance(package, dict)
        ],
        "version": 10,
    }


def stage_is_current() -> bool:
    if not STAGE_STAMP_PATH.is_file():
        return False
    try:
        existing = json.loads(STAGE_STAMP_PATH.read_text())
    except Exception:
        return False
    return existing == current_stage_stamp() and staged_outputs_exist()


def staged_outputs_exist() -> bool:
    required_paths = [
        WEB_STATIC_ROOT / "current-artifacts.json",
        WEB_STATIC_ROOT / "cycle-bundle.json",
        WEB_STATIC_ROOT / "vectors" / "vectors",
        WEB_STATIC_ROOT / "vectors" / "points",
        WEB_STATIC_ROOT / "vectors" / "airspace",
        WEB_STATIC_ROOT / "vectors" / "had",
        WEB_STATIC_ROOT / "nav-kv" / "root",
    ]
    return all(path.exists() for path in required_paths)


def write_stage_stamp() -> None:
    STAGE_STAMP_PATH.write_text(json.dumps(current_stage_stamp(), indent=2, sort_keys=True) + "\n")


def main() -> None:
    WEB_STATIC_ROOT.mkdir(parents=True, exist_ok=True)
    # Icons are not part of the artifact bundle stamp, so stage them before
    # the cache check. Cheap (handful of hard links).
    stage_icons()
    if stage_is_current():
        return
    stage_bundle_manifests()
    stage_sectional_packages()
    stage_chart_assets()
    stage_fast_products()
    stage_nav_kv()
    stage_shaded_relief_products()
    write_stage_stamp()


if __name__ == "__main__":
    main()
