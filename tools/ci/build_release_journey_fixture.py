#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import argparse
from datetime import datetime
import hashlib
import json
import lzma
import math
import os
import shutil
import subprocess
import tempfile
import zipfile
from pathlib import Path
from typing import Any, Callable, Iterable

from build_e2e_package_fixture import (
    BuildError,
    deterministic_zip_info,
    package_path,
    read_json,
    safe_member_path,
    sha256,
    update_package_file,
    write_json,
)


FIXTURE_SCHEMA_VERSION = 1
FIXTURE_ID = "release-journey-publication"
PUBLICATION_ROOT = "release-e2e-v1"
START_VALID = "2020-01-01"
END_VALID = "2100-01-01"
MAP_CENTER = (47.493, -122.216)
TILE_WINDOW_CENTERS = (MAP_CENTER, (33.9425, -118.4081))
# The Android portrait map is taller than the web release viewport. Keep a
# 7x7 source window so both platforms can paint a full zoom-10 viewport and
# still pan far enough to prove newly revealed tiles load.
MAP_TILE_RADIUS = 3
LIVE_FEED_HISTORY_LIMIT = 2
FIXTURE_PIREP_ID = "pirep:release-journey-ksea"
TPP_ASSETS = {
    "nw": {
        "KSEA": {"BANGR NINE", "CHINS FIVE", "AIRPORT DIAGRAM"},
        "KPAE": {"RNAV (GPS) RWY 34L", "CHINS FIVE"},
    },
    "nc": {"KOMA": {"ILS OR LOC RWY 32R"}},
}
# CSUP asset metadata uses the FAA location identifier rather than the ICAO alias.
CSUP_AIRPORTS = {"nw": {"SEA"}}
TILE_FAMILIES = {"sec", "tac", "enr-l", "enr-h", "terrain", "shaded-relief"}
REGIONAL_FAMILIES = {"terrain", "shaded-relief"}
CHART_FAMILIES = {"sec", "tac", "enr-l", "enr-h"}


def slippy_tms_tile(latitude: float, longitude: float, zoom: int) -> tuple[int, int]:
    scale = 1 << zoom
    x = int((longitude + 180.0) / 360.0 * scale)
    latitude_rad = math.radians(max(-85.05112878, min(85.05112878, latitude)))
    xyz_y = int(
        (1.0 - math.asinh(math.tan(latitude_rad)) / math.pi) / 2.0 * scale
    )
    return x, scale - 1 - xyz_y


def tile_coordinates(name: str) -> tuple[int, int, int] | None:
    parts = safe_member_path(name).parts
    if not parts or parts[0] != "tiles":
        return None
    if len(parts) == 4:
        _, zoom, x, y_name = parts
    elif len(parts) == 5:
        _, _, zoom, x, y_name = parts
    else:
        return None
    try:
        return int(zoom), int(x), int(Path(y_name).stem)
    except ValueError:
        return None


def tile_is_in_window(name: str, radius: int = MAP_TILE_RADIUS) -> bool:
    coordinates = tile_coordinates(name)
    if coordinates is None:
        return not name.startswith("tiles/")
    zoom, x, y = coordinates
    return any(
        abs(x - center_x) <= radius and abs(y - center_y) <= radius
        for center_x, center_y in (
            slippy_tms_tile(*center, zoom) for center in TILE_WINDOW_CENTERS
        )
    )


def deterministic_zip(
    source: Path,
    destination: Path,
    select: Callable[[str], bool],
    overrides: dict[str, bytes] | None = None,
) -> list[str]:
    overrides = overrides or {}
    destination.parent.mkdir(parents=True, exist_ok=True)
    selected: dict[str, bytes] = {}
    with zipfile.ZipFile(source) as archive:
        for member in archive.infolist():
            if member.is_dir() or not select(member.filename):
                continue
            safe_member_path(member.filename)
            selected[member.filename] = overrides.get(
                member.filename, archive.read(member.filename)
            )
    selected.update(overrides)
    if not selected:
        raise BuildError(f"compaction selected no members from {source}")
    with zipfile.ZipFile(destination, "w", allowZip64=True) as output:
        for name in sorted(selected):
            output.writestr(deterministic_zip_info(name), selected[name])
    return sorted(selected)


