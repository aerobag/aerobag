#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

import verify_fixture_contracts


class VerifyFixtureContractsTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.repo = Path(self.temporary.name) / "repo"
        self.fixtures = Path(self.temporary.name) / "fixtures"
        contracts = {
            "publication": {
                "current_manifest_schema": 1,
                "bundle_manifest_schema": 2,
            },
            "package_contracts": {"nav-db": "NAV24"},
        }
        inventory = {
            "schema_version": 1,
            **contracts,
            "package_contracts": {"nav-db": "NAV24", "tpp": "TPP1"},
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
        inventory_path = self.repo / "crates/product-contracts/contracts"
        inventory_path.mkdir(parents=True)
        (inventory_path / "client-data-contracts.json").write_text(
            json.dumps(inventory), encoding="utf-8"
        )
        lock = {
            "schema_version": 2,
            "repository": "https://example.invalid/fixtures.git",
            "commit": "a" * 40,
            "fixtures": {
                "client-fixture": {
                    "path": "client-fixture",
                    "contract_version": 1,
                    "manifest": {
                        "path": "fixture.json",
                        "version_field": "schema_version",
                    },
                    "client_contracts": contracts,
                }
            },
        }
        (self.repo / "test-artifacts.lock.json").write_text(
            json.dumps(lock), encoding="utf-8"
        )
        fixture = self.fixtures / "client-fixture"
        fixture.mkdir(parents=True)
        (fixture / "fixture.json").write_text(
            json.dumps({"schema_version": 1, "client_contracts": contracts}),
            encoding="utf-8",
        )

    def test_accepts_exact_contract_match(self) -> None:
        self.assertEqual(
            1,
            verify_fixture_contracts.verify(
                self.repo, self.fixtures, ["client-fixture"]
            ),
        )

    def test_reports_client_incompatibility(self) -> None:
        lock_path = self.repo / "test-artifacts.lock.json"
        lock = json.loads(lock_path.read_text(encoding="utf-8"))
        lock["fixtures"]["client-fixture"]["client_contracts"][
            "package_contracts"
        ]["nav-db"] = "NAV23"
        lock_path.write_text(json.dumps(lock), encoding="utf-8")

        with self.assertRaisesRegex(
            verify_fixture_contracts.ContractError,
            "provides nav-db contract NAV23; client requires NAV24",
        ):
            verify_fixture_contracts.verify(
                self.repo, self.fixtures, ["client-fixture"]
            )

    def test_reports_manifest_lock_drift(self) -> None:
        manifest_path = self.fixtures / "client-fixture/fixture.json"
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        manifest["client_contracts"]["package_contracts"]["nav-db"] = "NAV23"
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

        with self.assertRaisesRegex(
            verify_fixture_contracts.ContractError,
            "manifest client contracts do not match the lock",
        ):
            verify_fixture_contracts.verify(
                self.repo, self.fixtures, ["client-fixture"]
            )


if __name__ == "__main__":
    unittest.main()
