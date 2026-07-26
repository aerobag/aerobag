#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import hashlib
import json
import tempfile
import unittest
from pathlib import Path

import build_nav_db_advance_fixture


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class BuildNavDbAdvanceFixtureTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / "publication"
        self.packaged = self.source / "build-1" / "packaged"
        self.packaged.mkdir(parents=True)
        self.bundle_refs = []
        for cycle, start, end in [
            ("2607", "2026-07-09", "2026-08-06"),
            ("2608", "2026-08-06", "2026-09-03"),
        ]:
            nav_path = self.packaged / f"nav_db_NAV15_{cycle}_01.zip"
            nav_path.write_bytes(f"nav-{cycle}".encode())
            bundle_path = self.packaged / f"bundle_cycle_{cycle}_01.json"
            self.write_json(
                bundle_path,
                {
                    "schema_version": 2,
                    "packages": [
                        {
                            "id": f"NAV_DB_NAV15_{cycle}_01",
                            "family_id": "nav-db",
                            "contract_id": "NAV15",
                            "filename": nav_path.name,
                            "relative_path": nav_path.name,
                            "checksum_sha256": digest(nav_path),
                            "size_bytes": nav_path.stat().st_size,
                        }
                    ],
                },
            )
            self.bundle_refs.append(
                {
                    "filename": bundle_path.name,
                    "relative_path": bundle_path.name,
                    "id": f"cycle_{cycle}_01",
                    "bundle_type": "cycle",
                    "cycle": cycle,
                    "cycle_version": "01",
                    "start_valid": start,
                    "end_valid": end,
                }
            )
        self.write_json(
            self.source / "current_artifacts.json",
            [
                {
                    "schema_version": 1,
                    "contracts": {"nav-db": "NAV15"},
                    "artifact_roots": {
                        "packaged": "build-1/packaged/",
                        "unpacked": "build-1/unpacked/",
                    },
                    "bundles": self.bundle_refs,
                }
            ],
        )

    def write_json(self, path: Path, value: object) -> None:
        path.write_text(json.dumps(value), encoding="utf-8")

    def test_builds_hashed_two_cycle_fixture(self) -> None:
        output = self.root / "fixture"

        build_nav_db_advance_fixture.build_fixture(
            self.source, output, ["2607", "2608"]
        )

        fixture = json.loads((output / "fixture.json").read_text())
        self.assertEqual(["2607", "2608"], [entry["cycle"] for entry in fixture["cycles"]])
        self.assertEqual(
            ["NAV15", "NAV15"],
            [entry["contract_id"] for entry in fixture["cycles"]],
        )
        for cycle in fixture["cycles"]:
            for artifact_name in ["bundle", "nav_db"]:
                artifact = cycle[artifact_name]
                path = output / artifact["filename"]
                self.assertTrue(path.is_file())
                self.assertEqual(digest(path), artifact["sha256"])
                self.assertEqual(path.stat().st_size, artifact["size_bytes"])

    def test_rejects_contract_disagreement(self) -> None:
        current_path = self.source / "current_artifacts.json"
        current = json.loads(current_path.read_text())
        current[0]["contracts"]["nav-db"] = "NAV14"
        self.write_json(current_path, current)

        with self.assertRaisesRegex(
            build_nav_db_advance_fixture.BuildError,
            "provides NAV15; current publication advertises NAV14",
        ):
            build_nav_db_advance_fixture.build_fixture(
                self.source, self.root / "fixture", ["2607", "2608"]
            )


if __name__ == "__main__":
    unittest.main()
