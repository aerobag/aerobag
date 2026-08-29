# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import json
import socket
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


CI_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(CI_DIR))

import local_candidate_qualification as qualification  # noqa: E402


class LocalCandidateQualificationTests(unittest.TestCase):
    def assert_shell_is_valid(self, command: tuple[str, ...]) -> None:
        self.assertEqual(command[:4], ("bash", "-euo", "pipefail", "-c"))
        subprocess.run(
            ["bash", "-n"],
            input=command[4],
            text=True,
            check=True,
        )

    def test_web_lane_uses_requested_repetition_count_and_cleans_up(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            lane = qualification.web_lane(
                "p1", root, root / "fixture.json", root / "apps", 9
            )

        self.assertEqual(lane.env["AEROBAG_RELEASE_JOURNEY_REPETITIONS"], "9")
        self.assertIn("fixture-stop", lane.command[4])
        self.assertIn("cloud-stop", lane.command[4])
        self.assert_shell_is_valid(lane.command)

    def test_android_shard_matches_one_fresh_github_matrix_lane(self) -> None:
        with (
            tempfile.TemporaryDirectory() as temp_dir,
            mock.patch.object(qualification, "git", return_value="12345678"),
        ):
            root = Path(temp_dir)
            lane = qualification.android_shard_lane(
                2, root, root / "fixture.json", root / "apps", 5
            )

        self.assertEqual(lane.env["AEROBAG_RELEASE_JOURNEY_REPETITIONS"], "5")
        self.assertEqual(lane.env["ANDROID_PACKAGE_SOURCE_DEVICE_PORT"], "18093")
        self.assertIn("android-suite-shard all 2 4", lane.command[4])
        self.assertNotIn("shared.startup-navigation", lane.command[4])
        self.assertIn("android-boot-install", lane.command[4])
        self.assertIn(
            'avdmanager delete avd --name "$AVD_INSTANCE_NAME"',
            lane.command[4],
        )
        self.assertEqual(
            lane.env["AEROBAG_ANDROID_BASELINE_ARCHIVE"],
            str(root / "android-release-journey-baseline.tar"),
        )
        self.assertEqual(lane.name, "e2e-android-s2")
        self.assertEqual(lane.env["PACKAGE_SOURCE_PORT"], "21202")
        self.assert_shell_is_valid(lane.command)

    def test_local_android_qualification_models_one_emulator_per_github_runner(self) -> None:
        self.assertEqual(qualification.ANDROID_SHARDS, 4)
        self.assertEqual(qualification.DEFAULT_ANDROID_WORKERS, 2)

    def test_native_android_lane_maps_the_immutable_apk_device_port(self) -> None:
        with (
            tempfile.TemporaryDirectory() as temp_dir,
            mock.patch.object(qualification, "git", return_value="12345678"),
        ):
            root = Path(temp_dir)
            lane = qualification.native_lane(
                "android.flight-plan-route-smoke",
                0,
                root,
                root / "fixtures",
                root / "apps",
                32123,
            )

        self.assertEqual(lane.env["PACKAGE_SOURCE_PORT"], "32123")
        self.assertEqual(
            lane.env["AEROBAG_ANDROID_PACKAGE_SOURCE_DEVICE_PORT"],
            "18093",
        )
        self.assertEqual(lane.env["ANDROID_PACKAGE_SOURCE_DEVICE_PORT"], "18093")
        self.assertEqual(lane.env["AEROBAG_ANDROID_CLOUD_DEVICE_PORT"], "18094")
        self.assertEqual(
            lane.env["AEROBAG_ANDROID_SMOKE_FIXTURE"],
            str(root / "fixtures/e2e/android-smoke-publication/fixture.json"),
        )

    def test_available_loopback_ports_are_distinct_and_bindable(self) -> None:
        ports = qualification.available_loopback_ports(5)

        self.assertEqual(len(set(ports)), 5)
        listeners = []
        try:
            for port in ports:
                listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                listener.bind(("127.0.0.1", port))
                listeners.append(listener)
        finally:
            for listener in listeners:
                listener.close()

    def test_local_qualification_rejects_insufficient_workspace_capacity(self) -> None:
        with mock.patch.object(
            qualification.shutil,
            "disk_usage",
            return_value=mock.Mock(free=1),
        ):
            with self.assertRaisesRegex(
                qualification.QualificationError,
                "requires at least 14 GiB free",
            ):
                qualification.require_qualification_capacity(Path("/tmp"))

    def test_local_qualification_reuses_ready_gradle_caches(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            wrapper_cache = root / "gradle/wrapper"
            dependency_cache = root / "gradle/caches"
            distribution, unpacked = qualification.gradle_wrapper_distribution()
            installed = wrapper_cache / "dists" / distribution / "cache-key"
            (installed / unpacked).mkdir(parents=True)
            (installed / f"{distribution}.zip.ok").touch()
            dependency_cache.mkdir(parents=True)
            (dependency_cache / "modules-2").mkdir()

            selected = qualification.prepare_gradle_caches(
                root / "candidate", wrapper_cache, dependency_cache
            )

            self.assertEqual(
                selected,
                (wrapper_cache.resolve(), dependency_cache.resolve()),
            )
            for target_root in ("ci-ui-target", "release-ui-target"):
                wrapper_link = (
                    root / "candidate" / target_root
                    / "android/gradle-user-home/wrapper"
                )
                dependency_link = wrapper_link.parent / "caches"
                self.assertTrue(wrapper_link.is_symlink())
                self.assertEqual(wrapper_link.resolve(), wrapper_cache.resolve())
                self.assertTrue(dependency_link.is_symlink())
                self.assertEqual(dependency_link.resolve(), dependency_cache.resolve())

    def test_local_qualification_rejects_an_incomplete_gradle_wrapper_cache(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            with self.assertRaisesRegex(
                qualification.QualificationError,
                "prime it once",
            ):
                qualification.prepare_gradle_caches(
                    root / "candidate", root / "empty-wrapper"
                )

    def test_local_qualification_rejects_an_empty_gradle_dependency_cache(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            wrapper_cache = root / "gradle/wrapper"
            distribution, unpacked = qualification.gradle_wrapper_distribution()
            installed = wrapper_cache / "dists" / distribution / "cache-key"
            (installed / unpacked).mkdir(parents=True)
            (installed / f"{distribution}.zip.ok").touch()

            with self.assertRaisesRegex(
                qualification.QualificationError,
                "dependency cache.*prime it once",
            ):
                qualification.prepare_gradle_caches(
                    root / "candidate", wrapper_cache, root / "empty-caches"
                )

    def test_local_qualification_selects_a_cache_with_the_pinned_commit(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            cache = root / "fixtures.git"
            subprocess.run(["git", "init", "--quiet", "--bare", str(cache)], check=True)
            with (
                mock.patch.dict(
                    qualification.os.environ,
                    {"AEROBAG_TEST_ARTIFACTS_REPOSITORY_CACHE": str(cache)},
                ),
                mock.patch.object(
                    qualification.subprocess,
                    "run",
                    return_value=subprocess.CompletedProcess([], 0),
                ) as run,
            ):
                selected = qualification.test_artifacts_repository_cache()

            self.assertEqual(selected, cache.resolve())
            self.assertIn("^{commit}", run.call_args.args[0][-1])

    def test_local_qualification_copies_binaryen_archive_into_isolated_target(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            source = root / "cached-binaryen.tar.gz"
            source.write_bytes(b"pinned archive")
            run_root = root / "candidate"

            with mock.patch.dict(
                qualification.os.environ,
                {"AEROBAG_BINARYEN_ARCHIVE_CACHE": str(source)},
            ):
                destination = qualification.prepare_binaryen_archive(run_root)

            self.assertEqual(
                destination,
                run_root / "release-ui-target/tools" / qualification.BINARYEN_ARCHIVE,
            )
            self.assertEqual(destination.read_bytes(), b"pinned archive")

    def test_local_qualification_requires_a_cached_binaryen_archive(self) -> None:
        with (
            tempfile.TemporaryDirectory() as temp_dir,
            mock.patch.dict(
                qualification.os.environ,
                {"AEROBAG_BINARYEN_ARCHIVE_CACHE": f"{temp_dir}/missing.tar.gz"},
            ),
        ):
            with self.assertRaisesRegex(
                qualification.QualificationError,
                "prime it once",
            ):
                qualification.prepare_binaryen_archive(Path(temp_dir) / "candidate")

    def test_android_baseline_qualifies_startup_once_before_shards(self) -> None:
        with (
            tempfile.TemporaryDirectory() as temp_dir,
            mock.patch.object(qualification, "git", return_value="12345678"),
        ):
            root = Path(temp_dir)
            lane = qualification.android_baseline_lane(
                root, root / "fixture.json", root / "apps"
            )

        self.assertIn("--test shared.startup-navigation", lane.command[4])
        self.assertIn("android-baseline-save", lane.command[4])
        self.assertEqual(lane.name, "e2e-android-baseline")
        self.assert_shell_is_valid(lane.command)

    def test_gui_heavy_github_jobs_are_separate_local_phases(self) -> None:
        with (
            tempfile.TemporaryDirectory() as temp_dir,
            mock.patch.object(qualification, "git", return_value="12345678"),
        ):
            root = Path(temp_dir)
            groups = qualification.initial_journey_lane_groups(
                root,
                root / "fixtures",
                root / "fixture.json",
                root / "apps",
                5,
            )

        self.assertEqual(
            [[lane.name for lane in lanes] for _, lanes in groups],
            [
                ["e2e-web-p0"],
                ["e2e-web-p1"],
                ["e2e-web-p2"],
                ["e2e-web-nav-db-rollover"],
                ["e2e-android-baseline"],
            ],
        )

    def test_local_receipt_is_bound_to_workflow_content(self) -> None:
        identity = qualification.workflow_identity()

        self.assertIn(".github/workflows/ci.yml", identity)
        self.assertIn(".github/workflows/e2e-ci.yml", identity)
        self.assertIn("tools/ci/local_candidate_qualification.py", identity)
        self.assertTrue(all(len(value) == 64 for value in identity.values()))

    def test_receipt_with_too_few_repetitions_cannot_authorize_staging(self) -> None:
        with (
            tempfile.TemporaryDirectory() as temp_dir,
            mock.patch.object(
                qualification,
                "receipt_path",
                return_value=Path(temp_dir) / "receipt.json",
            ) as receipt_path,
            mock.patch.object(qualification, "workflow_identity", return_value={}),
        ):
            receipt_path.return_value.write_text(
                json.dumps(
                    {
                        "commit": "a" * 40,
                        "status": "passed",
                        "repetitions": qualification.DEFAULT_REPETITIONS - 1,
                        "workflow_identity": {},
                    }
                ),
                encoding="utf-8",
            )
            self.assertIsNone(qualification.valid_receipt("a" * 40))


if __name__ == "__main__":
    unittest.main()
