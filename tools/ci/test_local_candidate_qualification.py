# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import json
import io
import socket
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest import mock


CI_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(CI_DIR))

import local_candidate_qualification as qualification  # noqa: E402
import fast_release_preflight as preflight  # noqa: E402
import diagnose_release_journey as diagnostic  # noqa: E402


class LocalCandidateQualificationTests(unittest.TestCase):
    def test_default_is_one_complete_pass_with_two_emulators(self) -> None:
        with mock.patch.object(sys, "argv", ["local_candidate_qualification.py"]):
            args = qualification.parse_args()
        self.assertEqual(args.repetitions, 1)
        self.assertEqual(args.android_workers, 2)
        self.assertEqual(qualification.PRIORITIES, ("p0", "p1", "p2"))
        self.assertEqual(qualification.ANDROID_SHARDS, 4)

    def test_repetition_and_worker_overrides_remain_explicit(self) -> None:
        with mock.patch.object(sys, "argv", [
            "local_candidate_qualification.py", "--repetitions", "5", "--android-workers", "4",
        ]):
            args = qualification.parse_args()
        self.assertEqual(args.repetitions, 5)
        self.assertEqual(args.android_workers, 4)

    def test_single_pass_runs_every_lane_and_preserves_isolated_gui_phases(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            ordinary = qualification.Lane("ci-example", ("true",))
            cached = qualification.LaneResult("ci-example", 0, 1, root / "ci.log")
            batches = []

            def record_lanes(lanes, logs, workers):
                lanes = list(lanes)
                batches.append((lanes, workers))
                return [qualification.LaneResult(lane.name, 0, 1, logs / lane.name) for lane in lanes]

            with (
                mock.patch.object(sys, "argv", ["local_candidate_qualification.py"]),
                mock.patch.object(qualification, "assert_clean_commit", return_value="a" * 40),
                mock.patch.object(qualification, "valid_receipt", return_value=None),
                mock.patch.object(qualification, "receipt_path", return_value=root / "receipt.json"),
                mock.patch.object(qualification, "require_qualification_capacity"),
                mock.patch.object(qualification, "create_run_root", return_value=root),
                mock.patch.object(qualification, "prepare_gradle_caches"),
                mock.patch.object(qualification, "prepare_environment"),
                mock.patch.object(qualification, "ordinary_lanes", return_value=[ordinary]),
                mock.patch.object(qualification, "sequential_ci_lanes", return_value=[]),
                mock.patch.object(qualification, "preflight_results", return_value=[cached]),
                mock.patch.object(qualification, "git", return_value=""),
                mock.patch.object(qualification, "prepare_inputs", return_value=(root, root / "fixture.json", root / "apps")),
                mock.patch.object(qualification, "available_loopback_ports", return_value=(21000, 21001, 21002, 21003, 21004)),
                mock.patch.object(qualification, "run_lanes", side_effect=record_lanes),
                mock.patch.object(qualification, "write_receipt") as receipt,
                redirect_stdout(io.StringIO()),
            ):
                self.assertEqual(qualification.main(), 0)

            self.assertEqual([workers for _, workers in batches], [1, 1, 1, 1, 1, 2, 2])
            self.assertEqual([lane.name for lanes, _ in batches for lane in lanes], [
                "e2e-web-p0", "e2e-web-p1", "e2e-web-p2",
                "e2e-web-nav-db-rollover", "e2e-android-baseline",
                "e2e-android-s0", "e2e-android-s1", "e2e-android-s2", "e2e-android-s3",
                *[f"e2e-{name}" for name in qualification.NATIVE_TESTS],
                "e2e-android-chrome-live-feed",
            ])
            for lanes, _ in batches:
                for lane in lanes:
                    if "AEROBAG_RELEASE_JOURNEY_REPETITIONS" in lane.env:
                        self.assertEqual(lane.env["AEROBAG_RELEASE_JOURNEY_REPETITIONS"], "1")
            self.assertEqual(receipt.call_args.args[-1], 1)
            self.assertEqual(receipt.call_args.args[-2][0], cached)

    def test_stability_evidence_is_separate_from_routine_prequalification(self) -> None:
        commit = "a" * 40
        with (
            tempfile.TemporaryDirectory() as temp_dir,
            mock.patch.object(qualification, "git", return_value=temp_dir),
            mock.patch.object(qualification, "workflow_identity", return_value={}),
        ):
            root = Path(temp_dir)
            apps = root / "apps"
            apps.mkdir()
            (apps / "build-manifest.json").write_text("{}", encoding="utf-8")
            stability = qualification.write_receipt(commit, "start", root, apps, [], 5)
            self.assertEqual(stability.parent.name, "stability")
            self.assertIsNone(qualification.valid_receipt(commit))
            self.assertIsNotNone(qualification.valid_receipt(commit, repetitions=5))
            self.assertIsNone(qualification.valid_receipt(commit, repetitions=3))

            routine = qualification.write_receipt(commit, "start", root, apps, [], 1)
            original = routine.read_bytes()
            self.assertNotEqual(stability, routine)
            self.assertIsNotNone(qualification.valid_receipt(commit))
            qualification.write_receipt(commit, "later", root, apps, [], 5)
            self.assertEqual(routine.read_bytes(), original)
            with (
                mock.patch.object(sys, "argv", ["local_candidate_qualification.py", "--check"]),
                mock.patch.object(qualification, "assert_clean_commit", return_value=commit),
                mock.patch.object(qualification, "prepare_inputs") as build,
            ):
                self.assertEqual(qualification.main(), 0)
            build.assert_not_called()

    def test_new_attempt_preserves_previous_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir, mock.patch.object(
            qualification.tempfile, "gettempdir", return_value=temp_dir,
        ):
            first = qualification.create_run_root("candidate", "a" * 40)
            (first / "failure.log").write_text("original failure", encoding="utf-8")
            second = qualification.create_run_root("candidate", "a" * 40)
            self.assertNotEqual(first, second)
            self.assertEqual((first / "failure.log").read_text(), "original failure")
            self.assertTrue(second.is_dir())

    def test_host_lane_lock_rejects_overlap_and_releases_after_failure(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir, mock.patch.object(
            qualification.tempfile, "gettempdir", return_value=temp_dir,
        ):
            with self.assertRaisesRegex(RuntimeError, "intentional"):
                with qualification.qualification_lock():
                    with self.assertRaisesRegex(qualification.QualificationError, "owns the host lanes"):
                        with qualification.qualification_lock():
                            self.fail("overlapping run acquired the host")
                    raise RuntimeError("intentional")
            with qualification.qualification_lock():
                pass

    def test_full_qualification_reuses_only_complete_exact_preflight_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            receipt = root / "receipt.json"
            names = (
                "ci-actionlint", "ci-reuse", "ci-rust-format", "ci-rust-shared",
                "ci-rust-core", "ci-rust-services", "ci-rust-preprocessor",
                "ci-python", "ci-web", "ci-android-jvm",
            )
            lanes = []
            for name in names:
                log = root / f"{name}.log"
                log.write_text("passed", encoding="utf-8")
                lanes.append({"name": name, "duration_seconds": 1, "log": str(log)})
            document = {"schema_version": 1, "commit": "a" * 40, "status": "passed", "lanes": lanes}
            with mock.patch.object(preflight, "receipt_path", return_value=receipt):
                receipt.write_text(json.dumps(document), encoding="utf-8")
                results = qualification.preflight_results("a" * 40, set(names))
                self.assertEqual(len(results), len(names))
                self.assertTrue(all(result.passed for result in results))
                self.assertIsNone(qualification.preflight_results("b" * 40, set(names)))
                self.assertIsNone(qualification.preflight_results("a" * 40, {*names, "ci-new-lane"}))
                for changes in ({"status": "failed"}, {"lanes": lanes[:-1]}, {"lanes": lanes[:-1] + [lanes[0]]}):
                    receipt.write_text(json.dumps({**document, **changes}), encoding="utf-8")
                    self.assertIsNone(qualification.preflight_results("a" * 40, set(names)))
                receipt.write_text(json.dumps(document), encoding="utf-8")
                Path(lanes[0]["log"]).unlink()
                self.assertIsNone(qualification.preflight_results("a" * 40, set(names)))

    def test_focused_diagnostics_reuse_retained_inputs_and_cannot_write_receipts(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir, mock.patch.object(
            qualification, "git", return_value="12345678",
        ):
            root = Path(temp_dir)
            source = root / "previous run"
            output = root / "diagnostic-unique"
            (source / "release-journey-materialized").mkdir(parents=True)
            (source / "release-journey-materialized/fixture.json").write_text("{}")
            (source / "release-ui-target/web/workspace/node_modules").mkdir(parents=True)
            (source / "android-release-journey-baseline.tar").touch()
            for platform in ("web", "android"):
                lane = diagnostic.diagnostic_lane(
                    source, output, "shared.inspector-details", platform, 20, "p1", platform == "web",
                )
                self.assertIn(f"{platform}-test shared.inspector-details", lane.command[4])
                self.assertNotIn("-suite", lane.command[4])
                self.assertEqual(lane.env["AEROBAG_RELEASE_JOURNEY_REPETITIONS"], "20")
                self.assertEqual(lane.env["AEROBAG_WEB_WORKSPACE_DIR"], str(source / "release-ui-target/web/workspace"))
                self.assertIn("fixture-stop", lane.command[4])
                if platform == "web":
                    self.assertEqual(lane.env["AEROBAG_E2E_RETAIN_NET_LOG"], "1")
                    self.assertNotIn("AEROBAG_CHROME_NET_LOG", lane.env,
                                     "peer browsers must not overwrite the main browser's trace")
                self.assert_shell_is_valid(lane.command)
            (source / "android-release-journey-baseline.tar").unlink()
            with self.assertRaisesRegex(qualification.QualificationError, "baseline is missing"):
                diagnostic.diagnostic_lane(source, output, "shared.inspector-details", "android", 1, "p1", False)
        self.assertNotIn("write_receipt", Path(diagnostic.__file__).read_text())

    def test_diagnostic_validates_journey_before_starting_devices(self) -> None:
        with mock.patch.object(diagnostic.subprocess, "check_output", return_value='{"priority":"p1"}') as query:
            self.assertEqual(diagnostic.journey_metadata("shared.inspector-details", "android")["priority"], "p1")
        self.assertEqual(query.call_args.args[0][-2:], ["shared.inspector-details", "android"])
        self.assertEqual(query.call_args.kwargs["timeout"], 15)
        with mock.patch.object(diagnostic.subprocess, "check_output", side_effect=subprocess.CalledProcessError(2, "node")):
            with self.assertRaises(qualification.QualificationError):
                diagnostic.journey_metadata("not-a-journey", "web")

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

    def test_receipt_requires_exact_commit_workflow_and_requested_pass_count(self) -> None:
        with (
            tempfile.TemporaryDirectory() as temp_dir,
            mock.patch.object(
                qualification,
                "receipt_path",
                return_value=Path(temp_dir) / "receipt.json",
            ) as receipt_path,
            mock.patch.object(qualification, "workflow_identity", return_value={}),
        ):
            valid = {
                "commit": "a" * 40,
                "status": "passed",
                "repetitions": 1,
                "workflow_identity": {},
            }
            for change in (
                {"repetitions": 0}, {"repetitions": 5}, {"repetitions": True},
                {"repetitions": "1"}, {"repetitions": 1.0},
                {"commit": "b" * 40}, {"status": "failed"},
                {"workflow_identity": {"changed": "workflow"}},
            ):
                with self.subTest(change=change):
                    receipt_path.return_value.write_text(json.dumps(valid | change), encoding="utf-8")
                    self.assertIsNone(qualification.valid_receipt("a" * 40))


if __name__ == "__main__":
    unittest.main()
