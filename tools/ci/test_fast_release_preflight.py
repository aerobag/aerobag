# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import fast_release_preflight


class FastReleasePreflightTest(unittest.TestCase):
    def test_fast_gate_never_installs_a_browser(self) -> None:
        source = Path(fast_release_preflight.__file__).read_text()
        self.assertNotIn("prepare_test_browser(", source)
        self.assertNotIn("install_test_browser", source)

    def test_receipt_is_bound_to_exact_commit_and_success(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            receipt = Path(temp_dir) / "receipt.json"
            with mock.patch.object(
                fast_release_preflight,
                "receipt_path",
                return_value=receipt,
            ):
                receipt.write_text(
                    json.dumps(
                        {
                            "schema_version": 1,
                            "commit": "a" * 40,
                            "status": "passed",
                        }
                    ),
                    encoding="utf-8",
                )
                self.assertTrue(fast_release_preflight.valid_receipt("a" * 40))
                self.assertFalse(fast_release_preflight.valid_receipt("b" * 40))

    def test_cached_receipt_skips_all_preflight_work(self) -> None:
        with (
            mock.patch.object(
                fast_release_preflight.qualification,
                "assert_clean_commit",
                return_value="a" * 40,
            ),
            mock.patch.object(
                fast_release_preflight,
                "valid_receipt",
                return_value=True,
            ),
            mock.patch.object(
                fast_release_preflight.qualification,
                "prepare_environment",
            ) as prepare,
        ):
            self.assertEqual(fast_release_preflight.main(), 0)
        prepare.assert_not_called()

    def test_uncached_preflight_runs_only_ordinary_lanes(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            lane = fast_release_preflight.qualification.Lane("unit", ("true",))
            result = fast_release_preflight.qualification.LaneResult(
                "unit",
                0,
                0.1,
                Path(temp_dir) / "unit.log",
            )
            with (
                mock.patch.object(
                    fast_release_preflight.qualification,
                    "assert_clean_commit",
                    return_value="a" * 40,
                ),
                mock.patch.object(
                    fast_release_preflight,
                    "valid_receipt",
                    return_value=False,
                ),
                mock.patch.object(
                    fast_release_preflight.tempfile,
                    "gettempdir",
                    return_value=temp_dir,
                ),
                mock.patch.object(
                    fast_release_preflight.qualification,
                    "prepare_gradle_caches",
                ),
                mock.patch.object(
                    fast_release_preflight.qualification,
                    "prepare_environment",
                ),
                mock.patch.object(
                    fast_release_preflight.qualification,
                    "ordinary_lanes",
                    return_value=[lane],
                ),
                mock.patch.object(
                    fast_release_preflight.qualification,
                    "sequential_ci_lanes",
                    return_value=[],
                ),
                mock.patch.object(
                    fast_release_preflight.qualification,
                    "run_lanes",
                    return_value=[result],
                ) as run_lanes,
                mock.patch.object(
                    fast_release_preflight.qualification,
                    "git",
                    return_value="",
                ),
                mock.patch.object(
                    fast_release_preflight,
                    "write_receipt",
                    return_value=Path(temp_dir) / "receipt.json",
                ),
            ):
                self.assertEqual(fast_release_preflight.main(), 0)
        run_lanes.assert_called_once_with(
            [lane],
            Path(temp_dir) / "aerobag-fast-preflight-aaaaaaaaaaaa/logs",
            7,
        )


if __name__ == "__main__":
    unittest.main()