def compact_path_manifest(payload: bytes, selected_names: set[str]) -> bytes:
    lines = payload.decode("utf-8").splitlines()
    if not lines:
        raise BuildError("package path manifest is empty")
    return (lines[0] + "\n" + "\n".join(
        line for line in lines[1:] if line in selected_names
    ) + "\n").encode("utf-8")


def compact_tile_package(source: Path, destination: Path) -> int:
    with zipfile.ZipFile(source) as archive:
        names = archive.namelist()
        selected_names = {name for name in names if tile_is_in_window(name)}
        selected_tiles = [name for name in selected_names if name.startswith("tiles/")]
        if not selected_tiles:
            raise BuildError(f"tile window selected no tiles from {source}")
        overrides = {
            name: compact_path_manifest(archive.read(name), selected_names)
            for name in names
            if name.endswith(".manifest")
        }
    deterministic_zip(source, destination, lambda name: name in selected_names, overrides)
    return len(selected_tiles)


def compact_asset_package(
    source: Path,
    destination: Path,
    airport_ids: set[str],
    labels_by_airport: dict[str, set[str]] | None = None,
) -> int:
    with zipfile.ZipFile(source) as archive:
        names = archive.namelist()
        if "package-assets.json" not in names:
            raise BuildError(f"asset package has no package-assets.json: {source}")
        manifest = json.loads(archive.read("package-assets.json"))
        def selected(asset: dict[str, Any]) -> bool:
            airport_id = asset.get("airport_id")
            if airport_id not in airport_ids:
                return False
            if labels_by_airport is None:
                return True
            label = asset.get("label", "")
            return any(value in label for value in labels_by_airport[airport_id])

        assets = [asset for asset in manifest.get("assets", []) if selected(asset)]
        found_airports = {asset.get("airport_id") for asset in assets}
        missing = airport_ids - found_airports
        if missing:
            raise BuildError(
                f"asset package {source.name} is missing airports {sorted(missing)}"
            )
        if labels_by_airport is not None:
            missing_labels = [
                f"{airport_id}:{label}"
                for airport_id, labels in labels_by_airport.items()
                for label in labels
                if not any(
                    asset.get("airport_id") == airport_id
                    and label in asset.get("label", "")
                    for asset in assets
                )
            ]
            if missing_labels:
                raise BuildError(
                    f"asset package {source.name} is missing named assets "
                    f"{sorted(missing_labels)}"
                )
        selected_names = {"package-assets.json"}
        for asset in assets:
            for field in ("asset_path", "thumbnail_path"):
                path = asset.get(field)
                if isinstance(path, str):
                    selected_names.add(path)
        path_manifests = [name for name in names if name.endswith(".manifest")]
        selected_names.update(path_manifests)
        manifest["assets"] = assets
        overrides = {
            "package-assets.json": (json.dumps(manifest, indent=2) + "\n").encode(),
        }
        for name in path_manifests:
            overrides[name] = compact_path_manifest(
                archive.read(name), selected_names
            )
    deterministic_zip(source, destination, lambda name: name in selected_names, overrides)
    return len(assets)


def compact_package(
    packaged_root: Path,
    output_packaged: Path,
    package: dict[str, Any],
) -> tuple[dict[str, Any], dict[str, Any]]:
    source = package_path(packaged_root, package)
    preliminary = output_packaged / f"compact-{package['id']}.zip"
    family = package["family_id"]
    region = package.get("region_id")
    if family in TILE_FAMILIES:
        member_count = compact_tile_package(source, preliminary)
    elif family == "tpp":
        labels = TPP_ASSETS[region]
        member_count = compact_asset_package(
            source, preliminary, set(labels), labels
        )
    elif family == "csup":
        member_count = compact_asset_package(source, preliminary, CSUP_AIRPORTS[region])
    else:
        shutil.copyfile(source, preliminary)
        with zipfile.ZipFile(preliminary) as archive:
            member_count = len(archive.namelist())
    destination = preliminary.with_name(source.name)
    preliminary.rename(destination)
    return (
        update_package_file(package, destination.name, destination),
        {"family_id": family, "region_id": region, "member_count": member_count},
    )


