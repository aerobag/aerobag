#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from datetime import date, datetime, timedelta, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import pipeline_health


TEST_LIVE_FEED_HEALTH_POLICIES = {
    "tafs": ("TAFs", 60 * 60, 3 * 60 * 60),
    "metars": ("METARs", 7 * 60, 30 * 60),
    "pireps": ("PIREPs", 15 * 60, 30 * 60),
    "obstacles": ("Obstacles", 2 * 24 * 60 * 60, 7 * 24 * 60 * 60),
    "tfrs": ("TFRs", 3 * 60 * 60, 6 * 60 * 60),
    "nexrad": ("NEXRAD", 700, 15 * 60),
    "notams": ("NOTAMs", 5 * 60, 15 * 60),
    "winds-aloft": ("Winds aloft", 12 * 60 * 60, 18 * 60 * 60),
}


def evaluate_health(
    facts: dict, history: list, now: datetime
) -> dict:
    payload = facts.get("inputs", {}).get("live_feeds_status", {}).get("payload")
    if isinstance(payload, dict) and "product_policies" not in payload:
        products = payload.get("products")
        product_ids = products.keys() if isinstance(products, dict) else []
        payload["product_policies"] = [
            {
                "product_id": product_id,
                "display_name": TEST_LIVE_FEED_HEALTH_POLICIES[product_id][0],
                "operator_health": {
                    "warning_after_seconds": TEST_LIVE_FEED_HEALTH_POLICIES[product_id][1],
                    "critical_after_seconds": TEST_LIVE_FEED_HEALTH_POLICIES[product_id][2],
                },
            }
            for product_id in product_ids
            if product_id in TEST_LIVE_FEED_HEALTH_POLICIES
        ]
    return pipeline_health.evaluate_health(facts, history, now)


