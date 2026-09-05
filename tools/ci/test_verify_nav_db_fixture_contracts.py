#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

import verify_nav_db_fixture_contracts


class VerifyNavDbFixtureContractsTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.repo = self.root / "repo"
        contract_source = self.repo / "crates/product-contracts/src/lib.rs"
        contract_source.parent.mkdir(parents=True)
        contract_source.write_text(
            'pub const NAV_DB_CONTRACT_ID: &str = "NAV15";\n',
            encoding="utf-8",
        )
        self.fixtures = self.root / "fixtures"
        android = self.fixtures / "e2e/android-smoke-publication/fixture.json"
        android.parent.mkdir(parents=True)
        android.write_text(
            json.dumps(
                {
                    "packages": [
                        {"family_id": "nav-db", "contract_id": "NAV15"},
                        {"family_id": "tpp", "contract_id": "TPP1"},
                    ]
                }
            ),
            encoding="utf-8",
        )
        advance = self.fixtures / "nav-db/advance-2608-to-2609/fixture.json"
        advance.parent.mkdir(parents=True)
        advance.write_text(
            json.dumps(
                {
                    "cycles": [
                        {"cycle": "2607", "contract_id": "NAV15"},
                        {"cycle": "2608", "contract_id": "NAV15"},
                    ]
                }
            ),
            encoding="utf-8",
        )
        release = (
            self.fixtures
            / "e2e/release-journey-publication/published/current_artifacts.json"
        )
        release.parent.mkdir(parents=True)
        release.write_text(
            json.dumps([{"contracts": {"nav-db": "NAV15"}}]),
            encoding="utf-8",
        )

    def test_accepts_exact_contract_match(self) -> None:
        result = verify_nav_db_fixture_contracts.verify(
            self.repo,
            self.fixtures,
            [
                "android-smoke-publication",
                "nav-db-advance",
                "release-journey-publication",
            ],
        )

        self.assertEqual("NAV15", result)

    def test_reports_fixture_and_required_contract(self) -> None:
        advance = self.fixtures / "nav-db/advance-2608-to-2609/fixture.json"
        manifest = json.loads(advance.read_text())
        manifest["cycles"][1]["contract_id"] = "NAV14"
        advance.write_text(json.dumps(manifest), encoding="utf-8")

        with self.assertRaisesRegex(
            verify_nav_db_fixture_contracts.ContractError,
            r"nav-db-advance provides NAVDB contract\(s\) \[NAV14, NAV15\]; "
            r"client requires NAV15",
        ):
            verify_nav_db_fixture_contracts.verify(
                self.repo, self.fixtures, ["nav-db-advance"]
            )

    def test_checks_release_journey_publication_contract(self) -> None:
        release = (
            self.fixtures
            / "e2e/release-journey-publication/published/current_artifacts.json"
        )
        release.write_text(
            json.dumps([{"contracts": {"nav-db": "NAV14"}}]),
            encoding="utf-8",
        )

        with self.assertRaisesRegex(
            verify_nav_db_fixture_contracts.ContractError,
            r"release-journey-publication provides NAVDB contract\(s\) \[NAV14\]; "
            r"client requires NAV15",
        ):
            verify_nav_db_fixture_contracts.verify(
                self.repo, self.fixtures, ["release-journey-publication"]
            )


if __name__ == "__main__":
    unittest.main()