def selected_packages(bundle: dict[str, Any]) -> list[dict[str, Any]]:
    selected = []
    for package in bundle.get("packages", []):
        family = package.get("family_id")
        region = package.get("region_id")
        if family in {"nav-db", "world-basemap"}:
            selected.append(package)
        elif family in CHART_FAMILIES and region in {"nw", "wide"}:
            selected.append(package)
        elif family in {"sec", "tac"} and region == "sw":
            selected.append(package)
        elif family in REGIONAL_FAMILIES and region == "nw":
            selected.append(package)
        elif family == "tpp" and region in TPP_ASSETS:
            selected.append(package)
        elif family == "csup" and region in CSUP_AIRPORTS:
            selected.append(package)
    required = {
        "nav-db", "world-basemap", "sec", "tac", "enr-l", "enr-h",
        "terrain", "shaded-relief", "tpp", "csup",
    }
    missing = required - {package["family_id"] for package in selected}
    if missing:
        raise BuildError(f"source bundle is missing release fixture families {sorted(missing)}")
    return selected


def query_had(had_query: Path, nav_package: Path, key: str) -> Any:
    result = subprocess.run(
        [str(had_query), str(nav_package), key],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode != 0:
        raise BuildError(f"had_query failed for {key}: {result.stderr.strip()}")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise BuildError(f"had_query returned invalid JSON for {key}: {error}") from error


def validate_nav_capabilities(had_query: Path, nav_package: Path) -> None:
    airport_info = {
        airport: query_had(had_query, nav_package, f"airport/info/{airport}")
        for airport in ("KSEA", "KRNT", "S88", "KPAE")
    }
    if len(airport_info["KSEA"].get("runways", [])) < 3 or any(
        end.get("latitude") is None
        for runway in airport_info["KSEA"]["runways"]
        for end in (runway["end_a"], runway["end_b"])
    ):
        raise BuildError("KSEA no longer supplies a complete runway complex")
    if not any(
        end.get("latitude") is None
        for runway in airport_info["S88"].get("runways", [])
        for end in (runway["end_a"], runway["end_b"])
    ):
        raise BuildError("S88 no longer exercises fallback runway geometry")
    if airport_info["KRNT"].get("traffic_pattern_altitude_msl_ft") is None:
        raise BuildError("KRNT no longer has a published traffic-pattern altitude")
    if airport_info["KPAE"].get("traffic_pattern_altitude_msl_ft") is not None:
        raise BuildError("KPAE no longer exercises derived traffic-pattern altitude")

    expected = {
        "KSEA": ("sid", "BANGR9"),
        "KPAE": ("star", "CHINS5"),
    }
    for airport, (kind, procedure_id) in expected.items():
        index = query_had(had_query, nav_package, f"plate/airport/{airport}")
        if not any(
            item.get("kind") == kind and item.get("procedure_id") == procedure_id
            for item in index.get("charted_procedures", [])
        ):
            raise BuildError(f"{airport} no longer exposes {kind} {procedure_id}")
    koma = query_had(had_query, nav_package, "plate/airport/KOMA")
    if not any("ILS OR LOC RWY 32R" in value for value in koma.get("chart_ids", [])):
        raise BuildError("KOMA ILS OR LOC RWY 32R plate is missing")
    airway = query_had(had_query, nav_package, "airway/V4")
    if not any(
        {"MEDEA", "YKM"}.issubset({
            next(iter(point.get("nav_ref", {}).values()), None)
            for point in branch.get("points", [])
        })
        for branch in airway
    ):
        raise BuildError("V4 no longer contains MEDEA and YKM on one branch")


def build_publication(
    source_publication: Path,
    source_current: dict[str, Any],
    output: Path,
    cycle: str,
    had_query: Path,
) -> dict[str, Any]:
    packaged_relative = source_current.get("artifact_roots", {}).get("packaged")
    if not isinstance(packaged_relative, str):
        raise BuildError("source publication has no packaged artifact root")
    packaged_root = source_publication.joinpath(*safe_member_path(packaged_relative).parts)
    bundle_ref = next(
        (bundle for bundle in source_current.get("bundles", []) if bundle.get("cycle") == cycle),
        None,
    )
    if bundle_ref is None:
        raise BuildError(f"source publication has no cycle {cycle}")
    source_bundle_path = package_path(packaged_root, bundle_ref)
    source_bundle = read_json(source_bundle_path)
    packages = selected_packages(source_bundle)

    output_packaged = output / PUBLICATION_ROOT / "packaged"
    output_packaged.mkdir(parents=True)
    compact_packages = []
    package_diagnostics = []
    for package in packages:
        compact, diagnostics = compact_package(packaged_root, output_packaged, package)
        compact_packages.append(compact)
        package_diagnostics.append(diagnostics)

    nav_package = next(package for package in compact_packages if package["family_id"] == "nav-db")
    validate_nav_capabilities(had_query, output_packaged / nav_package["relative_path"])

    compact_bundle = dict(source_bundle)
    compact_bundle.update({
        "generated_at_utc": "2026-08-20T00:00:00Z",
        "effective_date": START_VALID,
        "expiration_date": END_VALID,
        "start_valid": START_VALID,
        "end_valid": END_VALID,
        "packages": compact_packages,
    })
    preliminary_bundle = output_packaged / f"bundle_release_e2e_{cycle}.json"
    write_json(preliminary_bundle, compact_bundle)
    bundle_digest = sha256(preliminary_bundle)
    bundle_path = preliminary_bundle.with_name(
        f"bundle_release_e2e_{cycle}_{bundle_digest}.json"
    )
    preliminary_bundle.rename(bundle_path)

    compact_current = dict(source_current)
    compact_current["artifact_roots"] = {
        "packaged": f"{PUBLICATION_ROOT}/packaged/",
        "unpacked": f"{PUBLICATION_ROOT}/unpacked/",
    }
    compact_current["as_of_date"] = "2026-08-20"
    compact_current["as_of_utc"] = "2026-08-20T00:00:00Z"
    compact_current["bundles"] = [{
        "filename": bundle_path.name,
        "relative_path": bundle_path.name,
        "id": compact_bundle["bundle_id"],
        "bundle_type": "cycle",
        "cycle": cycle,
        "cycle_version": compact_bundle.get("cycle_version", "01"),
        "start_valid": START_VALID,
        "end_valid": END_VALID,
        "checksum_sha256": bundle_digest,
        "size_bytes": bundle_path.stat().st_size,
    }]
    compact_current.pop("startup_prefetch", None)
    compact_current.pop("diagnostics", None)
    write_json(output / "current_artifacts.json", [compact_current])
    return {
        "cycle": cycle,
        "source_bundle_sha256": sha256(source_bundle_path),
        "packages": package_diagnostics,
    }


def fixture_capabilities(reference_epoch_ms: int) -> dict[str, Any]:
    return {
        "reference_epoch_ms": reference_epoch_ms,
        "initial_viewport": {"latitude": MAP_CENTER[0], "longitude": MAP_CENTER[1], "zoom": 10},
        "raster_families": ["none", "sec", "tac", "flyway", "enr-l", "enr-h", "shaded-relief"],
        "airport": {
            "runway_complex": "KSEA",
            "runway_fallback": "S88",
            "published_tpa": "KRNT",
            "derived_tpa": "KPAE",
        },
        "airway": {"entry": "MEDEA", "airway": "V4", "exit": "YKM"},
        "procedure": {
            "sid": {"airport_id": "KSEA", "procedure_id": "BANGR9"},
            "star": {"airport_id": "KPAE", "procedure_id": "CHINS5", "transition": "YKM"},
            "approach": {"airport_id": "KOMA", "procedure_id": "I32R", "transition": "OVR"},
        },
        "plate": {
            "georeferenced": {"airport_id": "KPAE", "label_contains": "RNAV 34L"},
            "multi_page_rotated": {"airport_id": "KPAE", "label_contains": "CHINS FIVE"},
            "notam": {"airport_id": "KOMA", "label_contains": "ILS OR LOC 32R"},
            "geometry_warning": {"airport_id": "KOMA", "label_contains": "ILS OR LOC 32R"},
            "legend": {"family_id": "tac", "label_contains": "Seattle TAC"},
            "inset": {
                "family_id": "tac",
                "map_airport_id": "KLAX",
                "label_contains": "Los Angeles TAC Insets",
            },
        },
        "document": {
            "csup": {"airport_id": "KSEA"},
            "other": {"airport_id": "KSEA", "label_contains": "AIRPORT DIAGRAM"},
        },
        "replay_trace": "replay/track-gap.json",
        "second_publication": {"fixture": "nav-db-advance"},
        "live_feeds": {
            "empty": "live-feeds/empty",
            "fresh": "live-feeds/fresh",
            "mixed": "live-feeds/mixed",
            "stale": "live-feeds/stale",
            "pirep_target_airport": "KSEA",
            "tfr_target_airport": "27W",
        },
    }


def copy_live_feed_resource(source: Path, output: Path, resource: dict[str, Any]) -> None:
    url = resource.get("url")
    if not isinstance(url, str):
        return
    relative = safe_member_path(url)
    source_path = source.joinpath(*relative.parts)
    destination = output.joinpath(*relative.parts)
    if source_path.is_dir():
        shutil.copytree(source_path, destination, dirs_exist_ok=True)
    elif source_path.name == "manifest.json" and source_path.parent.is_dir():
        shutil.copytree(source_path.parent, destination.parent, dirs_exist_ok=True)
    elif source_path.is_file():
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source_path, destination)
    else:
        raise BuildError(f"live-feed resource is missing: {source_path}")


