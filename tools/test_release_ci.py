#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import os
import sys
import unittest
import urllib.error
from unittest import mock


TOOLS_DIR = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS_DIR)

import release_ci  # noqa: E402


COMMIT = "a" * 40
TAG = "2026-08-24.1"


def run(
    *,
    workflow: str,
    branch: str,
    status: str = "completed",
    conclusion: str | None = "success",
    run_id: int = 1,
) -> dict:
    return {
        "id": run_id,
        "head_sha": COMMIT,
        "head_branch": branch,
        "display_title": (
            f"Release qualification {TAG}" if workflow == "e2e-ci.yml" else "CI"
        ),
        "status": status,
        "conclusion": conclusion,
        "created_at": f"2026-08-24T00:00:{run_id:02d}Z",
        "html_url": f"https://example.test/runs/{run_id}",
    }


class ReleaseCiTests(unittest.TestCase):
    def test_expired_installation_token_is_refreshed_through_helper(self) -> None:
        def expired(_request: object, timeout: int) -> object:
            self.assertEqual(timeout, 20)
            raise urllib.error.HTTPError(
                "https://api.github.test/runs",
                401,
                "expired",
                hdrs=None,
                fp=None,
            )

        with (
            mock.patch.dict(
                os.environ,
                {
                    "GITHUB_TOKEN": "expired-token",
                    "AEROBAG_GITHUB_TOKEN_HELPER": "/credentials/with-token",
                },
                clear=True,
            ),
            mock.patch.object(
                release_ci,
                "_github_json_via_token_helper",
                return_value={"workflow_runs": []},
            ) as refresh,
        ):
            runs = release_ci.github_workflow_runs(
                "aerobag/aerobag",
                "ci.yml",
                COMMIT,
                opener=expired,
            )

        self.assertEqual(runs, [])
        refresh.assert_called_once()

    def test_candidate_requires_full_repeated_journey_run_for_exact_commit(self) -> None:
        candidate = run(workflow="e2e-ci.yml", branch="candidate-main", run_id=7)
        candidate["display_title"] = f"Candidate qualification {COMMIT}"
        status = release_ci.evaluate_candidate_qualification(
            commit=COMMIT,
            ci_runs=[run(workflow="ci.yml", branch="main")],
            e2e_runs=[candidate],
        )

        self.assertTrue(status.passed)
        self.assertEqual(status.release_journeys.run_id, 7)

    def test_ordinary_main_e2e_cannot_substitute_for_candidate_run(self) -> None:
        ordinary = run(workflow="e2e-ci.yml", branch="main")
        ordinary["display_title"] = "E2E main"
        status = release_ci.evaluate_candidate_qualification(
            commit=COMMIT,
            ci_runs=[run(workflow="ci.yml", branch="main")],
            e2e_runs=[ordinary],
        )

        self.assertFalse(status.passed)
        self.assertEqual(status.release_journeys.state, "missing")

    def test_requires_successful_main_and_exact_release_tag_runs(self) -> None:
        status = release_ci.evaluate_release_qualification(
            tag=TAG,
            commit=COMMIT,
            ci_runs=[run(workflow="ci.yml", branch="main")],
            e2e_runs=[run(workflow="e2e-ci.yml", branch=TAG)],
        )

        self.assertTrue(status.passed)
        self.assertEqual(status.failure_summary(), "passed")

    def test_ordinary_p0_run_cannot_substitute_for_full_tag_run(self) -> None:
        ordinary_e2e = run(workflow="e2e-ci.yml", branch="main")
        ordinary_e2e["display_title"] = "E2E main"
        status = release_ci.evaluate_release_qualification(
            tag=TAG,
            commit=COMMIT,
            ci_runs=[run(workflow="ci.yml", branch="main")],
            e2e_runs=[ordinary_e2e],
        )

        self.assertFalse(status.passed)
        self.assertEqual(status.release_journeys.state, "missing")

    def test_pending_and_failed_runs_report_distinct_states(self) -> None:
        status = release_ci.evaluate_release_qualification(
            tag=TAG,
            commit=COMMIT,
            ci_runs=[
                run(
                    workflow="ci.yml",
                    branch="main",
                    status="in_progress",
                    conclusion=None,
                )
            ],
            e2e_runs=[
                run(
                    workflow="e2e-ci.yml",
                    branch=TAG,
                    conclusion="failure",
                )
            ],
        )

        self.assertEqual(status.ordinary_ci.state, "pending")
        self.assertEqual(status.release_journeys.state, "failed")
        self.assertIn("in_progress", status.failure_summary())
        self.assertIn("failure", status.failure_summary())

    def test_newest_exact_run_wins_after_a_rerun(self) -> None:
        failed = run(workflow="e2e-ci.yml", branch=TAG, conclusion="failure", run_id=1)
        passed = run(workflow="e2e-ci.yml", branch=TAG, run_id=2)
        status = release_ci.evaluate_release_qualification(
            tag=TAG,
            commit=COMMIT,
            ci_runs=[run(workflow="ci.yml", branch="main")],
            e2e_runs=[failed, passed],
        )

        self.assertTrue(status.passed)
        self.assertEqual(status.release_journeys.url, "https://example.test/runs/2")


if __name__ == "__main__":
    unittest.main()
