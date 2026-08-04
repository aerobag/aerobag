#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import unittest
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from verify_acs_workload_report import verify_report


class VerifyAcsWorkloadReportTests(unittest.TestCase):
    def test_real_pipeline_health_classification_is_required(self) -> None:
        report = {
            "profile": "ci",
            "status_scenarios": [
                {
                    "name": "capacity",
                    "status": {
                        "server_time_epoch_ms": 1_785_000_000_000,
                        "mode": "normal",
                        "database_healthy": True,
                        "metrics": [
                            {
                                "id": "current_sse_connections",
                                "current": 8,
                                "peak": 8,
                                "warning_at": 4,
                                "critical_at": 7,
                                "hard_limit": 8,
                                "window_seconds": None,
                                "rejected_in_window": 0,
                                "lower_is_worse": False,
                            }
                        ],
                    },
                    "expected_pipeline_health": [
                        {
                            "metric_id": "aerobag_cloud.current_sse_connections",
                            "severity": "critical",
                        }
                    ],
                }
            ],
        }

        result = verify_report(report)

        self.assertTrue(result["scenarios"][0]["checks"][0]["passed"])

    def test_mismatched_expectation_fails(self) -> None:
        report = {
            "profile": "ci",
            "status_scenarios": [
                {
                    "name": "mode",
                    "status": {
                        "server_time_epoch_ms": 1_785_000_000_000,
                        "mode": "read_only",
                        "database_healthy": True,
                        "metrics": [],
                    },
                    "expected_pipeline_health": [
                        {"metric_id": "aerobag_cloud.mode", "severity": "ok"}
                    ],
                }
            ],
        }

        with self.assertRaisesRegex(RuntimeError, "expected ok, got warning"):
            verify_report(report)


if __name__ == "__main__":
    unittest.main()
