#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path


TOOLS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_DIR))

import build_release  # noqa: E402


class ReleaseBuildTests(unittest.TestCase):
    def test_release_directory_is_immutable_and_commit_disambiguated(self) -> None:
        self.assertEqual(
            build_release.release_directory(
                Path("/artifacts"), "2026-08-22.1", "a" * 40
            ),
            Path("/artifacts/release-builds/2026-08-22.1-aaaaaaaaaaaa"),
        )

    def test_web_and_apk_are_pinned_to_release_scoped_resources(self) -> None:
        values = build_release.release_environment(
            "2026-08-22.1",
            public_origin="https://aerobag.org/",
            web_dist=Path("/release/web"),
            ui_target_root=Path("/ui-target"),
        )
        release = "https://aerobag.org/releases/2026-08-22.1"
        self.assertEqual(
            values["AEROBAG_PACKAGE_SOURCE_BASE_URL"], f"{release}/packages/"
        )
        self.assertEqual(
            values["ANDROID_PACKAGE_SOURCE_BASE_URL"], f"{release}/packages/"
        )
        self.assertEqual(values["AEROBAG_LIVE_FEEDS_ORIGIN"], release)
        self.assertEqual(values["ANDROID_LIVE_FEED_SOURCE_BASE_URL"], release)
        self.assertEqual(
            values["AEROBAG_WEB_PUBLIC_BASE_URL"],
            "/releases/2026-08-22.1/web/",
        )

    def test_directory_identity_includes_names_and_contents(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "a").mkdir()
            (root / "a/asset.js").write_text("one", encoding="utf-8")
            first = build_release.directory_sha256(root)
            (root / "a/asset.js").write_text("two", encoding="utf-8")
            self.assertNotEqual(first, build_release.directory_sha256(root))

    def test_legacy_web_output_is_collected_without_using_the_served_tree(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            isolated_ui = root / "isolated-ui"
            legacy_output = isolated_ui / "web/dist"
            legacy_output.mkdir(parents=True)
            (legacy_output / "index.html").write_text("legacy", encoding="utf-8")
            release_web = root / "release/web"

            build_release.collect_web_build_output(release_web, isolated_ui)

            self.assertEqual(
                (release_web / "index.html").read_text(encoding="utf-8"),
                "legacy",
            )
            self.assertFalse(legacy_output.exists())


if __name__ == "__main__":
    unittest.main()
