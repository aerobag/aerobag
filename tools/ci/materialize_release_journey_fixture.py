#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import shutil
import tempfile
import zipfile
from pathlib import Path

from build_e2e_package_fixture import BuildError, read_json, safe_member_path, write_json


def refresh_live_feed_timestamps(root: Path) -> None:
    now = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    for variant in ("fresh", "mixed"):
        current_path = root / "live-feeds" / variant / "current.json"
        if not current_path.is_file():
            continue
        current = read_json(current_path)
        current["generated_at_utc"] = now
        for product in current.get("products", {}).values():
            for field in ("collected_at_utc", "observed_at_utc", "published_at_utc"):
                value = product.get(field)
                if isinstance(value, str) and not value.startswith("2020-"):
                    product[field] = now
        write_json(current_path, current)


def extract_packages(publication: Path) -> None:
    current_values = read_json(publication / "current_artifacts.json")
    if not isinstance(current_values, list) or not current_values:
        raise BuildError(f"{publication}: current_artifacts.json is not a non-empty list")
    roots = current_values[-1].get("artifact_roots", {})
    packaged_relative = roots.get("packaged")
    unpacked_relative = roots.get("unpacked")
    if not isinstance(packaged_relative, str) or not isinstance(unpacked_relative, str):
        raise BuildError(f"{publication}: artifact roots are incomplete")
    packaged = publication.joinpath(*safe_member_path(packaged_relative).parts)
    unpacked = publication.joinpath(*safe_member_path(unpacked_relative).parts)
    unpacked.mkdir(parents=True, exist_ok=True)
    for package in sorted(packaged.glob("*.zip")):
        destination = unpacked / package.name.removesuffix(".zip")
        if destination.exists():
            continue
        with zipfile.ZipFile(package) as archive:
            for member in archive.infolist():
                if member.is_dir():
                    continue
                relative = safe_member_path(member.filename)
                output = destination.joinpath(*relative.parts)
                output.parent.mkdir(parents=True, exist_ok=True)
                with archive.open(member) as source, output.open("wb") as target:
                    shutil.copyfileobj(source, target)


def materialize(source: Path, output: Path) -> None:
    if output.exists():
        raise BuildError(f"output already exists: {output}")
    fixture = read_json(source / "fixture.json")
    publication_root = fixture.get("publication_root")
    if not isinstance(publication_root, str):
        raise BuildError("release fixture has no publication_root")
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(tempfile.mkdtemp(prefix=f".{output.name}.", dir=output.parent))
    try:
        shutil.copytree(source, temporary, dirs_exist_ok=True)
        extract_packages(temporary / publication_root)
        refresh_live_feed_timestamps(temporary)
        temporary.rename(output)
    except BaseException:
        shutil.rmtree(temporary, ignore_errors=True)
        raise


def main() -> int:
    parser = argparse.ArgumentParser(description="Expand packaged release-journey resources for static serving.")
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        materialize(args.source.resolve(), args.output.resolve())
    except BuildError as error:
        print(f"error: {error}")
        return 1
    print(args.output.resolve())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
