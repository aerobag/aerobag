#!/usr/bin/env python3

import json
import os
import pathlib
import shutil
import time
from dataclasses import dataclass, field


DEST = pathlib.Path("/root/aerobag-artifacts-snapshot")
BACKUP_DEST = pathlib.Path("/root/aerobag-artifacts-snapshot.bak")
SOURCE_ROOT = pathlib.Path("/root/aerobag-artifacts")

PACKAGED_ROOT_NAME = "published_packaged"
UNPACKED_ROOT_NAME = "published_unpacked"


@dataclass
class SnapshotPlan:
    root_files: set[pathlib.Path] = field(default_factory=set)
    packaged_files: set[pathlib.Path] = field(default_factory=set)
    unpacked_files: set[pathlib.Path] = field(default_factory=set)
    unpacked_dirs: set[pathlib.Path] = field(default_factory=set)


def load_json(path: pathlib.Path) -> dict:
    return json.loads(path.read_text())


def discovery_manifest_candidates(source_root: pathlib.Path) -> list[pathlib.Path]:
    candidates = [source_root / "current_artifacts.json"]
    candidates.extend(sorted(source_root.glob("current_artifacts_*T*.json")))
    seen = set()
    deduped = []
    for candidate in candidates:
        if candidate in seen or not candidate.is_file():
            continue
        seen.add(candidate)
        deduped.append(candidate)
    return deduped


def zip_stem(filename: str) -> str:
    if not filename.endswith(".zip"):
        raise ValueError(f"expected zip filename: {filename}")
    return filename[:-4]


def validate_roots(current: dict, discovery_path: pathlib.Path) -> None:
    roots = current.get("artifact_roots")
    expected = {
        "packaged": f"{PACKAGED_ROOT_NAME}/",
        "unpacked": f"{UNPACKED_ROOT_NAME}/",
    }
    if roots != expected:
        raise ValueError(
            f"{discovery_path} has artifact_roots={roots!r}; expected {expected!r}"
        )


def add_packaged_file(plan: SnapshotPlan, source_root: pathlib.Path, relative_path: str) -> None:
    path = source_root / PACKAGED_ROOT_NAME / relative_path
    if not path.is_file():
        raise FileNotFoundError(f"missing packaged artifact {path}")
    plan.packaged_files.add(path)

    unpacked_root = source_root / UNPACKED_ROOT_NAME
    if relative_path.endswith(".zip"):
        unpacked_path = unpacked_root / zip_stem(relative_path)
        if not unpacked_path.is_dir():
            raise FileNotFoundError(f"missing unpacked package dir {unpacked_path}")
        plan.unpacked_dirs.add(unpacked_path)
    else:
        unpacked_path = unpacked_root / relative_path
        if not unpacked_path.is_file():
            raise FileNotFoundError(f"missing unpacked artifact {unpacked_path}")
        plan.unpacked_files.add(unpacked_path)


def add_bundle_contents(
    plan: SnapshotPlan,
    source_root: pathlib.Path,
    bundle_filename: str,
) -> None:
    add_packaged_file(plan, source_root, bundle_filename)
    bundle = load_json(source_root / PACKAGED_ROOT_NAME / bundle_filename)

    for artifact in bundle.get("packages", []):
        add_packaged_file(plan, source_root, artifact["relative_path"])
    for artifact in bundle.get("ancillary", []):
        add_packaged_file(plan, source_root, artifact["relative_path"])


def try_add_discovery(
    plan: SnapshotPlan,
    source_root: pathlib.Path,
    discovery_path: pathlib.Path,
    *,
    required: bool,
) -> bool:
    try:
        current = load_json(discovery_path)
        validate_roots(current, discovery_path)
        for bundle in current.get("bundles", []):
            add_bundle_contents(plan, source_root, bundle["filename"])
        diagnostics = current.get("diagnostics")
        if isinstance(diagnostics, dict):
            filename = diagnostics.get("filename")
            if isinstance(filename, str) and filename:
                add_packaged_file(plan, source_root, filename)
    except Exception as error:
        if required:
            raise
        print(
            f"WARNING skipping stale historical discovery {discovery_path.name}: {error}"
        )
        return False

    plan.root_files.add(discovery_path)
    return True


def build_snapshot_plan(source_root: pathlib.Path) -> SnapshotPlan:
    plan = SnapshotPlan()
    discoveries = discovery_manifest_candidates(source_root)
    if not discoveries:
        raise FileNotFoundError(f"missing {source_root / 'current_artifacts.json'}")

    for discovery_path in discoveries:
        try_add_discovery(
            plan,
            source_root,
            discovery_path,
            required=discovery_path.name == "current_artifacts.json",
        )
    return plan


def hardlink_file(src: pathlib.Path, dst: pathlib.Path) -> None:
    dst.parent.mkdir(parents=True, exist_ok=True)
    os.link(src, dst)


def hardlink_tree(src: pathlib.Path, dst: pathlib.Path) -> None:
    for root, dirs, files in os.walk(src):
        root_path = pathlib.Path(root)
        relative_root = root_path.relative_to(src)
        dest_root = dst / relative_root
        dest_root.mkdir(parents=True, exist_ok=True)
        dirs.sort()
        for filename in sorted(files):
            hardlink_file(root_path / filename, dest_root / filename)


def materialize_snapshot(source_root: pathlib.Path, dest_root: pathlib.Path, plan: SnapshotPlan) -> None:
    start = time.monotonic()
    for root_file in sorted(plan.root_files):
        hardlink_file(root_file, dest_root / root_file.relative_to(source_root))
    for packaged_file in sorted(plan.packaged_files):
        hardlink_file(packaged_file, dest_root / packaged_file.relative_to(source_root))
    for unpacked_file in sorted(plan.unpacked_files):
        hardlink_file(unpacked_file, dest_root / unpacked_file.relative_to(source_root))
    for unpacked_dir in sorted(plan.unpacked_dirs):
        hardlink_tree(unpacked_dir, dest_root / unpacked_dir.relative_to(source_root))
    elapsed = time.monotonic() - start
    print(
        "linked "
        f"{len(plan.root_files)} root manifests, "
        f"{len(plan.packaged_files)} packaged files, "
        f"{len(plan.unpacked_files)} unpacked files, "
        f"{len(plan.unpacked_dirs)} unpacked package dirs "
        f"in {elapsed:.3f}s"
    )


def main() -> int:
    plan = build_snapshot_plan(SOURCE_ROOT)
    if BACKUP_DEST.exists():
        shutil.rmtree(BACKUP_DEST)
    if DEST.exists():
        print(f"Backing up {DEST} -> {BACKUP_DEST}")
        shutil.move(DEST, BACKUP_DEST)
    print(f"Snapshotting contract from {SOURCE_ROOT} -> {DEST}")
    materialize_snapshot(SOURCE_ROOT, DEST, plan)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
