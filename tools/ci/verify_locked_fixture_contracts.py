#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import argparse
import json
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
ROLLOVER_SOURCE = ROOT / "crates/nav-db-fixture/source.json"
REQUIRED_CLIENT_FIXTURES = {
    "android-smoke-publication",
    "release-journey-publication",
}


def verify(
    client_inventory_path: Path,
    fixture_lock_path: Path,
    rollover_source_path: Path = ROLLOVER_SOURCE,
) -> int:
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
    try:
        source = json.loads(rollover_source_path.read_text(encoding="utf-8"))
        descriptor = json.loads(client_inventory_path.read_text(encoding="utf-8"))["nav_db"]
    except (OSError, ValueError, KeyError) as error:
        raise ContractInventoryError(f"cannot read rollover source contracts: {error}") from error
    if not isinstance(source, dict) or source.get("schema_version") != 1:
        raise ContractInventoryError("unsupported NAVDB rollover source schema")
    if source.get("nav_db_contract") != descriptor:
        raise ContractInventoryError(
            "NAVDB rollover logical source needs an explicit contract migration; "
            "update crates/nav-db-fixture/source.json, not historical FAA packages"
        )
    records = source.get("records")
    if not isinstance(records, dict):
        raise ContractInventoryError("NAVDB rollover source lacks logical records")
    for key, schema in descriptor["required_exact_keys"].items():
        record = records.get(key)
        if not isinstance(record, dict) or record.get("schema_version") != schema:
            raise ContractInventoryError(f"NAVDB rollover source lacks required {key} schema {schema}")
    return checked + 1


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Verify locked fixtures against the compiled client data contracts."
    )
    parser.add_argument("--client-inventory", type=Path, default=CLIENT_INVENTORY)
    parser.add_argument("--fixture-lock", type=Path, default=FIXTURE_LOCK)
    parser.add_argument("--rollover-source", type=Path, default=ROLLOVER_SOURCE)
    args = parser.parse_args()
    try:
        count = verify(args.client_inventory, args.fixture_lock, args.rollover_source)
    except (ContractInventoryError, LockError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(f"Client fixture contracts match ({count} published/source fixtures)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