class PipelineHealthTests(unittest.TestCase):
    def test_unknown_build_result_cannot_report_healthy(self) -> None:
        metrics: list[dict] = []
        facts = {
            "inputs": {
                "build_watch": {
                    "payload": {"result": {"status": "mystery"}},
                }
            }
        }

        pipeline_health.add_build_watch_metrics(metrics, facts)

        self.assertEqual(metrics[0]["id"], "cycle_build.latest_result")
        self.assertEqual(metrics[0]["severity"], "warning")

    def test_live_feed_health_requires_daemon_product_policy(self) -> None:
        now = datetime(2026, 8, 17, 12, 0, 0, tzinfo=timezone.utc)
        facts = {
            "inputs": {
                "current_artifacts": {"error": None, "payload": []},
                "deploy_health": {"error": None, "payload": {}},
                "live_feeds_status": {
                    "error": None,
                    "payload": {"products": {}, "product_policies": None},
                },
                "build_watch": {"error": None, "payload": {}},
                "faa_cycle_calendar": {"error": None, "payload": {"cycles": []}},
                "product_facts": [],
            }
        }

        evaluation = pipeline_health.evaluate_health(facts, [], now)

        policy = metric(evaluation, "live_feed.product_policy.present")
        self.assertEqual(policy["severity"], "critical")
        self.assertFalse(policy["value"])

    def test_live_feed_health_covers_pireps_and_winds_from_daemon_policy(self) -> None:
        now = datetime(2026, 8, 17, 12, 0, 0, tzinfo=timezone.utc)
        facts = {
            "inputs": {
                "current_artifacts": {"error": None, "payload": []},
                "deploy_health": {"error": None, "payload": {}},
                "live_feeds_status": {
                    "error": None,
                    "payload": {
                        "products": {
                            "pireps": {
                                "last_source_timestamp_utc": "2026-08-17T11:40:00Z",
                                "consecutive_failure_count": 0,
                            },
                            "winds-aloft": {
                                "last_source_timestamp_utc": "2026-08-16T23:00:00Z",
                                "consecutive_failure_count": 0,
                            },
                        }
                    },
                },
                "build_watch": {"error": None, "payload": {}},
                "faa_cycle_calendar": {"error": None, "payload": {"cycles": []}},
                "product_facts": [],
            }
        }

        evaluation = evaluate_health(facts, [], now)

        self.assertEqual(
            metric(evaluation, "live_feed.pireps.stale_seconds")["severity"],
            "warning",
        )
        self.assertEqual(
            metric(evaluation, "live_feed.winds-aloft.stale_seconds")["severity"],
            "warning",
        )

    def test_cloud_operator_authorization_is_derived_without_sending_master_secret(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            secret = Path(temp_dir) / "cloud.bin"
            secret.write_bytes(bytes([0x5A]) * 32)
            authorization, error = pipeline_health.cloud_status_authorization(secret)
        self.assertIsNone(error)
        self.assertEqual(
            authorization,
            "Bearer oAvfo7uXmJVexL5TLb2Uwt5nQZ7smFsvuqkN6YXikFg",
        )

    def test_dashboard_lazily_reuses_plots_and_serializes_refreshes(self) -> None:
        html = pipeline_health.dashboard_html()

        self.assertIn("new IntersectionObserver", html)
        self.assertIn("Plotly.purge(row.plot)", html)
        self.assertIn("Plotly.react(row.plot", html)
        self.assertIn("mergeCurrentSample(current)", html)
        self.assertIn("severityTrace(points, severity)", html)
        self.assertIn('warning:"#f0c85a"', html)
        self.assertIn('critical:"#ff6b6b"', html)
        self.assertIn("setTimeout(refreshLoop, 30000)", html)
        self.assertEqual(
            html.count('loadJson("/pipeline-health/series.json")'), 1
        )
        self.assertNotIn("setInterval(", html)

    def test_aerobag_cloud_uses_server_reported_limits_and_mode(self) -> None:
        now = datetime(2026, 8, 2, 12, 0, 0, tzinfo=timezone.utc)
        facts = {
            "inputs": {
                "current_artifacts": {"error": None, "payload": []},
                "deploy_health": {"error": None, "payload": {}},
                "live_feeds_status": {"error": None, "payload": {"products": {}}},
                "aerobag_cloud_status": {
                    "error": None,
                    "payload": {
                        "mode": "read_only",
                        "database_healthy": True,
                        "metrics": [
                            {
                                "id": "stored_bytes",
                                "current": 85,
                                "peak": 90,
                                "warning_at": 80,
                                "critical_at": 90,
                                "hard_limit": 100,
                                "window_seconds": None,
                                "rejected_in_window": 0,
                            },
                            {
                                "id": "current_sse_connections",
                                "current": 20,
                                "peak": 20,
                                "warning_at": None,
                                "critical_at": None,
                                "hard_limit": 20,
                                "window_seconds": None,
                                "rejected_in_window": 1,
                            },
                            {
                                "id": "account_creation_network_rate_rejections_5m",
                                "current": 3,
                                "peak": 3,
                                "warning_at": None,
                                "critical_at": None,
                                "hard_limit": None,
                                "window_seconds": 300,
                                "rejected_in_window": 3,
                            },
                            {
                                "id": "filesystem_free_bytes",
                                "current": 15,
                                "peak": 30,
                                "warning_at": 20,
                                "critical_at": 10,
                                "hard_limit": 0,
                                "window_seconds": None,
                                "rejected_in_window": 0,
                                "lower_is_worse": True,
                            },
                            {
                                "id": "backup_elapsed_ms",
                                "current": 120000,
                                "peak": 120000,
                                "warning_at": 30000,
                                "critical_at": 120000,
                                "hard_limit": None,
                                "window_seconds": None,
                                "rejected_in_window": 0,
                            },
                        ],
                    },
                },
                "build_watch": {"error": None, "payload": {}},
                "faa_cycle_calendar": {"error": None, "payload": {"cycles": []}},
                "product_facts": [],
            }
        }

        evaluation = evaluate_health(facts, [], now)

        self.assertEqual(metric(evaluation, "aerobag_cloud.mode")["severity"], "warning")
        stored = metric(evaluation, "aerobag_cloud.stored_bytes")
        self.assertEqual(stored["severity"], "warning")
        self.assertEqual(stored["warning_threshold"], 80)
        self.assertEqual(stored["critical_threshold"], 90)
        connections = metric(evaluation, "aerobag_cloud.current_sse_connections")
        self.assertEqual(connections["severity"], "critical")
        self.assertEqual(connections["critical_threshold"], 20)
        backup = metric(evaluation, "aerobag_cloud.backup_elapsed_ms")
        self.assertEqual(backup["severity"], "critical")
        self.assertEqual(backup["warning_threshold"], 30000)
        self.assertEqual(backup["critical_threshold"], 120000)
        creation_rejections = metric(
            evaluation,
            "aerobag_cloud.account_creation_network_rate_rejections_5m",
        )
        self.assertEqual(creation_rejections["value"], 3)
        self.assertEqual(creation_rejections["details"]["window_seconds"], 300)
        self.assertEqual(creation_rejections["details"]["rejected_in_window"], 3)
        filesystem_free = metric(evaluation, "aerobag_cloud.filesystem_free_bytes")
        self.assertEqual(filesystem_free["severity"], "warning")
        self.assertTrue(filesystem_free["details"]["lower_is_worse"])

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
                                    {
                                        "attempted_at_utc": "2026-06-19T11:55:00Z",
                                        "result": "success",
                                    },
                                    {
                                        "attempted_at_utc": "2026-06-19T12:00:00Z",
                                        "result": "failure",
                                    },
                                    {
                                        "attempted_at_utc": "2026-06-19T12:05:00Z",
                                        "result": "failure",
                                    },
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

        evaluation = evaluate_health(facts, [], now)

        stale = metric(evaluation, "live_feed.metars.stale_seconds")
        self.assertEqual(stale["value"], 600)
        self.assertEqual(stale["severity"], "warning")
        self.assertEqual(stale["warning_threshold"], 420)
        self.assertEqual(stale["critical_threshold"], 1800)
        failure_rate = metric(evaluation, "live_feed.metars.failure_rate_2h")
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

        evaluation = evaluate_health(facts, [], now)

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
                                    {
                                        "attempted_at_utc": "2026-06-19T11:30:00Z",
                                        "result": "success",
                                    },
                                    {
                                        "attempted_at_utc": "2026-06-19T11:55:00Z",
                                        "result": "success",
                                    },
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

        evaluation = evaluate_health(facts, [], now)

        failure_rate = metric(evaluation, "live_feed.metars.failure_rate_2h")
        self.assertEqual(failure_rate["value"], 0.333333)
        self.assertEqual(failure_rate["severity"], "warning")
        self.assertEqual(failure_rate["details"]["last_error"], "gzip failed")
        self.assertEqual(
            failure_rate["details"]["failures"][0]["attempted_at_utc"],
            "2026-06-19T11:00:00Z",
        )

    def test_live_feed_failure_rate_expires_after_two_hours(self) -> None:
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
                                "last_source_timestamp_utc": "2026-06-19T11:59:00Z",
                                "last_failure_at_utc": "2026-06-19T09:59:59Z",
                                "last_failure_phase": "build",
                                "last_error": "expired failure",
                                "consecutive_failure_count": 0,
                                "attempts": [
                                    {
                                        "attempted_at_utc": "2026-06-19T09:59:59Z",
                                        "result": "failure",
                                        "phase": "build",
                                        "error": "expired failure",
                                    },
                                    {
                                        "attempted_at_utc": "2026-06-19T10:30:00Z",
                                        "result": "success",
                                    },
                                    {
                                        "attempted_at_utc": "2026-06-19T11:59:00Z",
                                        "result": "success",
                                    },
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

        evaluation = evaluate_health(facts, [], now)

        failure_rate = metric(evaluation, "live_feed.metars.failure_rate_2h")
        self.assertEqual(failure_rate["value"], 0.0)
        self.assertEqual(failure_rate["severity"], "ok")
        self.assertEqual(failure_rate["details"]["attempt_count"], 2)
        self.assertEqual(failure_rate["details"]["failure_count"], 0)
        self.assertIsNone(failure_rate["details"]["last_error"])
        self.assertEqual(failure_rate["details"]["failures"], [])

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
                                "source_samples": [
                                    {
                                        "observed_at_utc": "2026-07-17T17:00:00Z",
                                        "cursor_utc": "2026-07-17T17:00:00Z",
                                        "rejected_count": 9,
                                    },
                                    {
                                        "observed_at_utc": "2026-07-17T19:50:00Z",
                                        "cursor_utc": "2026-07-17T19:49:00Z",
                                        "rejected_count": 1,
                                    },
                                ],
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

        evaluation = evaluate_health(facts, [], now)

        stale = metric(evaluation, "live_feed.notams.stale_seconds")
        self.assertEqual(stale["severity"], "critical")
        self.assertEqual(stale["warning_threshold"], 5 * 60)
        self.assertEqual(stale["critical_threshold"], 15 * 60)
        failures = metric(evaluation, "live_feed.notams.consecutive_failures")
        self.assertEqual(failures["severity"], "warning")
        failure_rate = metric(evaluation, "live_feed.notams.failure_rate_2h")
        self.assertEqual(failure_rate["severity"], "ok")
        self.assertIsNone(failure_rate["details"]["last_error"])
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
        rejected_updates = metric(
            evaluation, "live_feed.notams.rejected_api_updates_2h"
        )
        self.assertEqual(rejected_updates["value"], 1)
        self.assertEqual(rejected_updates["severity"], "warning")
        self.assertEqual(
            rejected_updates["details"]["samples"][0]["cursor_utc"],
            "2026-07-17T19:49:00Z",
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

        evaluation = evaluate_health(
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

        evaluation = evaluate_health(
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

        evaluation = evaluate_health(
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

        evaluation = evaluate_health(facts, [], now)

        countdown = metric(evaluation, "cycle_calendar.2607.seconds_until_effective")
        self.assertEqual(countdown["value"], 19 * 24 * 60 * 60)
        self.assertEqual(countdown["severity"], "warning")
        self.assertIn("effective in 19d", countdown["message"])

    def test_calendar_hides_unpublished_cycle_before_publication_window(self) -> None:
        facts = calendar_facts([{"cycle": "2607", "effective_date": "2026-07-09"}])
        now = datetime(2026, 6, 18, 12, 0, 0, tzinfo=timezone.utc)

        evaluation = evaluate_health(facts, [], now)

        self.assertFalse(
            has_metric(evaluation, "cycle_calendar.2607.seconds_until_effective")
        )

    def test_calendar_marks_unpublished_cycle_critical_inside_final_window(self) -> None:
        facts = calendar_facts([{"cycle": "2607", "effective_date": "2026-07-09"}])
        now = datetime(2026, 6, 24, 0, 0, 0, tzinfo=timezone.utc)

        evaluation = evaluate_health(facts, [], now)

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

        evaluation = evaluate_health(facts, [], now)

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

        evaluation = evaluate_health(facts, [], now)

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
            "metrics": [
                {
                    "id": "live_feed.metars.stale_seconds",
                    "label": "METAR age",
                    "value": 123,
                    "severity": "warning",
                    "message": "large repeated message",
                    "details": {"large": "z" * 100_000},
                },
                {
                    "id": "cycle_build.latest_result",
                    "value": "pass",
                    "severity": "ok",
                },
            ],
            "alerts": [],
        }

        record = pipeline_health.compact_history_record(facts, evaluation)
        encoded = json.dumps(record)

        self.assertNotIn("facts", record)
        self.assertNotIn("payload", encoded)
        self.assertNotIn("evaluation", record)
        self.assertNotIn("large repeated message", encoded)
        self.assertEqual(
            record["metrics"]["live_feed.metars.stale_seconds"],
            {"value": 123, "severity": "warning"},
        )
        self.assertNotIn("cycle_build.latest_result", record["metrics"])
        self.assertLess(len(encoded), 1_000)

    def test_compact_metric_series_preserves_bucket_extrema_and_severity(self) -> None:
        now = datetime(2026, 6, 19, 12, 5, 0, tzinfo=timezone.utc)
        series = pipeline_health.compact_metric_series(
            [
                {
                    "sampled_at_utc": "2026-06-19T12:00:00Z",
                    "metrics": {
                        "live_feed.metars.stale_seconds": 10,
                    },
                },
                {
                    "sampled_at_utc": "2026-06-19T12:02:00Z",
                    "metrics": {
                        "live_feed.metars.stale_seconds": {
                            "value": 123,
                            "severity": "critical",
                        },
                    },
                },
                {
                    "sampled_at_utc": "2026-06-19T12:04:00Z",
                    "metrics": {
                        "live_feed.metars.stale_seconds": 20,
                    },
                },
            ],
            now=now,
        )

        self.assertEqual(series["times"], ["2026-06-19T12:00:00Z"])
        columns = series["series"]["live_feed.metars.stale_seconds"]
        self.assertEqual(columns["first"], [10])
        self.assertEqual(columns["last"], [20])
        self.assertEqual(columns["min"], [10])
        self.assertEqual(columns["max"], [123])
        self.assertEqual(columns["severity"], [2])

    def test_compact_metric_series_is_bounded_to_one_day_of_buckets(self) -> None:
        now = datetime(2026, 6, 20, 12, 0, 0, tzinfo=timezone.utc)
        records = []
        for index in range(pipeline_health.DASHBOARD_BUCKET_LIMIT + 20):
            sampled_at = now - timedelta(minutes=5 * index)
            records.append(
                {
                    "sampled_at_utc": pipeline_health.iso_utc(sampled_at),
                    "metrics": {"metric": index},
                }
            )

        series = pipeline_health.compact_metric_series(records, now=now)

        self.assertLessEqual(
            len(series["times"]), pipeline_health.DASHBOARD_BUCKET_LIMIT
        )
        self.assertEqual(len(series["series"]["metric"]["last"]), 288)

    def test_history_migration_removes_full_evaluation_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "pipeline_health-2026-06-19.jsonl"
            path.write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "sampled_at_utc": "2026-06-19T12:00:00Z",
                        "evaluation": {
                            "metrics": [
                                {
                                    "id": "metric",
                                    "value": 7,
                                    "severity": "warning",
                                    "message": "discard me",
                                }
                            ]
                        },
                        "product_facts_key": ["publication"],
                        "product_counts": {"error_count": 1, "warning_count": 2},
                    }
                )
                + "\n",
                encoding="utf-8",
            )

            self.assertTrue(pipeline_health.migrate_history_file(path))
            migrated = json.loads(path.read_text(encoding="utf-8"))

        self.assertEqual(
            migrated["history_schema_version"],
            pipeline_health.HISTORY_SCHEMA_VERSION,
        )
        self.assertEqual(
            migrated["metrics"]["metric"],
            {"value": 7, "severity": "warning"},
        )
        self.assertEqual(migrated["product_facts_key"], ["publication"])
        self.assertNotIn("evaluation", migrated)
        self.assertNotIn("discard me", json.dumps(migrated))

    def test_history_retention_removes_only_expired_daily_files(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            health_root = Path(temp_dir)
            expired = health_root / "pipeline_health-2026-06-14.jsonl"
            retained = health_root / "pipeline_health-2026-06-15.jsonl"
            unrelated = health_root / "status.json"
            for path in (expired, retained, unrelated):
                path.write_text("{}\n", encoding="utf-8")

            removed = pipeline_health.prune_history_files(
                health_root,
                datetime(2026, 6, 28, 12, 0, 0, tzinfo=timezone.utc),
            )

            self.assertEqual(removed, [expired])
            self.assertFalse(expired.exists())
            self.assertTrue(retained.exists())
            self.assertTrue(unrelated.exists())

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
