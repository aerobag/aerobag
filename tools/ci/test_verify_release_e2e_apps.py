# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
import zipfile


CI_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(CI_DIR))

import verify_release_e2e_apps as verifier  # noqa: E402


class ReleaseE2eAppVerificationTests(unittest.TestCase):
    def make_bundle(self, root: Path, protocol: str) -> None:
        app = root / "app.apk"
        app.write_bytes(b"app")
        driver = root / "driver.apk"
        with zipfile.ZipFile(driver, "w") as archive:
            archive.writestr("classes.dex", protocol.encode("utf-8"))
        cloud = root / "cloud"
        cloud.write_bytes(b"cloud")
        (root / "web-dist").mkdir()
        (root / "web-dist/index.html").write_text("app", encoding="utf-8")

        def record(path: Path) -> dict[str, object]:
            return {
                "path": path.name,
                "size_bytes": path.stat().st_size,
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            }

        driver_record = record(driver)
        driver_record["protocol"] = verifier.SEMANTIC_DRIVER_PROTOCOL
        (root / "build-manifest.json").write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "android_apk": record(app),
                    "android_e2e_driver_apk": driver_record,
                    "cloud_server": record(cloud),
                }
            ),
            encoding="utf-8",
        )

    def test_accepts_matching_driver_protocol(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.make_bundle(root, verifier.SEMANTIC_DRIVER_PROTOCOL)
            verifier.verify_bundle(root)

    def test_rejects_a_stale_driver_even_when_its_digest_matches(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.make_bundle(root, "aerobag-semantic-driver/1")
            with self.assertRaisesRegex(
                verifier.VerificationError,
                "does not implement aerobag-semantic-driver/3",
            ):
                verifier.verify_bundle(root)


if __name__ == "__main__":
    unittest.main()
