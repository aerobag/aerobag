#!/usr/bin/env python3
from __future__ import annotations

import json
import sys
import tempfile
import unittest
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import pipeline_health


class PipelineHealthTests(unittest.TestCase):
    def test_live_feed_staleness_uses_monitor_thresholds(self) -> None:
        now = datetime(2026, 6, 19, 12, 10, 0, tzinfo=timezone.utc)
        facts = {
            "inputs": {
                "current_artifacts": {"error": None, "payload": []},
                "deploy_health": {"error": None, "payload": {}},
                "live_feeds_status": {
                    "error": None,
                    "payload": {
                        "products": {
                            "metars": {
                                "last_source_timestamp_utc": "2026-06-19T12:00:00Z",
                                "consecutive_failure_count": 0,
                                "attempts": [
                                    {"result": "success"},
                                    {"result": "failure"},
                                    {"result": "failure"},
                                ],
                            }
                        }
                    },
                },
                "build_watch": {"error": None, "payload": {}},
                "faa_cycle_calendar": {"error": None, "payload": {"cycles": []}},
                "product_facts": [],
            }
        }

        evaluation = pipeline_health.evaluate_health(facts, [], now)

        stale = metric(evaluation, "live_feed.metars.stale_seconds")
        self.assertEqual(stale["value"], 600)
        self.assertEqual(stale["severity"], "warning")
        self.assertEqual(stale["warning_threshold"], 300)
        self.assertEqual(stale["critical_threshold"], 1800)
        failure_rate = metric(evaluation, "live_feed.metars.recent_failure_rate")
        self.assertEqual(failure_rate["value"], 0.666667)
        self.assertEqual(failure_rate["severity"], "critical")

    def test_product_facts_compare_against_previous_history(self) -> None:
        current_facts = {
            "inputs": {
                "current_artifacts": {"error": None, "payload": []},
                "deploy_health": {"error": None, "payload": {}},
                "live_feeds_status": {"error": None, "payload": {"products": {}}},
                "build_watch": {"error": None, "payload": {}},
                "faa_cycle_calendar": {"error": None, "payload": {"cycles": []}},
                "product_facts": [
                    {
                        "path": "/artifacts/current/product-facts.json",
                        "payload": {
                            "products": [
                                {
                                    "product_id": "NAV_DB_NAV10_2607_01",
                                    "family": "nav-db",
                                    "cycle": "2607",
                                    "error_count": 2,
                                    "warning_count": 1,
                                }
                            ]
                        }
                    }
                ],
            }
        }
        previous = [
            {
                "facts": {
                    "inputs": {
                        "product_facts": [
                            {
                                "path": "/artifacts/previous/product-facts.json",
                                "payload": {
                                    "products": [
                                        {
                                            "product_id": "NAV_DB_NAV10_2606_01",
                                            "family": "nav-db",
                                            "cycle": "2606",
                                            "error_count": 1,
                                            "warning_count": 1,
                                        }
                                    ]
                                }
                            }
                        ]
                    }
                }
            }
        ]

        evaluation = pipeline_health.evaluate_health(
            current_facts,
            previous,
            datetime(2026, 6, 19, 12, 0, 0, tzinfo=timezone.utc),
        )

        errors = metric(evaluation, "cycle_product.error_count")
        warnings = metric(evaluation, "cycle_product.warning_count")
        self.assertEqual(errors["severity"], "warning")
        self.assertIn("previous distinct publication: 1", errors["message"])
        self.assertEqual(warnings["severity"], "ok")

    def test_product_facts_compare_against_distinct_publication(self) -> None:
        current_facts = {
            "inputs": {
                "current_artifacts": {"error": None, "payload": []},
                "deploy_health": {"error": None, "payload": {}},
                "live_feeds_status": {"error": None, "payload": {"products": {}}},
                "build_watch": {"error": None, "payload": {}},
                "faa_cycle_calendar": {"error": None, "payload": {"cycles": []}},
                "product_facts": [
                    {
                        "path": "/artifacts/current/product-facts.json",
                        "payload": {
                            "products": [
                                {
                                    "product_id": "NAV_DB_NAV10_2607_01",
                                    "family": "nav-db",
                                    "cycle": "2607",
                                    "error_count": 2,
                                    "warning_count": 4,
                                }
                            ]
                        },
                    }
                ],
            }
        }
        previous = [
            {
                "facts": {
                    "inputs": {
                        "product_facts": [
                            {
                                "path": "/artifacts/baseline/product-facts.json",
                                "payload": {
                                    "products": [
                                        {
                                            "product_id": "NAV_DB_NAV10_2606_01",
                                            "family": "nav-db",
                                            "cycle": "2606",
                                            "error_count": 1,
                                            "warning_count": 4,
                                        }
                                    ]
                                },
                            }
                        ]
                    }
                }
            },
            {"facts": current_facts},
        ]

        evaluation = pipeline_health.evaluate_health(
            current_facts,
            previous,
            datetime(2026, 6, 19, 12, 0, 0, tzinfo=timezone.utc),
        )

        errors = metric(evaluation, "cycle_product.error_count")
        self.assertEqual(errors["severity"], "warning")
        self.assertIn("previous distinct publication: 1", errors["message"])

    def test_current_response_reports_sample_age(self) -> None:
        record = {"sampled_at_utc": "2026-06-19T12:00:00Z"}

        age = pipeline_health.sample_age_seconds(
            record,
            datetime(2026, 6, 19, 12, 0, 3, tzinfo=timezone.utc),
        )

        self.assertEqual(age, 3)

    def test_calendar_alerts_when_expected_cycle_is_missing(self) -> None:
        facts = {
            "inputs": {
                "current_artifacts": {"error": None, "payload": []},
                "deploy_health": {"error": None, "payload": {}},
                "live_feeds_status": {"error": None, "payload": {"products": {}}},
                "build_watch": {"error": None, "payload": {}},
                "faa_cycle_calendar": {
                    "error": None,
                    "payload": {
                        "cycles": [
                            {"cycle": "2607", "effective_date": "2026-07-09"}
                        ]
                    },
                },
                "product_facts": [],
            }
        }
        now = datetime(2026, 6, 23, 12, 0, 0, tzinfo=timezone.utc)

        evaluation = pipeline_health.evaluate_health(facts, [], now)

        missing = metric(evaluation, "cycle_calendar.2607.missing_seconds")
        self.assertEqual(missing["severity"], "critical")
        self.assertEqual(evaluation["top_line_status"], "critical")

    def test_collect_product_facts_resolves_packaged_roots(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            artifact_root = Path(temp_dir)
            packaged = (
                artifact_root
                / "published"
                / "master"
                / "20260619T120000Z"
                / "packaged"
            )
            packaged.mkdir(parents=True)
            (packaged / "product-facts.json").write_text(
                json.dumps({"schema_version": 1, "products": []}),
                encoding="utf-8",
            )
            current = [
                {
                    "artifact_roots": {
                        "packaged": "master/20260619T120000Z/packaged/"
                    }
                }
            ]

            facts = pipeline_health.collect_product_facts(artifact_root, current)

            self.assertIsNone(facts[0]["error"])
            self.assertEqual(facts[0]["payload"]["schema_version"], 1)


def metric(evaluation: dict, metric_id: str) -> dict:
    for item in evaluation["metrics"]:
        if item["id"] == metric_id:
            return item
    raise AssertionError(f"missing metric {metric_id}")


if __name__ == "__main__":
    unittest.main()
