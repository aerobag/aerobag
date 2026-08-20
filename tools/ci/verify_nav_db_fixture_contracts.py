#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any


CONTRACT_PATTERN = re.compile(
    r'pub const NAV_DB_CONTRACT_ID:\s*&str\s*=\s*"([^"]+)";'
)


class ContractError(ValueError):
    pass


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ContractError(f"cannot read JSON {path}: {error}") from error


def required_client_contract(repo_root: Path) -> str:
    path = repo_root / "crates/product-contracts/src/lib.rs"
    try:
        source = path.read_text(encoding="utf-8")
    except OSError as error:
        raise ContractError(f"cannot read {path}: {error}") from error
    match = CONTRACT_PATTERN.search(source)
    if match is None:
        raise ContractError(f"cannot find NAV_DB_CONTRACT_ID in {path}")
    return match.group(1)


def fixture_contracts(fixture_root: Path, fixture: str) -> set[str]:
    manifest_path = fixture_root / {
        "android-smoke-publication": "e2e/android-smoke-publication/fixture.json",
        "nav-db-advance": "nav-db/advance-2608-to-2609/fixture.json",
    }[fixture]
    manifest = read_json(manifest_path)
    if fixture == "android-smoke-publication":
        packages = manifest.get("packages") if isinstance(manifest, dict) else None
        if not isinstance(packages, list):
            raise ContractError(f"{manifest_path} has no package list")
        contracts = {
            package.get("contract_id")
            for package in packages
            if isinstance(package, dict) and package.get("family_id") == "nav-db"
        }
    else:
        cycles = manifest.get("cycles") if isinstance(manifest, dict) else None
        if not isinstance(cycles, list):
            raise ContractError(f"{manifest_path} has no cycle list")
        contracts = {
            cycle.get("contract_id")
            for cycle in cycles
            if isinstance(cycle, dict)
        }
    if not contracts or any(not isinstance(value, str) or not value for value in contracts):
        raise ContractError(f"{manifest_path} has no valid NAVDB contract")
    return contracts


def verify(repo_root: Path, fixture_root: Path, fixtures: list[str]) -> str:
    required = required_client_contract(repo_root)
    for fixture in fixtures:
        offered = fixture_contracts(fixture_root, fixture)
        if offered != {required}:
            values = ", ".join(sorted(offered))
            raise ContractError(
                f"{fixture} provides NAVDB contract(s) [{values}]; "
                f"client requires {required}"
            )
    return required


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
    )
    parser.add_argument("--fixture-root", required=True, type=Path)
    parser.add_argument(
        "--fixture",
        action="append",
        required=True,
        choices=["android-smoke-publication", "nav-db-advance"],
    )
    args = parser.parse_args()
    try:
        contract = verify(
            args.repo_root.resolve(),
            args.fixture_root.resolve(),
            args.fixture,
        )
    except ContractError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(f"NAVDB fixtures match client contract {contract}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
