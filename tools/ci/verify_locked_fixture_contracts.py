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
from fetch_test_artifacts import LockError, load_lock


ROOT = Path(__file__).resolve().parents[2]
CLIENT_INVENTORY = ROOT / "crates/product-contracts/contracts/client-data-contracts.json"
FIXTURE_LOCK = ROOT / "test-artifacts.lock.json"
REQUIRED_CLIENT_FIXTURES = {
    "android-smoke-publication",
    "nav-db-advance",
    "release-journey-publication",
}


def verify(client_inventory_path: Path, fixture_lock_path: Path) -> int:
    required = load_client_contract_inventory(client_inventory_path)
    lock = load_lock(fixture_lock_path)
    missing = sorted(
        name
        for name in REQUIRED_CLIENT_FIXTURES
        if name not in lock.fixtures or lock.fixtures[name].client_contracts is None
    )
    if missing:
        raise ContractInventoryError(
            "client-facing fixtures lack contract metadata: " + ", ".join(missing)
        )
    checked = 0
    for fixture in lock.fixtures.values():
        if fixture.client_contracts is None:
            continue
        assert_compatible(required, fixture.client_contracts, fixture.name)
        checked += 1
    return checked


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Verify locked fixtures against the compiled client data contracts."
    )
    parser.add_argument("--client-inventory", type=Path, default=CLIENT_INVENTORY)
    parser.add_argument("--fixture-lock", type=Path, default=FIXTURE_LOCK)
    args = parser.parse_args()
    try:
        count = verify(args.client_inventory, args.fixture_lock)
    except (ContractInventoryError, LockError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(f"Locked client fixture contracts match ({count} fixtures)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
