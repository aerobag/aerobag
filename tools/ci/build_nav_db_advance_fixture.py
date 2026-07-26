#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import sys
import tempfile
from pathlib import Path, PurePosixPath
from typing import Any


FIXTURE_SCHEMA_VERSION = 1
FIXTURE_PURPOSE = "transactional runtime NAVDB advance"


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


def safe_member_path(value: str) -> PurePosixPath:
    path = PurePosixPath(value)
    if path.is_absolute() or ".." in path.parts or "." in path.parts:
        raise BuildError(f"unsafe publication path {value!r}")
    return path


def publication_path(root: Path, relative: str) -> Path:
    path = root.joinpath(*safe_member_path(relative).parts)
    if not path.is_file():
        raise BuildError(f"publication artifact is missing: {path}")
    return path


def required_string(value: dict[str, Any], field: str, context: str) -> str:
    result = value.get(field)
    if not isinstance(result, str) or not result:
        raise BuildError(f"{context} has no {field}")
    return result


def selected_current(source_publication: Path) -> tuple[Path, dict[str, Any]]:
    current_path = source_publication / "current_artifacts.json"
    values = read_json(current_path)
    if not isinstance(values, list) or not values or not isinstance(values[-1], dict):
        raise BuildError("source current_artifacts.json must be a non-empty object array")
    return current_path, values[-1]


def find_cycle_bundle(current: dict[str, Any], cycle: str) -> dict[str, Any]:
    bundles = current.get("bundles")
    if not isinstance(bundles, list):
        raise BuildError("source current artifacts has no bundle list")
    matches = [
        value
        for value in bundles
        if isinstance(value, dict)
        and value.get("bundle_type") == "cycle"
        and value.get("cycle") == cycle
    ]
    if len(matches) != 1:
        raise BuildError(f"source publication must have exactly one cycle {cycle} bundle")
    return matches[0]


def find_nav_db_package(bundle: dict[str, Any], cycle: str) -> dict[str, Any]:
    packages = bundle.get("packages")
    if not isinstance(packages, list):
        raise BuildError(f"cycle {cycle} bundle has no package list")
    matches = [
        value
        for value in packages
        if isinstance(value, dict) and value.get("family_id") == "nav-db"
    ]
    if len(matches) != 1:
        raise BuildError(f"cycle {cycle} bundle must have exactly one NAVDB package")
    return matches[0]


def artifact_record(path: Path, output_relative: str) -> dict[str, Any]:
    return {
        "filename": output_relative,
        "sha256": sha256(path),
        "size_bytes": path.stat().st_size,
    }


def build_fixture(
    source_publication: Path,
    output_root: Path,
    cycles: list[str],
) -> None:
    if output_root.exists():
        raise BuildError(f"output already exists: {output_root}")
    if len(cycles) < 2 or len(set(cycles)) != len(cycles):
        raise BuildError("provide at least two distinct cycles")

    current_path, current = selected_current(source_publication)
    artifact_roots = current.get("artifact_roots")
    if not isinstance(artifact_roots, dict):
        raise BuildError("source current artifacts has no artifact roots")
    packaged_relative = required_string(
        artifact_roots, "packaged", "source artifact roots"
    )
    packaged_root = source_publication.joinpath(
        *safe_member_path(packaged_relative).parts
    )
    source_contracts = current.get("contracts")
    if not isinstance(source_contracts, dict):
        raise BuildError("source current artifacts has no contracts")
    source_nav_contract = required_string(
        source_contracts, "nav-db", "source contracts"
    )

    output_root.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(
        tempfile.mkdtemp(prefix=f".{output_root.name}.", dir=output_root.parent)
    )
    try:
        source_output = temporary / "source"
        packaged_output = source_output / "packaged"
        packaged_output.mkdir(parents=True)
        shutil.copyfile(current_path, source_output / "current_artifacts.json")

        cycle_records = []
        for cycle in cycles:
            bundle_ref = find_cycle_bundle(current, cycle)
            bundle_relative = required_string(
                bundle_ref, "relative_path", f"cycle {cycle} bundle reference"
            )
            bundle_path = publication_path(packaged_root, bundle_relative)
            bundle = read_json(bundle_path)
            if not isinstance(bundle, dict):
                raise BuildError(f"cycle {cycle} bundle must be an object")
            nav_package = find_nav_db_package(bundle, cycle)
            contract_id = required_string(
                nav_package, "contract_id", f"cycle {cycle} NAVDB package"
            )
            if contract_id != source_nav_contract:
                raise BuildError(
                    f"cycle {cycle} NAVDB provides {contract_id}; "
                    f"current publication advertises {source_nav_contract}"
                )
            nav_relative = required_string(
                nav_package, "relative_path", f"cycle {cycle} NAVDB package"
            )
            nav_path = publication_path(packaged_root, nav_relative)

            bundle_name = Path(bundle_relative).name
            nav_name = Path(nav_relative).name
            copied_bundle = packaged_output / bundle_name
            copied_nav = packaged_output / nav_name
            shutil.copyfile(bundle_path, copied_bundle)
            shutil.copyfile(nav_path, copied_nav)

            start_valid = bundle_ref.get("start_valid")
            end_valid = bundle_ref.get("end_valid")
            if not isinstance(start_valid, str) or not isinstance(end_valid, str):
                raise BuildError(f"cycle {cycle} bundle has no validity interval")
            cycle_records.append(
                {
                    "cycle": cycle,
                    "contract_id": contract_id,
                    "start_valid": start_valid,
                    "end_valid": end_valid,
                    "bundle": artifact_record(
                        copied_bundle, f"source/packaged/{bundle_name}"
                    ),
                    "nav_db": artifact_record(
                        copied_nav, f"source/packaged/{nav_name}"
                    ),
                }
            )

        publication_build = str(PurePosixPath(packaged_relative).parent)
        fixture = {
            "schema_version": FIXTURE_SCHEMA_VERSION,
            "purpose": f"{FIXTURE_PURPOSE} from {cycles[0]} to {cycles[-1]}",
            "source": {
                "publication_build": publication_build,
                "current_artifacts_filename": "source/current_artifacts.json",
                "current_artifacts_sha256": sha256(
                    source_output / "current_artifacts.json"
                ),
            },
            "cycles": cycle_records,
        }
        write_json(temporary / "fixture.json", fixture)
        (temporary / "README.md").write_text(
            f"""# NAVDB Advance {' To '.join(cycles)}

Immutable production {source_nav_contract} inputs for testing transactional
runtime NAVDB advance.

Source publication identity:

```text
{publication_build}
```

`source/` preserves the original publication index, selected cycle bundle
manifests, and NAVDB ZIP bytes. The index references products that are not
copied into this fixture; it records provenance and is not a minimal server
root.

Tests verify every copied artifact against `fixture.json`, then generate
deterministic publication views with controlled effective times.
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
    parser.add_argument("--cycle", action="append", required=True)
    args = parser.parse_args()
    try:
        build_fixture(
            args.source_publication.resolve(),
            args.output.resolve(),
            args.cycle,
        )
    except (BuildError, OSError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(args.output.resolve())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
