#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Verify an immutable release-E2E application bundle before emulator use."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys
import zipfile


SEMANTIC_DRIVER_PROTOCOL = "aerobag-semantic-driver/16"


class VerificationError(RuntimeError):
    pass


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_file(root: Path, record: object, label: str) -> Path:
    if not isinstance(record, dict):
        raise VerificationError(f"build manifest has no {label} record")
    path_value = record.get("path")
    if not isinstance(path_value, str) or not path_value:
        raise VerificationError(f"build manifest has no {label} path")
    path = root / path_value
    if not path.is_file():
        raise VerificationError(f"{label} is missing: {path}")
    if record.get("size_bytes") != path.stat().st_size:
        raise VerificationError(f"{label} size does not match its build manifest")
    if record.get("sha256") != sha256(path):
        raise VerificationError(f"{label} digest does not match its build manifest")
    return path


def apk_contains_protocol(apk: Path, protocol: str) -> bool:
    encoded = protocol.encode("utf-8")
    with zipfile.ZipFile(apk) as archive:
        dex_names = sorted(name for name in archive.namelist() if name.endswith(".dex"))
        return any(encoded in archive.read(name) for name in dex_names)


def verify_bundle(root: Path) -> None:
    manifest_path = root / "build-manifest.json"
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise VerificationError(f"cannot read build manifest: {error}") from error
    if manifest.get("schema_version") != 1:
        raise VerificationError("unsupported release-E2E build manifest")
    verify_file(root, manifest.get("android_apk"), "Android APK")
    driver_record = manifest.get("android_e2e_driver_apk")
    driver = verify_file(root, driver_record, "Android semantic driver APK")
    protocol = driver_record.get("protocol") if isinstance(driver_record, dict) else None
    if protocol != SEMANTIC_DRIVER_PROTOCOL:
        raise VerificationError(
            "Android semantic driver manifest protocol mismatch: "
            f"expected {SEMANTIC_DRIVER_PROTOCOL}, got {protocol!r}"
        )
    if not apk_contains_protocol(driver, protocol):
        raise VerificationError(
            f"Android semantic driver APK does not implement {protocol}"
        )
    verify_file(root, manifest.get("cloud_server"), "Cloud server")
    if not (root / "web-dist/index.html").is_file():
        raise VerificationError("web-dist/index.html is missing")
    if not (root / "web-dist/about.html").is_file():
        raise VerificationError("web-dist/about.html is missing")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("bundle", type=Path)
    args = parser.parse_args()
    verify_bundle(args.bundle.resolve())
    print(f"release-E2E app bundle verified: {args.bundle}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except VerificationError as error:
        print(f"verify_release_e2e_apps: {error}", file=sys.stderr)
        raise SystemExit(1)
