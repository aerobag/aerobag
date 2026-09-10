#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import sys
import json
import shutil
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch


TOOLS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_DIR))

import build_release  # noqa: E402


class ReleaseBuildTests(unittest.TestCase):
    def test_build_pins_telemetry_beside_binary_identities_and_reuses_immutable_release(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            shutil.copytree(build_release.REPO_ROOT / "contracts/telemetry", source / "contracts/telemetry")
            args = SimpleNamespace(
                repo_root=source, tag="test-release", commit="a" * 40,
                artifact_root=root / "artifacts", cargo_target_dir=root / "cargo",
                ui_target_root=root / "ui", public_origin="https://example.invalid",
            )

            def fake_build(command, *, cwd, env):
                web = Path(env["AEROBAG_WEB_DIST"])
                if command[0] == "cargo":
                    binary_root = args.cargo_target_dir / "release"
                    binary_root.mkdir(parents=True)
                    for binary in ["aerobag-live-feedsd", "preprocessor-cli"]:
                        (binary_root / binary).write_bytes(binary.encode())
                elif command == ["npm", "run", "build:release"]:
                    web.mkdir(parents=True)
                    Path(env["AEROBAG_UI_TARGET_ROOT"]).mkdir(parents=True)
                    (web / "index.html").write_text("web")
                    (web / "about.html").write_text(
                        build_release.ABOUT_DOWNLOAD_PANEL_BEGIN + build_release.ABOUT_DOWNLOAD_PANEL_END
                        + build_release.ABOUT_DOWNLOAD_SCRIPT_BEGIN + build_release.ABOUT_DOWNLOAD_SCRIPT_END
                    )
                elif command == ["./scripts/build_prod_apk.sh"]:
                    downloads = web / "downloads"
                    downloads.mkdir()
                    (downloads / "app.apk").write_bytes(b"apk")
                    (downloads / "android-apk.json").write_text(json.dumps({
                        "filename": "app.apk", "apk_url": "/app.apk", "apk_size_bytes": 3,
                        "git_commit": args.commit, "version_name": args.tag, "built_at_utc": "now",
                    }))

            with patch.object(build_release.subprocess, "run", return_value=SimpleNamespace(stdout=args.commit)), patch.object(build_release, "_run", side_effect=fake_build):
                built = build_release.build_release(args)
            metadata_bytes = (built / "release.json").read_bytes()
            metadata = build_release.validate_release_directory(built, args.tag, args.commit)
            self.assertEqual(metadata["telemetry_contracts"], build_release.telemetry_contracts.producer_pins(source / "contracts/telemetry"))
            self.assertIn("sha256", metadata["artifacts"]["preprocessor_binary"])
            with patch.object(build_release.subprocess, "run", return_value=SimpleNamespace(stdout=args.commit)), patch.object(build_release, "_run") as rebuild:
                self.assertEqual(build_release.build_release(args), built)
                rebuild.assert_not_called()
            self.assertEqual((built / "release.json").read_bytes(), metadata_bytes)

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
        self.assertEqual(values["AEROBAG_VERSION_NAME"], "2026-08-22.1")
        self.assertEqual(values["ANDROID_VERSION_NAME"], "2026-08-22.1")
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

    def test_release_permissions_are_normalized_for_static_serving(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "release"
            (root / "web/assets").mkdir(parents=True, mode=0o700)
            (root / "web/index.html").write_text("web", encoding="utf-8")
            (root / "web/index.html").chmod(0o600)
            (root / "bin").mkdir(mode=0o700)
            (root / "bin/daemon").write_bytes(b"binary")
            (root / "bin/daemon").chmod(0o600)
            root.chmod(0o700)

            build_release.normalize_release_permissions(root)
            build_release.validate_release_permissions(root)

            self.assertEqual(root.stat().st_mode & 0o777, 0o755)
            self.assertEqual((root / "web/index.html").stat().st_mode & 0o777, 0o644)
            self.assertEqual((root / "bin/daemon").stat().st_mode & 0o777, 0o755)

    def test_legacy_web_output_is_collected_without_using_the_served_tree(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            isolated_ui = root / "isolated-ui"
            legacy_output = isolated_ui / "web/dist"
            legacy_output.mkdir(parents=True)
            (legacy_output / "index.html").write_text("legacy", encoding="utf-8")
            (legacy_output / "about.html").write_text("about", encoding="utf-8")
            release_web = root / "release/web"

            build_release.collect_web_build_output(release_web, isolated_ui)

            self.assertEqual(
                (release_web / "index.html").read_text(encoding="utf-8"),
                "legacy",
            )
            self.assertEqual(
                (release_web / "about.html").read_text(encoding="utf-8"),
                "about",
            )
            self.assertFalse(legacy_output.exists())

    def test_release_about_page_embeds_apk_metadata_without_javascript(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            about = Path(temporary) / "about.html"
            about.write_text(
                "before"
                f"{build_release.ABOUT_DOWNLOAD_PANEL_BEGIN}dynamic panel"
                f"{build_release.ABOUT_DOWNLOAD_PANEL_END}middle"
                f"{build_release.ABOUT_DOWNLOAD_SCRIPT_BEGIN}<script>fetch()</script>"
                f"{build_release.ABOUT_DOWNLOAD_SCRIPT_END}after",
                encoding="utf-8",
            )

            build_release.finalize_static_about_page(
                about,
                {
                    "apk_url": "/downloads/aerobag.apk",
                    "filename": "aerobag.apk",
                    "apk_size_bytes": 31_000_000,
                    "git_commit": "0123456789abcdef",
                    "version_name": "2026-08-30.1<&",
                    "built_at_utc": "2026-08-30T17:00:00Z",
                },
            )

            rendered = about.read_text(encoding="utf-8")
            self.assertIn('href="/downloads/aerobag.apk"', rendered)
            self.assertIn("31 MB", rendered)
            self.assertIn("2026-08-30.1&lt;&amp;", rendered)
            self.assertNotIn("<script", rendered)
            self.assertNotIn("fetch()", rendered)
            self.assertNotIn("AEROBAG_ANDROID_DOWNLOAD", rendered)


if __name__ == "__main__":
    unittest.main()
