#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import hashlib
import json
import tempfile
import unittest
import zipfile
from pathlib import Path

import build_e2e_package_fixture


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class BuildE2ePackageFixtureTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / "source"
        self.packaged = self.source / "source-v1" / "packaged"
        self.packaged.mkdir(parents=True)
        self.nav_path = self.packaged / "nav_db_NAV12_2607_01.zip"
        with zipfile.ZipFile(self.nav_path, "w") as archive:
            archive.writestr("root", b"root")
            archive.writestr("page_0000", b"page")
        self.tpp_path = self.packaged / "tpp_nw_TPP1_2607_01.zip"
        assets = [
            {
                "id": "plate:KPLU:one",
                "airport_id": "KPLU",
                "asset_path": "plates/PLU/one.png",
                "thumbnail_path": "thumbnails/plates/PLU/one.png",
            },
            {
                "id": "plate:KRNT:two",
                "airport_id": "KRNT",
                "asset_path": "plates/RNT/two.png",
                "thumbnail_path": "thumbnails/plates/RNT/two.png",
            },
        ]
        with zipfile.ZipFile(self.tpp_path, "w") as archive:
            archive.writestr("plates/PLU/one.png", b"plu")
            archive.writestr("thumbnails/plates/PLU/one.png", b"plu-thumb")
            archive.writestr("plates/RNT/two.png", b"rnt")
            archive.writestr("thumbnails/plates/RNT/two.png", b"rnt-thumb")
            archive.writestr(
                "package-assets.json",
                json.dumps(
                    {
                        "schema_version": 2,
                        "family_id": "tpp",
                        "package_id": "NW_TPP_TPP1_2607",
                        "assets": assets,
                    }
                ),
            )
            archive.writestr(
                "NW_TPP_TPP1_2607.manifest",
                "2607\nplates/PLU/one.png\nthumbnails/plates/PLU/one.png\n"
                "plates/RNT/two.png\nthumbnails/plates/RNT/two.png\n",
            )
        self.bundle_path = self.packaged / "bundle_cycle_2607.json"
        self.write_json(
            self.bundle_path,
            {
                "schema_version": 2,
                "bundle_id": "cycle_2607_01",
                "bundle_type": "cycle",
                "cycle": "2607",
                "cycle_version": "01",
                "packages": [
                    self.package("NAV_DB_2607", "nav-db", "NAV12", self.nav_path),
                    self.package(
                        "NW_TPP_TPP1_2607", "tpp", "TPP1", self.tpp_path, "nw"
                    ),
                    self.package(
                        "NW_SEC_SEC1_2607", "sec", "SEC1", self.tpp_path, "nw"
                    ),
                ],
            },
        )
        self.write_json(
            self.source / "current_artifacts.json",
            [
                {
                    "schema_version": 1,
                    "contracts": {
                        "nav-db": "NAV12",
                        "tpp": "TPP1",
                        "sec": "SEC1",
                    },
                    "artifact_roots": {
                        "packaged": "source-v1/packaged/",
                        "unpacked": "source-v1/unpacked/",
                    },
                    "as_of_date": "2026-07-25",
                    "as_of_utc": "2026-07-25T00:00:00Z",
                    "bundles": [
                        {
                            "filename": self.bundle_path.name,
                            "relative_path": self.bundle_path.name,
                            "id": "cycle_2607_01",
                            "bundle_type": "cycle",
                            "cycle": "2607",
                            "cycle_version": "01",
                            "start_valid": "2026-07-09",
                            "end_valid": "2026-08-06",
                        }
                    ],
                }
            ],
        )

    def write_json(self, path: Path, value: object) -> None:
        path.write_text(json.dumps(value), encoding="utf-8")

    def package(
        self,
        package_id: str,
        family: str,
        contract: str,
        path: Path,
        region: str | None = None,
    ) -> dict[str, object]:
        return {
            "id": package_id,
            "family_id": family,
            "contract_id": contract,
            "region_id": region,
            "filename": path.name,
            "relative_path": path.name,
            "checksum_sha256": digest(path),
            "size_bytes": path.stat().st_size,
            "effective_date": "2026-07-09",
            "expiration_date": "2026-08-06",
        }

    def test_builds_two_package_publication_with_only_kplu_plates(self) -> None:
        output = self.root / "fixture"

        build_e2e_package_fixture.build_fixture(self.source, output, "2607")

        current = json.loads(
            (output / "published" / "current_artifacts.json").read_text()
        )[0]
        packaged = output / "published" / current["artifact_roots"]["packaged"]
        bundle_ref = current["bundles"][0]
        bundle_path = packaged / bundle_ref["relative_path"]
        bundle = json.loads(bundle_path.read_text())
        self.assertEqual(["nav-db", "tpp"], [p["family_id"] for p in bundle["packages"]])
        self.assertEqual(digest(bundle_path), bundle_ref["checksum_sha256"])
        tpp = next(p for p in bundle["packages"] if p["family_id"] == "tpp")
        tpp_path = packaged / tpp["relative_path"]
        self.assertEqual(digest(tpp_path), tpp["checksum_sha256"])
        with zipfile.ZipFile(tpp_path) as archive:
            self.assertIn("plates/PLU/one.png", archive.namelist())
            self.assertNotIn("plates/RNT/two.png", archive.namelist())
            assets = json.loads(archive.read("package-assets.json"))
            self.assertEqual(["KPLU"], [asset["airport_id"] for asset in assets["assets"]])
        fixture = json.loads((output / "fixture.json").read_text())
        self.assertEqual(1, fixture["schema_version"])
        self.assertEqual(1, fixture["packages"][1]["plate_count"])

    def test_refuses_to_replace_existing_output(self) -> None:
        output = self.root / "fixture"
        output.mkdir()

        with self.assertRaisesRegex(
            build_e2e_package_fixture.BuildError, "output already exists"
        ):
            build_e2e_package_fixture.build_fixture(self.source, output, "2607")

    def test_rejects_package_contract_not_advertised_by_publication(self) -> None:
        current_path = self.source / "current_artifacts.json"
        current = json.loads(current_path.read_text())
        current[0]["contracts"]["nav-db"] = "NAV13"
        self.write_json(current_path, current)

        with self.assertRaisesRegex(
            build_e2e_package_fixture.BuildError,
            "nav-db package provides NAV12; current publication advertises NAV13",
        ):
            build_e2e_package_fixture.build_fixture(
                self.source, self.root / "fixture", "2607"
            )


if __name__ == "__main__":
    unittest.main()
