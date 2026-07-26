#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from datetime import date, datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import pipeline_health


class PipelineHealthTests(unittest.TestCase):
    def test_dashboard_disposes_plots_and_serializes_refreshes(self) -> None:
        html = pipeline_health.dashboard_html()

        self.assertIn("Plotly.purge(plot)", html)
        self.assertIn("setTimeout(refreshLoop, 30000)", html)
        self.assertNotIn("setInterval(", html)

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
        self.assertEqual(stale["warning_threshold"], 420)
        self.assertEqual(stale["critical_threshold"], 1800)
        failure_rate = metric(evaluation, "live_feed.metars.recent_failure_rate")
        self.assertEqual(failure_rate["value"], 0.666667)
        self.assertEqual(failure_rate["severity"], "critical")

    def test_nexrad_staleness_allows_five_minute_fetch_interval(self) -> None:
        now = datetime(2026, 6, 19, 12, 10, 0, tzinfo=timezone.utc)
        facts = {
            "inputs": {
                "current_artifacts": {"error": None, "payload": []},
                "deploy_health": {"error": None, "payload": {}},
                "live_feeds_status": {
                    "error": None,
                    "payload": {
                        "products": {
                            "nexrad": {
                                "last_source_timestamp_utc": "2026-06-19T12:00:00Z",
                                "consecutive_failure_count": 0,
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

        stale = metric(evaluation, "live_feed.nexrad.stale_seconds")
        self.assertEqual(stale["value"], 600)
        self.assertEqual(stale["severity"], "ok")
        self.assertEqual(stale["warning_threshold"], 700)
        self.assertEqual(stale["critical_threshold"], 900)

    def test_live_feed_failure_rate_exposes_failure_details(self) -> None:
        now = datetime(2026, 6, 19, 12, 0, 0, tzinfo=timezone.utc)
        facts = {
            "inputs": {
                "current_artifacts": {"error": None, "payload": []},
                "deploy_health": {"error": None, "payload": {}},
                "live_feeds_status": {
                    "error": None,
                    "payload": {
                        "products": {
                            "metars": {
                                "nominal_interval_seconds": 300,
                                "last_source_timestamp_utc": "2026-06-19T11:59:00Z",
                                "last_failure_at_utc": "2026-06-19T11:00:00Z",
                                "last_failure_phase": "build",
                                "last_error": "gzip failed",
                                "consecutive_failure_count": 0,
                                "attempts": [
                                    {
                                        "attempted_at_utc": "2026-06-19T11:00:00Z",
                                        "result": "failure",
                                        "phase": "build",
                                        "error": "gzip failed",
                                    },
                                    {"result": "success"},
                                    {"result": "success"},
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

        failure_rate = metric(evaluation, "live_feed.metars.recent_failure_rate")
        self.assertEqual(failure_rate["value"], 0.333333)
        self.assertEqual(failure_rate["severity"], "warning")
        self.assertEqual(failure_rate["details"]["last_error"], "gzip failed")
        self.assertEqual(
            failure_rate["details"]["failures"][0]["attempted_at_utc"],
            "2026-06-19T11:00:00Z",
        )

    def test_notams_without_a_successful_sample_are_critical(self) -> None:
        now = datetime(2026, 7, 17, 20, 0, 0, tzinfo=timezone.utc)
        facts = {
            "inputs": {
                "current_artifacts": {"error": None, "payload": []},
                "deploy_health": {"error": None, "payload": {}},
                "live_feeds_status": {
                    "error": None,
                    "payload": {
                        "products": {
                            "notams": {
                                "last_source_timestamp_utc": None,
                                "last_success_at_utc": None,
                                "last_failure_at_utc": "2026-07-17T16:30:53Z",
                                "last_failure_phase": "poll",
                                "last_error": "unsupported NOTAM type R",
                                "consecutive_failure_count": 1,
                                "quality": {
                                    "rejected_row_count": 1,
                                    "oldest_rejected_ingest_seq": 6922,
                                    "latest_rejected_ingest_seq": 6922,
                                    "last_rejection_error": "unsupported NOTAM type R",
                                    "recent_rejections": [
                                        {
                                            "ingest_seq": 6922,
                                            "first_rejected_at_utc": "2026-07-17T16:30:53Z",
                                            "last_rejected_at_utc": "2026-07-17T16:30:53Z",
                                            "rejection_count": 1,
                                            "error": "unsupported NOTAM type R",
                                        }
                                    ],
                                },
                                "attempts": [
                                    {
                                        "attempted_at_utc": "2026-07-17T16:30:53Z",
                                        "result": "failure",
                                        "phase": "poll",
                                        "error": "unsupported NOTAM type R",
                                    }
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

        stale = metric(evaluation, "live_feed.notams.stale_seconds")
        self.assertEqual(stale["severity"], "critical")
        self.assertEqual(stale["warning_threshold"], 5 * 60)
        self.assertEqual(stale["critical_threshold"], 15 * 60)
        failures = metric(evaluation, "live_feed.notams.consecutive_failures")
        self.assertEqual(failures["severity"], "warning")
        failure_rate = metric(evaluation, "live_feed.notams.recent_failure_rate")
        self.assertEqual(failure_rate["details"]["last_error"], "unsupported NOTAM type R")
        rejected = metric(evaluation, "live_feed.notams.rejected_row_count")
        self.assertEqual(rejected["value"], 1)
        self.assertEqual(rejected["severity"], "warning")
        self.assertEqual(rejected["details"]["oldest_rejected_ingest_seq"], 6922)
        self.assertEqual(
            rejected["details"]["last_rejection_error"],
            "unsupported NOTAM type R",
        )
        self.assertEqual(
            rejected["details"]["recent_rejections"][0]["ingest_seq"], 6922
        )
        self.assertEqual(evaluation["top_line_status"], "critical")

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

    def test_product_facts_uses_max_per_cycle_for_overlapping_publications(self) -> None:
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
                                    "product_id": "NAV_DB_NAV12_2606_01",
                                    "family": "nav-db",
                                    "cycle": "2606",
                                    "error_count": 0,
                                    "warning_count": 145,
                                },
                                {
                                    "product_id": "NAV_DB_NAV12_2607_01",
                                    "family": "nav-db",
                                    "cycle": "2607",
                                    "error_count": 0,
                                    "warning_count": 146,
                                },
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
                                "path": "/artifacts/previous/product-facts.json",
                                "payload": {
                                    "products": [
                                        {
                                            "product_id": "NAV_DB_NAV12_2606_01",
                                            "family": "nav-db",
                                            "cycle": "2606",
                                            "error_count": 0,
                                            "warning_count": 145,
                                        }
                                    ]
                                },
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

        warnings = metric(evaluation, "cycle_product.warning_count")
        self.assertEqual(warnings["value"], 146)
        self.assertEqual(warnings["severity"], "warning")
        self.assertIn("2606: 145, 2607: 146", warnings["message"])
        self.assertIn("previous distinct publication: 145", warnings["message"])

    def test_current_response_reports_sample_age(self) -> None:
        record = {"sampled_at_utc": "2026-06-19T12:00:00Z"}

        age = pipeline_health.sample_age_seconds(
            record,
            datetime(2026, 6, 19, 12, 0, 3, tzinfo=timezone.utc),
        )

        self.assertEqual(age, 3)

    def test_calendar_warns_when_unpublished_cycle_is_inside_publication_window(self) -> None:
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
        now = datetime(2026, 6, 20, 0, 0, 0, tzinfo=timezone.utc)

        evaluation = pipeline_health.evaluate_health(facts, [], now)

        countdown = metric(evaluation, "cycle_calendar.2607.seconds_until_effective")
        self.assertEqual(countdown["value"], 19 * 24 * 60 * 60)
        self.assertEqual(countdown["severity"], "warning")
        self.assertIn("effective in 19d", countdown["message"])

    def test_calendar_hides_unpublished_cycle_before_publication_window(self) -> None:
        facts = calendar_facts([{"cycle": "2607", "effective_date": "2026-07-09"}])
        now = datetime(2026, 6, 18, 12, 0, 0, tzinfo=timezone.utc)

        evaluation = pipeline_health.evaluate_health(facts, [], now)

        self.assertFalse(
            has_metric(evaluation, "cycle_calendar.2607.seconds_until_effective")
        )

    def test_calendar_marks_unpublished_cycle_critical_inside_final_window(self) -> None:
        facts = calendar_facts([{"cycle": "2607", "effective_date": "2026-07-09"}])
        now = datetime(2026, 6, 24, 0, 0, 0, tzinfo=timezone.utc)

        evaluation = pipeline_health.evaluate_health(facts, [], now)

        countdown = metric(evaluation, "cycle_calendar.2607.seconds_until_effective")
        self.assertEqual(countdown["value"], 15 * 24 * 60 * 60)
        self.assertEqual(countdown["severity"], "critical")
        self.assertEqual(evaluation["top_line_status"], "critical")

    def test_calendar_published_cycle_clears_countdown(self) -> None:
        facts = calendar_facts(
            [{"cycle": "2607", "effective_date": "2026-07-09"}],
            product_facts=[
                {
                    "payload": {
                        "products": [
                            {
                                "product_id": "NAV_DB_NAV12_2607_01",
                                "cycle": "2607",
                            }
                        ]
                    }
                }
            ],
        )
        now = datetime(2026, 6, 24, 0, 0, 0, tzinfo=timezone.utc)

        evaluation = pipeline_health.evaluate_health(facts, [], now)

        countdown = metric(evaluation, "cycle_calendar.2607.seconds_until_effective")
        self.assertEqual(countdown["value"], 0)
        self.assertEqual(countdown["severity"], "ok")

    def test_calendar_hides_obsolete_cycle_when_newer_cycle_is_published(self) -> None:
        facts = calendar_facts(
            [
                {"cycle": "2606", "effective_date": "2026-06-11"},
                {"cycle": "2607", "effective_date": "2026-07-09"},
            ],
            product_facts=[
                {
                    "payload": {
                        "products": [
                            {
                                "product_id": "NAV_DB_NAV12_2607_01",
                                "cycle": "2607",
                            }
                        ]
                    }
                }
            ],
        )
        now = datetime(2026, 7, 11, 0, 0, 0, tzinfo=timezone.utc)

        evaluation = pipeline_health.evaluate_health(facts, [], now)

        self.assertFalse(
            has_metric(evaluation, "cycle_calendar.2606.seconds_until_effective")
        )
        countdown = metric(evaluation, "cycle_calendar.2607.seconds_until_effective")
        self.assertEqual(countdown["value"], 0)
        self.assertEqual(countdown["severity"], "ok")

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

    def test_compact_history_record_omits_raw_input_payloads(self) -> None:
        facts = {
            "sampled_at_utc": "2026-06-19T12:00:00Z",
            "inputs": {
                "product_facts": [],
                "current_artifacts": {"payload": {"large": "x" * 100_000}},
                "live_feeds_status": {"payload": {"large": "y" * 100_000}},
            },
        }
        evaluation = {
            "schema_version": 1,
            "generated_at_utc": "2026-06-19T12:00:00Z",
            "top_line_status": "ok",
            "metrics": [],
            "alerts": [],
        }

        record = pipeline_health.compact_history_record(facts, evaluation)
        encoded = json.dumps(record)

        self.assertNotIn("facts", record)
        self.assertNotIn("payload", encoded)
        self.assertLess(len(encoded), 1_000)

    def test_compact_metric_series_keeps_only_historical_values(self) -> None:
        series = pipeline_health.compact_metric_series(
            [
                {
                    "sampled_at_utc": "2026-06-19T12:00:00Z",
                    "evaluation": {
                        "metrics": [
                            {
                                "id": "live_feed.metars.stale_seconds",
                                "value": 123,
                                "severity": "warning",
                                "message": "not needed in series",
                            }
                        ]
                    },
                }
            ]
        )

        self.assertEqual(
            series["samples"][0]["metrics"]["live_feed.metars.stale_seconds"],
            123,
        )
        self.assertNotIn("warning", json.dumps(series))

    def test_daily_history_reader_bounds_record_count_across_days(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            health_root = Path(temp_dir)
            write_history_records(
                pipeline_health.history_path_for_date(health_root, date(2026, 6, 27)),
                ["old-1", "old-2", "old-3"],
            )
            write_history_records(
                pipeline_health.history_path_for_date(health_root, date(2026, 6, 28)),
                ["new-1", "new-2"],
            )

            history = pipeline_health.read_history(
                health_root,
                4,
                now=datetime(2026, 6, 28, 12, 0, 0, tzinfo=timezone.utc),
            )

            self.assertEqual(
                [record["sampled_at_utc"] for record in history.records],
                ["old-2", "old-3", "new-1", "new-2"],
            )
            self.assertLessEqual(len(history.records), 4)
            self.assertEqual(len(history.files), 2)

    def test_history_reader_clamps_requested_limit(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            health_root = Path(temp_dir)
            path = pipeline_health.history_path_for_date(health_root, date(2026, 6, 28))
            write_history_records(
                path,
                [f"sample-{index}" for index in range(pipeline_health.HISTORY_RECORD_LIMIT + 5)],
            )

            history = pipeline_health.read_history(
                health_root,
                pipeline_health.HISTORY_RECORD_LIMIT + 100,
                now=datetime(2026, 6, 28, 12, 0, 0, tzinfo=timezone.utc),
            )

            self.assertEqual(len(history.records), pipeline_health.HISTORY_RECORD_LIMIT)
            self.assertEqual(history.records[0]["sampled_at_utc"], "sample-5")


def metric(evaluation: dict, metric_id: str) -> dict:
    for item in evaluation["metrics"]:
        if item["id"] == metric_id:
            return item
    raise AssertionError(f"missing metric {metric_id}")


def has_metric(evaluation: dict, metric_id: str) -> bool:
    return any(item["id"] == metric_id for item in evaluation["metrics"])


def calendar_facts(
    cycles: list[dict],
    *,
    product_facts: list[dict] | None = None,
) -> dict:
    return {
        "inputs": {
            "current_artifacts": {"error": None, "payload": []},
            "deploy_health": {"error": None, "payload": {}},
            "live_feeds_status": {"error": None, "payload": {"products": {}}},
            "build_watch": {"error": None, "payload": {}},
            "faa_cycle_calendar": {
                "error": None,
                "payload": {"cycles": cycles},
            },
            "product_facts": product_facts or [],
        }
    }


def write_history_records(path: Path, sampled_at_values: list[str]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as stream:
        for sampled_at in sampled_at_values:
            stream.write(
                json.dumps(
                    {
                        "schema_version": 1,
                        "sampled_at_utc": sampled_at,
                        "evaluation": {
                            "schema_version": 1,
                            "generated_at_utc": sampled_at,
                            "top_line_status": "ok",
                            "metrics": [],
                            "alerts": [],
                        },
                        "product_facts_key": [],
                        "product_counts": {"error_count": 0, "warning_count": 0},
                    },
                    sort_keys=True,
                )
                + "\n"
            )


if __name__ == "__main__":
    unittest.main()
