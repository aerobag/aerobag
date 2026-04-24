#!/usr/bin/env python3

import json
import os
import pathlib
import shutil
import time


DEST = pathlib.Path("/root/aerobag-artifacts-snapshot")
BACKUP_DEST = pathlib.Path("/root/aerobag-artifacts-snapshot.bak")
SOURCE_ROOT = pathlib.Path("/root/aerobag-artifacts")


def timed_copytree(src: pathlib.Path, dst: pathlib.Path) -> None:
    start = time.monotonic()
    shutil.copytree(src, dst, copy_function=os.link)
    elapsed = time.monotonic() - start
    print(f"copied {src} -> {dst} in {elapsed:.3f}s")


def load_current_artifacts(packaged_root: pathlib.Path) -> tuple[pathlib.Path, bytes, dict]:
    current_artifacts = packaged_root / "current_artifacts.json"
    if not current_artifacts.is_file():
        raise FileNotFoundError(f"missing {current_artifacts}")
    raw = current_artifacts.read_bytes()
    return current_artifacts, raw, json.loads(raw)

def discovery_manifests(packaged_root: pathlib.Path) -> list[pathlib.Path]:
    manifests = []
    latest = packaged_root / "current_artifacts.json"
    if latest.is_file():
        manifests.append(latest)
    manifests.extend(sorted(packaged_root.glob("current_artifacts_*T*.json")))
    deduped = []
    seen = set()
    for manifest in manifests:
        if manifest not in seen:
            seen.add(manifest)
            deduped.append(manifest)
    return deduped


def collect_packed_artifacts(source_root: pathlib.Path) -> tuple[pathlib.Path, bytes, list[pathlib.Path], set[pathlib.Path]]:
    packaged_root = source_root / "published-packaged"
    current_artifacts_path, current_artifacts_raw, current = load_current_artifacts(packaged_root)
    files_to_copy: set[pathlib.Path] = set()
    discovery_paths = discovery_manifests(packaged_root)

    def add_required_packed(filename: str, label: str) -> None:
        artifact_path = packaged_root / filename
        if not artifact_path.is_file():
            raise FileNotFoundError(f"current artifacts references missing {label}: {artifact_path}")
        files_to_copy.add(artifact_path)

    def add_required_bundle_artifact(artifact: dict, label: str) -> None:
        relative_path = artifact["relative_path"]
        artifact_path = packaged_root / relative_path
        if not artifact_path.is_file():
            raise FileNotFoundError(f"bundle references missing {label}: {artifact_path}")
        files_to_copy.add(artifact_path)

    for discovery_path in discovery_paths:
        current = json.loads(discovery_path.read_bytes())
        for bundle_entry in current["bundles"]:
            bundle_path = packaged_root / bundle_entry["filename"]
            if not bundle_path.is_file():
                raise FileNotFoundError(f"current artifacts references missing bundle: {bundle_path}")
            files_to_copy.add(bundle_path)

            bundle = json.loads(bundle_path.read_text())
            for artifact in bundle.get("ancillary", []):
                add_required_bundle_artifact(artifact, f"ancillary artifact {artifact.get('filename', '(unknown)')}")
            for package in bundle.get("packages", []):
                add_required_bundle_artifact(package, f"package {package.get('id', '(unknown)')}")

        diagnostics = current.get("diagnostics")
        if isinstance(diagnostics, dict):
            filename = diagnostics.get("filename")
            if isinstance(filename, str) and filename:
                add_required_packed(filename, "diagnostics artifact")

    return current_artifacts_path, current_artifacts_raw, discovery_paths, files_to_copy


def link_packed_artifacts(source_root: pathlib.Path, dest_root: pathlib.Path) -> None:
    current_artifacts_path, current_artifacts_raw, discovery_paths, source_paths = collect_packed_artifacts(source_root)
    current_dest_path = dest_root / current_artifacts_path.relative_to(source_root)
    current_dest_path.parent.mkdir(parents=True, exist_ok=True)
    current_dest_path.write_bytes(current_artifacts_raw)
    for discovery_path in discovery_paths:
        dest_path = dest_root / discovery_path.relative_to(source_root)
        dest_path.parent.mkdir(parents=True, exist_ok=True)
        if discovery_path == current_artifacts_path:
            continue
        dest_path.write_bytes(discovery_path.read_bytes())

    for source_path in sorted(source_paths):
        relative = source_path.relative_to(source_root)
        dest_path = dest_root / relative
        dest_path.parent.mkdir(parents=True, exist_ok=True)
        if dest_path.exists():
            continue
        os.link(source_path, dest_path)


def main() -> int:
    if BACKUP_DEST.exists():
        shutil.rmtree(BACKUP_DEST)
    if DEST.exists():
        print(f"Backing up {DEST} -> {BACKUP_DEST}")
        shutil.move(DEST, BACKUP_DEST)
    print(f"Snapshotting {SOURCE_ROOT} -> {DEST}")
    (DEST / "published-packaged").mkdir(parents=True, exist_ok=True)
    link_packed_artifacts(SOURCE_ROOT, DEST)
    timed_copytree(SOURCE_ROOT / "published-unpacked", DEST / "published-unpacked")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