def live_feed_manifest_resources(value: Any) -> Iterable[dict[str, Any]]:
    if isinstance(value, dict):
        if isinstance(value.get("url"), str):
            yield value
        for child in value.values():
            yield from live_feed_manifest_resources(child)
    elif isinstance(value, list):
        for child in value:
            yield from live_feed_manifest_resources(child)


def canonical_json_bytes(value: Any) -> bytes:
    return json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")


def install_fixture_pirep(output: Path, current: dict[str, Any]) -> bool:
    product = current.get("products", {}).get("pireps")
    if not isinstance(product, dict):
        return False
    state_url = product.get("state_url")
    manifest_url = product.get("version_manifest_url")
    version = product.get("current")
    if not all(isinstance(value, str) for value in (state_url, manifest_url, version)):
        raise BuildError("PIREP fixture product has incomplete state references")

    state_path = output.joinpath(*safe_member_path(state_url).parts)
    manifest_path = output.joinpath(*safe_member_path(manifest_url).parts)
    try:
        state = json.loads(lzma.decompress(state_path.read_bytes()))
    except (OSError, lzma.LZMAError, json.JSONDecodeError) as error:
        raise BuildError(f"cannot decode PIREP fixture state {state_path}: {error}") from error
    if state.get("version_label") != version:
        raise BuildError("PIREP fixture state version does not match current.json")
    records = state.get("pireps_by_id")
    if not isinstance(records, dict):
        raise BuildError("PIREP fixture state has no pireps_by_id object")

    observed_at_utc = current.get("generated_at_utc")
    if not isinstance(observed_at_utc, str):
        raise BuildError("live-feed fixture has no generated_at_utc for synthetic PIREP")
    records[FIXTURE_PIREP_ID] = {
        "id": FIXTURE_PIREP_ID,
        "raw_text": "KSEA UA /OV SEA/TM RELEASE/FI TEST/TP C172",
        "observed_at_utc": observed_at_utc,
        "report_type": "PIREP",
        "longitude": MAP_CENTER[1],
        "latitude": MAP_CENTER[0],
        "symbol": "generic",
        "icing": "none",
        "turbulence": "none",
    }
    state["pirep_count"] = len(records)
    state_bytes = canonical_json_bytes(state)
    state_sha256 = hashlib.sha256(state_bytes).hexdigest()
    compressed = lzma.compress(state_bytes, format=lzma.FORMAT_XZ, preset=6)
    state_path.write_bytes(compressed)

    manifest = read_json(manifest_path)
    manifest.pop("previous", None)
    manifest.pop("delta_from_previous", None)
    manifest["state"] = {
        "kind": "json_xz",
        "url": state_url,
        "bytes": len(compressed),
        "blob_sha256": hashlib.sha256(compressed).hexdigest(),
        "state_sha256": state_sha256,
    }
    write_json(manifest_path, manifest)
    product["state_sha256"] = state_sha256

    installed = records.get(FIXTURE_PIREP_ID)
    if (
        installed is None
        or abs(installed.get("latitude", 0) - MAP_CENTER[0]) > 0.001
        or abs(installed.get("longitude", 0) - MAP_CENTER[1]) > 0.001
    ):
        raise BuildError("synthetic PIREP is not visible in the initial viewport")
    return True


