#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath
from typing import Any


FIXTURE_SCHEMA_VERSION = 1
FIXTURE_ID = "android-smoke-publication"
PUBLICATION_ROOT = "e2e-v1"
TPP_AIRPORT_ID = "PLU"
TPP_PLATE_LABEL_CONTAINS = "RNAV 35"
START_VALID = "2020-01-01"
END_VALID = "2100-01-01"
HASH_SUFFIX_RE = re.compile(r"_[0-9a-f]{64}\.zip$")


class BuildError(ValueError):
    pass


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise BuildError(f"cannot read JSON {path}: {error}") from error


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while block := source.read(1024 * 1024):
            digest.update(block)
    return digest.hexdigest()


def safe_member_path(name: str) -> PurePosixPath:
    path = PurePosixPath(name)
    if path.is_absolute() or ".." in path.parts or "." in path.parts:
        raise BuildError(f"unsafe ZIP member path {name!r}")
    return path


def extract_zip(source: Path, destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(source) as archive:
        for member in archive.infolist():
            path = safe_member_path(member.filename)
            if member.is_dir():
                continue
            target = destination.joinpath(*path.parts)
            target.parent.mkdir(parents=True, exist_ok=True)
            with archive.open(member) as input_file, target.open("wb") as output_file:
                shutil.copyfileobj(input_file, output_file)


def deterministic_zip_info(name: str) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
    info.compress_type = zipfile.ZIP_STORED
    info.create_system = 3
    info.external_attr = 0o100644 << 16
    return info


def compact_tpp_package(source: Path, destination: Path) -> int:
    with zipfile.ZipFile(source) as archive:
        names = archive.namelist()
        asset_manifest_name = "package-assets.json"
        tpp_manifest_names = [name for name in names if name.endswith(".manifest")]
        if asset_manifest_name not in names or len(tpp_manifest_names) != 1:
            raise BuildError(
                f"TPP package must contain {asset_manifest_name} and one .manifest"
            )
        prefix = f"plates/{TPP_AIRPORT_ID}/"
        thumbnail_prefix = f"thumbnails/{prefix}"
        asset_names = [
            name
            for name in names
            if name.startswith(prefix) or name.startswith(thumbnail_prefix)
        ]
        if not asset_names:
            raise BuildError(f"TPP package contains no {TPP_AIRPORT_ID} assets")
        package_assets = json.loads(archive.read(asset_manifest_name))
        assets = [
            asset
            for asset in package_assets.get("assets", [])
            if isinstance(asset, dict)
            and isinstance(asset.get("asset_path"), str)
            and asset["asset_path"].startswith(prefix)
        ]
        if not assets:
            raise BuildError(f"TPP package asset manifest contains no {TPP_AIRPORT_ID}")
        package_assets["assets"] = assets
        original_manifest = archive.read(tpp_manifest_names[0]).decode("utf-8")
        version = original_manifest.splitlines()[0]
        tpp_manifest = version + "\n" + "\n".join(asset_names) + "\n"
        payloads = {
            name: archive.read(name)
            for name in asset_names
        }
        payloads[asset_manifest_name] = (
            json.dumps(package_assets, indent=2) + "\n"
        ).encode("utf-8")
        payloads[tpp_manifest_names[0]] = tpp_manifest.encode("utf-8")

    destination.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(destination, "w", allowZip64=True) as output:
        for name, payload in payloads.items():
            safe_member_path(name)
            output.writestr(deterministic_zip_info(name), payload)
    return len(assets)


def package_path(packaged_root: Path, package: dict[str, Any]) -> Path:
    relative_path = package.get("relative_path") or package.get("filename")
    if not isinstance(relative_path, str):
        raise BuildError(f"package {package.get('id')} has no relative path")
    path = safe_member_path(relative_path)
    result = packaged_root.joinpath(*path.parts)
    if not result.is_file():
        raise BuildError(f"package file is missing: {result}")
    return result


def compact_package_filename(source_name: str, digest: str) -> str:
    prefix = HASH_SUFFIX_RE.sub("", source_name)
    if prefix == source_name:
        prefix = source_name.removesuffix(".zip")
    return f"{prefix}_e2e_{digest}.zip"


def update_package_file(
    package: dict[str, Any],
    filename: str,
    path: Path,
) -> dict[str, Any]:
    updated = dict(package)
    updated.update(
        {
            "filename": filename,
            "relative_path": filename,
            "checksum_sha256": sha256(path),
            "size_bytes": path.stat().st_size,
            "effective_date": START_VALID,
            "expiration_date": END_VALID,
        }
    )
    return updated


def build_fixture(source_publication: Path, output_root: Path, cycle: str) -> None:
    if output_root.exists():
        raise BuildError(f"output already exists: {output_root}")
    current_path = source_publication / "current_artifacts.json"
    current_values = read_json(current_path)
    if not isinstance(current_values, list) or not current_values:
        raise BuildError("source current_artifacts.json must be a non-empty list")
    current = current_values[-1]
    packaged_relative = current.get("artifact_roots", {}).get("packaged")
    if not isinstance(packaged_relative, str):
        raise BuildError("source current artifacts has no packaged root")
    packaged_root = source_publication.joinpath(
        *safe_member_path(packaged_relative).parts
    )
    bundle_ref = next(
        (
            value
            for value in current.get("bundles", [])
            if value.get("bundle_type") == "cycle" and value.get("cycle") == cycle
        ),
        None,
    )
    if bundle_ref is None:
        raise BuildError(f"source publication has no cycle {cycle} bundle")
    bundle_path = package_path(packaged_root, bundle_ref)
    bundle = read_json(bundle_path)
    packages = bundle.get("packages", [])
    nav_package = next(
        (value for value in packages if value.get("family_id") == "nav-db"),
        None,
    )
    tpp_package = next(
        (
            value
            for value in packages
            if value.get("family_id") == "tpp" and value.get("region_id") == "nw"
        ),
        None,
    )
    if nav_package is None or tpp_package is None:
        raise BuildError("source bundle must contain NAVDB and NW TPP packages")
    source_contracts = current.get("contracts")
    if not isinstance(source_contracts, dict):
        raise BuildError("source current artifacts has no contracts")
    for package in (nav_package, tpp_package):
        family = package["family_id"]
        package_contract = package.get("contract_id")
        advertised_contract = source_contracts.get(family)
        if package_contract != advertised_contract:
            raise BuildError(
                f"{family} package provides {package_contract}; "
                f"current publication advertises {advertised_contract}"
            )

    output_root.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(
        tempfile.mkdtemp(prefix=f".{output_root.name}.", dir=output_root.parent)
    )
    try:
        published = temporary / "published"
        output_packaged = published / PUBLICATION_ROOT / "packaged"
        output_unpacked = published / PUBLICATION_ROOT / "unpacked"
        output_packaged.mkdir(parents=True)
        output_unpacked.mkdir(parents=True)

        nav_source = package_path(packaged_root, nav_package)
        nav_destination = output_packaged / nav_source.name
        shutil.copyfile(nav_source, nav_destination)
        nav_updated = update_package_file(
            nav_package, nav_destination.name, nav_destination
        )
        extract_zip(
            nav_destination,
            output_unpacked / nav_destination.name.removesuffix(".zip"),
        )

        tpp_source = package_path(packaged_root, tpp_package)
        preliminary_tpp = output_packaged / "tpp-e2e.zip"
        plate_count = compact_tpp_package(tpp_source, preliminary_tpp)
        tpp_digest = sha256(preliminary_tpp)
        tpp_filename = compact_package_filename(tpp_source.name, tpp_digest)
        tpp_destination = preliminary_tpp.with_name(tpp_filename)
        preliminary_tpp.rename(tpp_destination)
        tpp_updated = update_package_file(
            tpp_package, tpp_destination.name, tpp_destination
        )
        extract_zip(
            tpp_destination,
            output_unpacked / tpp_destination.name.removesuffix(".zip"),
        )

        compact_bundle = dict(bundle)
        compact_bundle.update(
            {
                "generated_at_utc": "2026-07-25T00:00:00Z",
                "effective_date": START_VALID,
                "expiration_date": END_VALID,
                "start_valid": START_VALID,
                "end_valid": END_VALID,
                "packages": [nav_updated, tpp_updated],
            }
        )
        preliminary_bundle = output_packaged / "bundle_e2e.json"
        write_json(preliminary_bundle, compact_bundle)
        bundle_digest = sha256(preliminary_bundle)
        bundle_filename = f"bundle_e2e_{cycle}_{bundle_digest}.json"
        bundle_destination = preliminary_bundle.with_name(bundle_filename)
        preliminary_bundle.rename(bundle_destination)

        compact_current = dict(current)
        compact_current["contracts"] = {
            "nav-db": nav_updated["contract_id"],
            "tpp": tpp_updated["contract_id"],
        }
        compact_current["artifact_roots"] = {
            "packaged": f"{PUBLICATION_ROOT}/packaged/",
            "unpacked": f"{PUBLICATION_ROOT}/unpacked/",
        }
        compact_current["as_of_date"] = "2026-07-25"
        compact_current["as_of_utc"] = "2026-07-25T00:00:00Z"
        compact_current["bundles"] = [
            {
                "filename": bundle_filename,
                "relative_path": bundle_filename,
                "id": compact_bundle["bundle_id"],
                "bundle_type": "cycle",
                "cycle": cycle,
                "cycle_version": compact_bundle.get("cycle_version", "01"),
                "start_valid": START_VALID,
                "end_valid": END_VALID,
                "checksum_sha256": bundle_digest,
                "size_bytes": bundle_destination.stat().st_size,
            }
        ]
        compact_current.pop("diagnostics", None)
        compact_current.pop("startup_prefetch", None)
        write_json(published / "current_artifacts.json", [compact_current])

        fixture_manifest = {
            "schema_version": FIXTURE_SCHEMA_VERSION,
            "fixture": FIXTURE_ID,
            "source_current_artifacts_sha256": sha256(current_path),
            "source_bundle_sha256": sha256(bundle_path),
            "source_cycle": cycle,
            "test_validity": {
                "start": START_VALID,
                "end": END_VALID,
            },
            "capabilities": {
                "plate": {
                    "georeferenced": {
                        "airport_id": f"K{TPP_AIRPORT_ID}",
                        "label_contains": TPP_PLATE_LABEL_CONTAINS,
                    }
                }
            },
            "packages": [
                {
                    "family_id": nav_updated["family_id"],
                    "contract_id": nav_updated["contract_id"],
                    "filename": nav_updated["filename"],
                    "size_bytes": nav_updated["size_bytes"],
                    "sha256": nav_updated["checksum_sha256"],
                },
                {
                    "family_id": tpp_updated["family_id"],
                    "contract_id": tpp_updated["contract_id"],
                    "filename": tpp_updated["filename"],
                    "size_bytes": tpp_updated["size_bytes"],
                    "sha256": tpp_updated["checksum_sha256"],
                    "airport_id": TPP_AIRPORT_ID,
                    "plate_count": plate_count,
                },
            ],
        }
        write_json(temporary / "fixture.json", fixture_manifest)
        (temporary / "README.md").write_text(
            f"""# Android Smoke Package Publication

This frozen publication drives the clean-emulator Android E2E suite through
the production Offline Packages discovery, download, verification, install,
and adoption paths. It contains the production {nav_updated["contract_id"]} package
for cycle {cycle} and a contract-valid TPP1 package restricted to KPLU plates.

The publication is test-only. Its package validity dates are widened so CI is
independent of wall-clock FAA cycles.
""",
            encoding="utf-8",
        )
        os.replace(temporary, output_root)
    except Exception:
        shutil.rmtree(temporary, ignore_errors=True)
        raise


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-publication", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--cycle", default="2607")
    args = parser.parse_args()
    try:
        build_fixture(
            args.source_publication.resolve(),
            args.output.resolve(),
            args.cycle,
        )
    except (BuildError, OSError, zipfile.BadZipFile) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(args.output.resolve())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
