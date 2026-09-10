# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

from contextlib import redirect_stdout
import io
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import cheap_preflight
import check_generated_ui_sources


class CheapPreflightTests(unittest.TestCase):
    def test_all_inexpensive_ci_suites_are_unconditional_and_hermetic(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            lanes = {lane.name: lane for lane in cheap_preflight.cheap_lanes(root)}
            self.assertEqual(set(lanes), {
                "ci-actionlint", "ci-reuse", "ci-rust-format", "ci-harness-contracts",
                "ci-rust-shared", "ci-rust-core", "ci-rust-services", "ci-rust-preprocessor",
                "ci-python", "ci-generated-ui", "ci-web-unit", "ci-android-jvm", "ci-diff",
            })
            harness = lanes["ci-harness-contracts"].command
            self.assertEqual(harness, (
                str(cheap_preflight.ROOT / "ui/web-app/scripts/run-target-workspace.sh"),
                "inner:test:harness",
            ))
            package = json.loads((cheap_preflight.ROOT / "ui/web-app/package.json").read_text())
            self.assertEqual(package["scripts"][harness[-1]],
                             'node --test "${AEROBAG_REPO_ROOT:?missing repository root}"/tools/e2e/*.test.mjs')
            self.assertNotEqual(lanes["ci-harness-contracts"].env["AEROBAG_WEB_WORKSPACE_DIR"],
                                lanes["ci-web-unit"].env["AEROBAG_WEB_WORKSPACE_DIR"])
            release_harness = next(lane for lane in cheap_preflight.qualification.ordinary_lanes(root)
                                   if lane.name == "ci-harness-contracts")
            self.assertEqual(release_harness.command, harness)
            self.assertEqual(release_harness.env["AEROBAG_WEB_WORKSPACE_DIR"],
                             str(root / "harness-workspace"))
            workflow = (cheap_preflight.ROOT / ".github/workflows/ci.yml").read_text()
            self.assertIn("run: ./ui/web-app/scripts/run-target-workspace.sh inner:test:harness", workflow)
            self.assertIn("--workspace", lanes["ci-rust-core"].command[-1])
            self.assertNotIn("-E ", lanes["ci-rust-core"].command[-1])
            self.assertIn("testDebugUnitTest", lanes["ci-android-jvm"].command)
            self.assertEqual(lanes["ci-android-jvm"].env["ANDROID_BUILD_NATIVE_LIBRARIES"], "false")
            self.assertEqual(lanes["ci-web-unit"].command[-1], "inner:check")
            self.assertIn("product/preprocessor/preprocessor-tpp/scripts/test_detect_landscape_rotation.py",
                          lanes["ci-python"].command[-1])
            for lane in lanes.values():
                self.assertEqual(lane.env["AEROBAG_ARTIFACT_READ_PATH"], str(root / "no-artifacts"))
                self.assertNotIn("run-release-journey", " ".join(lane.command))
            self.assertEqual(list((root / "no-artifacts").iterdir()), [])

    def test_checks_sources_before_builds_and_does_not_require_a_clean_commit(self):
        batches = []
        def run(lanes, logs, jobs):
            batches.append([lane.name for lane in lanes])
            return []
        with (
            mock.patch.object(cheap_preflight.qualification, "run_lanes", side_effect=run),
            mock.patch.object(cheap_preflight.qualification, "assert_clean_commit") as clean,
            redirect_stdout(io.StringIO()),
        ):
            self.assertEqual(cheap_preflight.main([]), 0)
        clean.assert_not_called()
        self.assertEqual(batches[0], ["ci-generated-ui"])
        self.assertIn("ci-harness-contracts", batches[1])
        self.assertIn("ci-android-jvm", batches[1])

    def test_failed_suite_cannot_produce_success_or_be_retried(self):
        with (
            mock.patch.object(cheap_preflight.qualification, "run_lanes",
                              side_effect=cheap_preflight.qualification.QualificationError("uncovered find_route")) as run,
            redirect_stdout(io.StringIO()) as output,
        ):
            self.assertEqual(cheap_preflight.main([]), 1)
        run.assert_called_once()
        self.assertIn("uncovered find_route", output.getvalue())
        self.assertNotIn("Cheap preflight passed", output.getvalue())

    def test_source_edit_during_run_cannot_produce_success(self):
        with (
            mock.patch.object(cheap_preflight.qualification, "run_lanes", return_value=[]),
            mock.patch.object(cheap_preflight, "worktree_fingerprint", side_effect=["before", "after"]),
            redirect_stdout(io.StringIO()) as output,
        ):
            self.assertEqual(cheap_preflight.main([]), 1)
        self.assertIn("files changed during the run", output.getvalue())
        self.assertNotIn("Cheap preflight passed", output.getvalue())

    def test_timeout_kills_the_entire_lane_and_missing_tools_fail(self):
        qualification = cheap_preflight.qualification
        with tempfile.TemporaryDirectory() as directory:
            lane = qualification.Lane("bounded", ("build-with-children",), timeout_seconds=2)
            process = mock.MagicMock()
            process.pid = 1234
            process.wait.side_effect = [subprocess.TimeoutExpired(lane.command, 2), -9]
            with (
                mock.patch.object(qualification.subprocess, "Popen") as start,
                mock.patch.object(qualification.os, "killpg") as kill,
                redirect_stdout(io.StringIO()),
            ):
                start.return_value.__enter__.return_value = process
                result = qualification.run_lane(lane, Path(directory))
            self.assertFalse(result.passed)
            self.assertEqual(result.returncode, 124)
            self.assertTrue(start.call_args.kwargs["start_new_session"])
            kill.assert_called_once_with(1234, qualification.signal.SIGKILL)
            with (
                mock.patch.object(qualification.subprocess, "Popen", side_effect=FileNotFoundError("missing tool")),
                redirect_stdout(io.StringIO()),
            ):
                result = qualification.run_lane(lane, Path(directory))
            self.assertEqual(result.returncode, 127)
            self.assertIn("missing tool", result.log_path.read_text())

    def test_generated_comparison_detects_missing_and_stale_files_without_repair(self):
        with tempfile.TemporaryDirectory() as directory:
            generated, source = Path(directory) / "generated", Path(directory) / "source"
            generated.mkdir()
            source.mkdir()
            for name in ["unchanged", "stale", "missing"]:
                (generated / name).write_text("new")
            (source / "unchanged").write_text("new")
            (source / "stale").write_text("old")
            self.assertEqual(check_generated_ui_sources.compare(generated, source), [
                str(source / "missing"), str(source / "stale"),
            ])
            self.assertEqual((source / "stale").read_text(), "old")
            self.assertFalse((source / "missing").exists())


if __name__ == "__main__":
    unittest.main()