def compact_live_feed_fixture(source: Path, output: Path, *, stale: bool) -> dict[str, Any]:
    current_path = source / "current.json"
    current = read_json(current_path)
    if current.get("schema_version") != 3:
        raise BuildError(f"live-feed fixture requires schema 3, got {current.get('schema_version')}")
    compact = dict(current)
    compact["products"] = {}
    if stale:
        compact["generated_at_utc"] = "2020-01-01T00:00:00Z"
    copied_versions = 0
    for product, value in sorted(current.get("products", {}).items()):
        product_value = dict(value)
        history_limit = (
            LIVE_FEED_HISTORY_LIMIT if product == "nexrad"
            else 1 if product in {"metars", "pireps", "tafs"}
            else 0
        )
        history = list(value.get("history", []))[-history_limit:] if history_limit else []
        product_value["history"] = history
        if stale:
            for field in ("collected_at_utc", "observed_at_utc", "published_at_utc"):
                if field in product_value:
                    product_value[field] = "2020-01-01T00:00:00Z"
        compact["products"][product] = product_value
        references = history + [{
            "version": value.get("current"),
            "version_manifest_url": value.get("version_manifest_url"),
        }]
        for reference in references:
            manifest_url = reference.get("version_manifest_url")
            if not isinstance(manifest_url, str):
                raise BuildError(f"{product} live-feed version has no manifest URL")
            relative = safe_member_path(manifest_url)
            source_manifest = source.joinpath(*relative.parts)
            manifest = read_json(source_manifest)
            for resource in live_feed_manifest_resources(manifest):
                copy_live_feed_resource(source, output, resource)
            destination = output.joinpath(*relative.parts)
            destination.parent.mkdir(parents=True, exist_ok=True)
            write_json(destination, manifest)
            copied_versions += 1
    pirep_overlay = install_fixture_pirep(output, compact)
    output.mkdir(parents=True, exist_ok=True)
    write_json(output / "current.json", compact)
    return {
        "source_current_sha256": sha256(current_path),
        "product_count": len(compact["products"]),
        "version_count": copied_versions,
        "stale": stale,
        "pirep_overlay": pirep_overlay,
    }


