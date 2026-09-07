#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

import verify_locked_fixture_contracts


class VerifyLockedFixtureContractsTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        root = Path(self.temporary.name)
        self.inventory_path = root / "inventory.json"
        self.lock_path = root / "lock.json"
        self.contracts = {
            "publication": {
                "current_manifest_schema": 1,
                "bundle_manifest_schema": 2,
            },
            "package_contracts": {"nav-db": "NAV24"},
        }
        self.inventory_path.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "publication": self.contracts["publication"],
                    "package_contracts": {"nav-db": "NAV24"},
                    "live_feeds": {
                        "manifest_schema": 3,
                        "product_contracts": {"notams": 7},
                    },
                    "nav_db": {
                        "contract_id": "NAV24",
                        "page_encoding": "xz",
                        "required_exact_keys": {"airport/notam-catalog": 1},
                    },
                }
            ),
            encoding="utf-8",
        )
        self.write_lock()

    def write_lock(self, nav_contract: str = "NAV24") -> None:
        contracts = json.loads(json.dumps(self.contracts))
        contracts["package_contracts"]["nav-db"] = nav_contract
        fixtures = {
            name: {
                "path": name,
                "contract_version": 1,
                "client_contracts": contracts,
            }
            for name in verify_locked_fixture_contracts.REQUIRED_CLIENT_FIXTURES
        }
        self.lock_path.write_text(
            json.dumps(
                {
                    "schema_version": 2,
                    "repository": "https://example.invalid/fixtures.git",
                    "commit": "a" * 40,
                    "fixtures": fixtures,
                }
            ),
            encoding="utf-8",
        )

    def test_accepts_all_required_fixture_contracts(self) -> None:
        self.assertEqual(
            3,
            verify_locked_fixture_contracts.verify(
                self.inventory_path, self.lock_path
            ),
        )

    def test_rejects_incompatible_lock_without_fetching_fixtures(self) -> None:
        self.write_lock("NAV23")

        with self.assertRaisesRegex(ValueError, "client requires NAV24"):
            verify_locked_fixture_contracts.verify(
                self.inventory_path, self.lock_path
            )


if __name__ == "__main__":
    unittest.main()
