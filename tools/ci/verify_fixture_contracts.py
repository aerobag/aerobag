#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from contract_inventory import (
    ContractInventoryError,
    assert_compatible,
    load_client_contract_inventory,
)
from fetch_test_artifacts import LockError, load_lock, validate_fixture


class ContractError(ValueError):
    pass


def verify(repo_root: Path, fixture_root: Path, fixtures: list[str]) -> int:
    inventory = load_client_contract_inventory(
        repo_root / "crates/product-contracts/contracts/client-data-contracts.json"
    )
    lock = load_lock(repo_root / "test-artifacts.lock.json")
    checked = 0
    for name in fixtures:
        fixture = lock.fixtures.get(name)
        if fixture is None:
            raise ContractError(f"fixture is not declared in lock: {name}")
        if fixture.client_contracts is None:
            raise ContractError(f"fixture has no client contract metadata: {name}")
        try:
            assert_compatible(inventory, fixture.client_contracts, name)
            validate_fixture(fixture_root, fixture)
        except (ContractInventoryError, LockError) as error:
            raise ContractError(str(error)) from error
        checked += 1
    return checked


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
    )
    parser.add_argument("--fixture-root", required=True, type=Path)
    parser.add_argument("--fixture", action="append", required=True)
    args = parser.parse_args()
    try:
        count = verify(
            args.repo_root.resolve(),
            args.fixture_root.resolve(),
            args.fixture,
        )
    except ContractError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(f"Fixture manifests and client contracts match ({count} fixtures)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
