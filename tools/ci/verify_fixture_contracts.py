#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import argparse
import json
import lzma
import re
import sys
from pathlib import Path
from typing import Any


CONTRACT_PATTERN = re.compile(
    r'pub const NAV_DB_CONTRACT_ID:\s*&str\s*=\s*"([^"]+)";'
)
NOTAM_CONTRACT_PATTERN = re.compile(
    r"pub const NOTAM_LIVE_FEED_CONTRACT_VERSION:\s*u32\s*=\s*(\d+);"
)


class ContractError(ValueError):
    pass


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ContractError(f"cannot read JSON {path}: {error}") from error


def required_client_contracts(repo_root: Path) -> tuple[str, int]:
    path = repo_root / "crates/product-contracts/src/lib.rs"
    try:
        source = path.read_text(encoding="utf-8")
    except OSError as error:
        raise ContractError(f"cannot read {path}: {error}") from error
    nav_db_match = CONTRACT_PATTERN.search(source)
    if nav_db_match is None:
        raise ContractError(f"cannot find NAV_DB_CONTRACT_ID in {path}")
    notam_match = NOTAM_CONTRACT_PATTERN.search(source)
    if notam_match is None:
        raise ContractError(
            f"cannot find NOTAM_LIVE_FEED_CONTRACT_VERSION in {path}"
        )
    return nav_db_match.group(1), int(notam_match.group(1))


def fixture_contracts(fixture_root: Path, fixture: str) -> set[str]:
    manifest_path = fixture_root / {
        "android-smoke-publication": "e2e/android-smoke-publication/fixture.json",
        "nav-db-advance": "nav-db/advance-2608-to-2609/fixture.json",
        "release-journey-publication": (
            "e2e/release-journey-publication/published/current_artifacts.json"
        ),
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
    elif fixture == "nav-db-advance":
        cycles = manifest.get("cycles") if isinstance(manifest, dict) else None
        if not isinstance(cycles, list):
            raise ContractError(f"{manifest_path} has no cycle list")
        contracts = {
            cycle.get("contract_id")
            for cycle in cycles
            if isinstance(cycle, dict)
        }
    else:
        publications = manifest if isinstance(manifest, list) else None
        if not publications:
            raise ContractError(f"{manifest_path} has no publication list")
        contracts = {
            publication.get("contracts", {}).get("nav-db")
            for publication in publications
            if isinstance(publication, dict)
            and isinstance(publication.get("contracts"), dict)
        }
    if not contracts or any(not isinstance(value, str) or not value for value in contracts):
        raise ContractError(f"{manifest_path} has no valid NAVDB contract")
    return contracts


def _inside(root: Path, relative: str) -> Path:
    candidate = (root / relative).resolve()
    try:
        candidate.relative_to(root.resolve())
    except ValueError as error:
        raise ContractError(f"fixture path escapes its profile: {relative}") from error
    return candidate


def _read_state_json(profile_root: Path, relative: str) -> Any:
    path = _inside(profile_root, relative)
    try:
        payload = path.read_bytes()
        if path.suffix == ".xz":
            payload = lzma.decompress(payload)
        return json.loads(payload)
    except (OSError, lzma.LZMAError, json.JSONDecodeError) as error:
        raise ContractError(f"cannot read fixture state {path}: {error}") from error


def release_journey_notam_contracts(fixture_root: Path) -> set[int]:
    live_feeds = fixture_root / "e2e/release-journey-publication/live-feeds"
    contracts: set[int] = set()
    for profile in ("fresh", "mixed", "stale"):
        profile_root = live_feeds / profile
        current_path = profile_root / "current.json"
        current = read_json(current_path)
        products = current.get("products") if isinstance(current, dict) else None
        notams = products.get("notams") if isinstance(products, dict) else None
        state_url = notams.get("state_url") if isinstance(notams, dict) else None
        if not isinstance(state_url, str) or not state_url:
            raise ContractError(f"{current_path} has no NOTAM state URL")
        state = _read_state_json(profile_root, state_url)
        contract = state.get("contract_version") if isinstance(state, dict) else None
        if not isinstance(contract, int):
            raise ContractError(
                f"{profile}/{state_url} has no numeric NOTAM contract_version"
            )
        contracts.add(contract)
    return contracts


def verify(repo_root: Path, fixture_root: Path, fixtures: list[str]) -> str:
    required, required_notam = required_client_contracts(repo_root)
    for fixture in fixtures:
        offered = fixture_contracts(fixture_root, fixture)
        if offered != {required}:
            values = ", ".join(sorted(offered))
            raise ContractError(
                f"{fixture} provides NAVDB contract(s) [{values}]; "
                f"client requires {required}"
            )
    if "release-journey-publication" in fixtures:
        offered_notam = release_journey_notam_contracts(fixture_root)
        if offered_notam != {required_notam}:
            values = ", ".join(str(value) for value in sorted(offered_notam))
            raise ContractError(
                "release-journey-publication provides NOTAM contract(s) "
                f"[{values}]; client requires {required_notam}"
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
        choices=[
            "android-smoke-publication",
            "nav-db-advance",
            "release-journey-publication",
        ],
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
    print(f"Fixture contracts match client requirements (NAVDB {contract})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