def write_replay_fixture(root: Path) -> Path:
    replay = root / "replay" / "track-gap.json"
    replay.parent.mkdir(parents=True)
    write_json(replay, {
        "r": "N-RELEASE",
        "t": "C172",
        "trace": [
            [0.0, 47.493, -122.216, 1500, 105, 320],
            [0.25, 47.497, -122.222, 1550, 105, 320],
            [0.5, 47.501, -122.228, 1600, 105, None],
            [4.0, 47.505, -122.234, 1650, 105, None],
            [4.25, 47.509, -122.240, 1700, 105, 300],
            [4.5, 47.513, -122.246, 1750, 105, 300],
            [4.75, 47.517, -122.252, 1800, 105, 300],
            [5.0, 47.521, -122.258, 1850, 105, 300],
            [5.25, 47.525, -122.264, 1900, 105, 300],
            [5.5, 47.529, -122.270, 1950, 105, 300],
            [5.75, 47.533, -122.276, 2000, 105, 300],
            [6.0, 47.537, -122.282, 2050, 105, 300],
            [6.25, 47.541, -122.288, 2100, 105, 300],
            [6.5, 47.545, -122.294, 2150, 105, 300],
            [6.75, 47.549, -122.300, 2200, 105, 300],
            [7.0, 47.553, -122.306, 2250, 105, 300],
            [7.25, 47.557, -122.312, 2300, 105, 300],
            [7.5, 47.561, -122.318, 2350, 105, 300],
            [7.75, 47.565, -122.324, 2400, 105, 300],
            [8.0, 47.569, -122.330, 2450, 105, 300],
            [8.25, 47.573, -122.336, 2500, 105, 300],
            [8.5, 47.577, -122.342, 2550, 105, 300],
        ],
    })
    return replay


