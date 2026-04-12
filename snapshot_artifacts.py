#!/usr/bin/env python3

import json
import os
import pathlib
import shutil
import time


DEST = pathlib.Path("/root/aerobag-artifacts-snapshot")
SOURCE_ROOT = pathlib.Path("/root/aerobag-artifacts")


def timed_copytree(src: pathlib.Path, dst: pathlib.Path) -> None:
    start = time.monotonic()
    shutil.copytree(src, dst, copy_function=os.link)
    elapsed = time.monotonic() - start
    print(f"copied {src} -> {dst} in {elapsed:.3f}s")


def load_current_artifacts(production_root: pathlib.Path) -> tuple[pathlib.Path, dict]:
    current_artifacts = max(production_root.glob("current_artifacts_*.json"))
    return current_artifacts, json.loads(current_artifacts.read_text())


def collect_packed_artifacts(source_root: pathlib.Path) -> set[pathlib.Path]:
    production_root = source_root / "published-packaged" / "production"
    current_artifacts_path, current = load_current_artifacts(production_root)
    files_to_copy: set[pathlib.Path] = {current_artifacts_path}

    for bundle_entry in current["bundles"]:
        bundle_path = production_root / bundle_entry["filename"]
        files_to_copy.add(bundle_path)
        build_manifest_path = production_root / bundle_entry["filename"].replace(
            "bundle_", "build-manifest_"
        )
        if build_manifest_path.exists():
            files_to_copy.add(build_manifest_path)

        bundle = json.loads(bundle_path.read_text())
        files_to_copy.add(source_root / bundle["catalog"]["relative_path"])
        files_to_copy.add(source_root / bundle["resource_index"]["relative_path"])
        files_to_copy.add(source_root / bundle["data"]["relative_path"])
        files_to_copy.add(source_root / bundle["vectors"]["relative_path"])
        for package in bundle["packages"]:
            files_to_copy.add(source_root / package["relative_path"])

    obstacle_path = production_root / current["obstacles"]["filename"]
    if obstacle_path.exists():
        files_to_copy.add(obstacle_path)

    return files_to_copy


def link_packed_artifacts(source_root: pathlib.Path, dest_root: pathlib.Path) -> None:
    for source_path in sorted(collect_packed_artifacts(source_root)):
        relative = source_path.relative_to(source_root)
        dest_path = dest_root / relative
        dest_path.parent.mkdir(parents=True, exist_ok=True)
        if dest_path.exists():
            continue
        os.link(source_path, dest_path)


def main() -> int:
    if DEST.exists():
        shutil.rmtree(DEST)
    (DEST / "published-packaged").mkdir(parents=True, exist_ok=True)

    timed_copytree(
        SOURCE_ROOT / "published-packaged" / "production",
        DEST / "published-packaged" / "production",
    )
    shutil.rmtree(DEST / "published-packaged" / "production" / "work", ignore_errors=True)
    link_packed_artifacts(SOURCE_ROOT, DEST)
    timed_copytree(SOURCE_ROOT / "published-unpacked", DEST / "published-unpacked")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