def write_auxiliary_fixtures(root: Path, live_feed_source: Path) -> list[dict[str, Any]]:
    write_replay_fixture(root)
    empty = root / "live-feeds" / "empty"
    fresh = root / "live-feeds" / "fresh"
    mixed = root / "live-feeds" / "mixed"
    stale = root / "live-feeds" / "stale"
    fresh_diagnostics = compact_live_feed_fixture(live_feed_source, fresh, stale=False)
    if not fresh_diagnostics["pirep_overlay"]:
        raise BuildError("live-feed fixture requires a PIREP product")
    fresh_current = read_json(fresh / "current.json")
    empty.mkdir(parents=True)
    write_json(empty / "current.json", {
        "schema_version": fresh_current["schema_version"],
        "generated_at_utc": fresh_current["generated_at_utc"],
        "products": {},
    })
    shutil.copytree(fresh, mixed, copy_function=os.link)
    mixed_current_path = mixed / "current.json"
    mixed_current = read_json(mixed_current_path)
    mixed_products = mixed_current.get("products", {})
    stale_product = mixed_products.get("tfrs")
    if not isinstance(stale_product, dict):
        raise BuildError("live-feed fixture has no TFR product for stale status coverage")
    for field in ("collected_at_utc", "observed_at_utc", "published_at_utc"):
        if field in stale_product:
            stale_product[field] = "2020-01-01T00:00:00Z"
    if mixed_products.pop("pireps", None) is None:
        raise BuildError("live-feed fixture has no PIREP product for missing status coverage")
    mixed_current_path.unlink()
    write_json(mixed_current_path, mixed_current)
    shutil.copytree(fresh, stale, copy_function=os.link)
    stale_current_path = stale / "current.json"
    stale_current = read_json(stale_current_path)
    stale_current["generated_at_utc"] = "2020-01-01T00:00:00Z"
    for value in stale_current.get("products", {}).values():
        for field in ("collected_at_utc", "observed_at_utc", "published_at_utc"):
            if field in value:
                value[field] = "2020-01-01T00:00:00Z"
    stale_current_path.unlink()
    write_json(stale_current_path, stale_current)
    stale_diagnostics = dict(fresh_diagnostics)
    stale_diagnostics["stale"] = True
    mixed_diagnostics = dict(fresh_diagnostics)
    mixed_diagnostics.update({"mixed": True, "missing_product": "pireps", "stale_product": "tfrs"})
    empty_diagnostics = {
        "product_count": 0,
        "version_count": 0,
        "empty": True,
    }
    return [empty_diagnostics, fresh_diagnostics, mixed_diagnostics, stale_diagnostics]


def live_feed_reference_epoch_ms(live_feed_source: Path) -> int:
    current = read_json(live_feed_source / "current.json")
    generated_at_utc = current.get("generated_at_utc")
    if not isinstance(generated_at_utc, str):
        raise BuildError("live-feed fixture current.json has no generated_at_utc")
    try:
        instant = datetime.fromisoformat(generated_at_utc.replace("Z", "+00:00"))
    except ValueError as error:
        raise BuildError(f"invalid live-feed generated_at_utc {generated_at_utc!r}") from error
    if instant.tzinfo is None:
        raise BuildError("live-feed generated_at_utc must include a UTC offset")
    return round(instant.timestamp() * 1000)


def build_fixture(
    source_publication: Path,
    output_root: Path,
    primary_cycle: str,
    had_query: Path,
    live_feed_source: Path,
) -> None:
    if output_root.exists():
        raise BuildError(f"output already exists: {output_root}")
    current_path = source_publication / "current_artifacts.json"
    current_values = read_json(current_path)
    if not isinstance(current_values, list) or not current_values:
        raise BuildError("source current_artifacts.json must be a non-empty list")
    current = current_values[-1]
    output_root.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(tempfile.mkdtemp(prefix=f".{output_root.name}.", dir=output_root.parent))
    try:
        primary = build_publication(
            source_publication, current, temporary / "published", primary_cycle, had_query
        )
        live_feeds = write_auxiliary_fixtures(temporary, live_feed_source)
        write_json(temporary / "fixture.json", {
            "schema_version": FIXTURE_SCHEMA_VERSION,
            "fixture": FIXTURE_ID,
            "publication_root": "published",
            "source_current_artifacts_sha256": sha256(current_path),
            "publications": [primary],
            "live_feed_publications": live_feeds,
            "dependencies": ["nav-db-advance"],
            "capabilities": fixture_capabilities(live_feed_reference_epoch_ms(live_feed_source)),
        })
        temporary.rename(output_root)
    except BaseException:
        shutil.rmtree(temporary, ignore_errors=True)
        raise


def main() -> int:
    parser = argparse.ArgumentParser(description="Build the capability-addressed release journey fixture.")
    parser.add_argument("--source-publication", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--primary-cycle", required=True)
    parser.add_argument("--had-query", type=Path, required=True)
    parser.add_argument("--live-feed-source", type=Path, required=True)
    args = parser.parse_args()
    try:
        build_fixture(
            args.source_publication.resolve(), args.output.resolve(),
            args.primary_cycle, args.had_query.resolve(), args.live_feed_source.resolve(),
        )
    except BuildError as error:
        print(f"error: {error}")
        return 1
    print(args.output.resolve())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
