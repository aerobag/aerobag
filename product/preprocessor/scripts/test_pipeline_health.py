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
from types import SimpleNamespace
from unittest.mock import patch

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
    if isinstance(payload, dict):
        payload.setdefault("schema_version", 2)
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


class NotamDeliveryMetricTests(unittest.TestCase):
    def facts(self, version=4):
        return {"inputs": {"live_feeds_status": {"payload": {
            "schema_version": version,
            "products": {"notams": {"failure_episodes": {}, "quality": {
                "source_record_count": 100,
                "client_record_count": 70,
                "server_only_records_by_keyword": {"AIRSPACE": 20, "(none)": 7, "NEW": 3},
                "delivery_audit": {
                    "tfr_overlap_count": 5,
                    "without_delivery_count": 25,
                    "tfr_version": "tfr-a",
                    "error": None,
                },
            }}},
        }}}}

    def metrics(self, facts):
        payload = facts["inputs"]["live_feeds_status"]["payload"]
        payload["product_policies"] = [{
            "product_id": "notams", "display_name": "NOTAMs",
            "operator_health": {"warning_after_seconds": 300, "critical_after_seconds": 900},
        }]
        metrics = []
        pipeline_health.add_live_feed_metrics(metrics, facts, datetime.now(timezone.utc))
        return {m["id"]: m for m in metrics}

    def test_coverage_and_category_breakdown_include_unknown_keywords(self):
        metrics = self.metrics(self.facts())
        self.assertEqual(metrics["live_feed.notams.without_delivery_count"]["value"], 25)
        excluded = metrics["live_feed.notams.server_only_record_count"]
        self.assertEqual(excluded["value"], 30)
        self.assertEqual(excluded["breakdown"], {"AIRSPACE": 20, "No keyword": 7,
            "NAV": 0, "OBST": 0, "COM": 0, "SVC": 0, "Other": 3})
        compact = pipeline_health.compact_evaluation_metrics({"metrics": list(metrics.values())})
        self.assertEqual(compact["live_feed.notams.server_only_record_count::Other"], 3)
        series = pipeline_health.compact_metric_series([{
            "sampled_at_utc": "2026-09-16T12:00:00Z", "metrics": compact,
        }], now=datetime(2026, 9, 16, 12, 1, tzinfo=timezone.utc))
        self.assertEqual(series["series"]["live_feed.notams.server_only_record_count::AIRSPACE"]["last"], [20])

    def test_bad_category_totals_and_missing_measurements_cannot_imply_coverage(self):
        for counts in [None, {"NAV": -1}, {"NAV": True}, {"NAV": 31}]:
            facts = self.facts()
            facts["inputs"]["live_feeds_status"]["payload"]["products"]["notams"]["quality"]["server_only_records_by_keyword"] = counts
            row = self.metrics(facts)["live_feed.notams.server_only_record_count"]
            self.assertIsNone(row["value"])
            self.assertNotIn("breakdown", row)
            self.assertEqual(row["severity"], "warning")

    def test_missing_or_inconsistent_promised_audit_is_not_zero(self):
        for bad in [None, {}, {"tfr_overlap_count": 40, "without_delivery_count": 0,
                              "tfr_version": "tfr-a", "error": None}]:
            facts = self.facts()
            facts["inputs"]["live_feeds_status"]["payload"]["products"]["notams"]["quality"]["delivery_audit"] = bad
            row = self.metrics(facts)["live_feed.notams.without_delivery_count"]
            self.assertIsNone(row["value"])
            self.assertEqual(row["severity"], "warning")

    def test_older_status_cannot_claim_delivery_coverage(self):
        row = self.metrics(self.facts(version=3))["live_feed.notams.without_delivery_count"]
        self.assertIsNone(row["value"])
        self.assertEqual(row["severity"], "not_instrumented")


class ChartQualityMetricTests(unittest.TestCase):
    now = datetime(2026, 9, 13, 15, 0, tzinfo=timezone.utc)

    def report(self, status="warning"):
        return {"schema_version": 2, "family": "SEC", "cycle": "2610", "report_id": "a" * 32,
                "started_at": "2026-09-13T14:59:00Z", "completed_at": "2026-09-13T15:00:00Z",
                "status": status, "warning_count": int(status == "warning"),
                "critical_count": int(status == "critical"), "unreviewed_count": 0,
                "regions": [{"chart": "Test SEC", "scores": {"overview": {"boundary": .2}}}],
                'publication': {'state': 'ready', 'has_map_sources': True,
                                'quarantined_sources': ['Test SEC.tif'] if status == 'critical' else [],
                                'excluded_metadata': ['SEC/Test SEC.geojson'] if status == 'critical' else []}}

    def test_partial_publication_is_still_critical_and_names_omissions(self):
        report = self.report('critical')
        metric = self.metrics(report)['chart_quality.SEC.unresolved']
        self.assertEqual(metric['severity'], 'critical')
        self.assertIn('OMITTED source sheets', metric['message'])
        self.assertIn('Test SEC.tif', metric['message'])
        self.assertEqual(metric['details']['publication'], report['publication'])

    def test_publication_decision_is_required_and_cannot_hide_criticals(self):
        for decision in [None, {}, {'state': 'ready', 'quarantined_sources': []}]:
            report = self.report('critical')
            report['publication'] = decision
            self.assertIsNotNone(pipeline_health.chart_quality_report_error(report))
        report = self.report('critical')
        report['publication']['quarantined_sources'] = []
        self.assertIsNotNone(pipeline_health.chart_quality_report_error(report))

    def metrics(self, report, previous=None, now=None):
        metrics = []
        pipeline_health.add_chart_quality_metrics(metrics, {"inputs": {"chart_quality": [
            {"family": "SEC", "payload": report, "error": None, "last_completed": previous}]}}, now or self.now)
        return {m["id"]: m for m in metrics}

    def test_warning_and_critical_do_not_require_increasing_counts(self):
        for status in ["warning", "critical"]:
            for _ in range(3):
                metric = self.metrics(self.report(status))["chart_quality.SEC.unresolved"]
                self.assertEqual(metric["severity"], status)
                self.assertEqual(metric["value"], 1)
                self.assertIn("/reports/" + "a" * 32, metric["details"]["review_url"])

    def test_incomplete_checks_are_unknown_not_zero_and_alarm_when_overdue(self):
        report = self.report("checking")
        for start, expected in [(report["started_at"], "unknown"), (None, "warning"),
                                ("invalid", "warning"), ("2026-09-13T15:01:00Z", "warning"),
                                ("2026-09-13T14:45:00Z", "warning")]:
            metrics = self.metrics({**report, "started_at": start})
            self.assertEqual(metrics["chart_quality.SEC.attempt"]["severity"], expected)
            self.assertEqual(metrics["chart_quality.SEC.unresolved"]["severity"], "unknown")
            self.assertIsNone(metrics["chart_quality.SEC.unresolved"]["value"])

    def test_retry_keeps_last_completed_findings_until_a_new_result(self):
        for status in ["ok", "warning", "critical"]:
            previous = {**self.report(status), "cycle": "2609"}
            metric = self.metrics(self.report("checking"), previous)["chart_quality.SEC.unresolved"]
            self.assertEqual(metric["severity"], status)
            self.assertIn("last completed check", metric["message"])
            self.assertIn("cycle 2609", metric["message"])
        self.assertEqual(self.metrics(self.report("ok"), self.report("critical"))[
            "chart_quality.SEC.unresolved"]["severity"], "ok")

    def test_malformed_checks_cannot_be_green(self):
        for bad in [None, {}, {**self.report(), "warning_count": "bad"}, {**self.report(), "regions": [{}]},
                    {**self.report("critical"), "status": "ok"}, {**self.report("ok"), "unreviewed_count": 1}]:
            metric = self.metrics(bad)["chart_quality.SEC.unresolved"]
            self.assertEqual(metric["severity"], "critical")
            self.assertEqual(metric["value"], 1)

    def test_collector_sees_unpublished_failed_attempt(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            current = root / "state/chart-quality/SEC/current.json"
            current.parent.mkdir(parents=True)
            current.write_text(json.dumps(self.report("critical")))
            reports = pipeline_health.collect_chart_quality(root)
            self.assertEqual(reports[0]["payload"]["status"], "critical")
            self.assertIsNone(reports[0]["error"])
            self.assertFalse((root / "published").exists())

    def test_collector_retains_latest_completed_findings_during_new_attempt(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            family = root / "state/chart-quality/SEC"
            for name, status, completed in [("old", "ok", "2026-09-13T14:00:00Z"),
                                             ("latest", "critical", "2026-09-13T14:30:00Z")]:
                report = family / "reports" / name / "report.json"
                report.parent.mkdir(parents=True)
                report.write_text(json.dumps({**self.report(status), "completed_at": completed}))
            (family / "current.json").write_text(json.dumps(self.report("checking")))
            reports = pipeline_health.collect_chart_quality(root)
            self.assertEqual(reports[0]["last_completed"]["status"], "critical")
            metrics = []
            pipeline_health.add_chart_quality_metrics(metrics, {"inputs": {"chart_quality": reports}}, self.now)
            self.assertEqual(next(m for m in metrics if m["id"].endswith(".unresolved"))["severity"], "critical")

    def test_review_assets_are_confined_to_report_tree(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report = root / "state/chart-quality/SEC/reports/id/index.html"
            report.parent.mkdir(parents=True)
            report.write_text("report")
            secret = root / "secret.json"
            secret.write_text("secret")
            (report.parent / "escape.json").symlink_to(secret)
            self.assertEqual(pipeline_health.chart_quality_file(root, "SEC/reports/id/index.html"), report)
            for path in ["../../secret.json", "%2e%2e/%2e%2e/secret.json", str(secret), "SEC/reports/id/escape.json", "SEC/.lock"]:
                self.assertIsNone(pipeline_health.chart_quality_file(root, path))


class PipelineHealthTests(unittest.TestCase):
    def test_release_channels_follow_one_active_generation(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            channel_root = root / "channel-current"
            channel_root.mkdir()
            (channel_root / "generation.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "generation": 7,
                        "production": "prod-tag",
                        "staging": "stage-tag",
                        "sunset": ["old-tag"],
                    }
                ),
                encoding="utf-8",
            )
            (channel_root / "live-feed-routes.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "production": "http://127.0.0.1:8101",
                        "staging": "http://127.0.0.1:8102",
                        "releases": {
                            "prod-tag": "http://127.0.0.1:8101",
                            "stage-tag": "http://127.0.0.1:8102",
                            "old-tag": "http://127.0.0.1:8103",
                        },
                    }
                ),
                encoding="utf-8",
            )
            config = pipeline_health.MonitorConfig(
                artifact_root=root,
                data_root=root,
                health_root=root / "health",
                channel_root=channel_root,
                standalone_current_artifacts_path=None,
                standalone_live_feeds_status_url=None,
                deploy_health_path=root / "health.json",
                cloud_status_url="http://127.0.0.1:1/cloud/v1/status",
                cloud_status_secret_path=root / "cloud.secret",
                build_watch_url="http://127.0.0.1:1/api/state",
                calendar_path=root / "calendar.json",
                listen="127.0.0.1:0",
                poll_seconds=60,
            )

            channels, status = pipeline_health.release_channel_sources(config)

        self.assertIsNone(status["error"])
        self.assertEqual(
            [(channel["id"], channel["role"], channel["tag"]) for channel in channels],
            [
                ("production", "production", "prod-tag"),
                ("staging", "staging", "stage-tag"),
                ("release-old-tag", "sunset", "old-tag"),
            ],
        )
        self.assertEqual(channels[1]["live_feeds_endpoint"], "http://127.0.0.1:8102")

    def test_collect_facts_keeps_release_inputs_separate(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            channel_root = root / "channel-current"
            for relative in ["production/packages", "staging/packages", "releases/prod/packages"]:
                (channel_root / relative).mkdir(parents=True)
                (channel_root / relative / "current_artifacts.json").write_text(
                    "[]\n", encoding="utf-8"
                )
            (channel_root / "generation.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "generation": 8,
                        "production": "prod",
                        "staging": "stage",
                        "sunset": [],
                    }
                ),
                encoding="utf-8",
            )
            (channel_root / "live-feed-routes.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "production": "http://127.0.0.1:8101",
                        "staging": "http://127.0.0.1:8102",
                        "releases": {},
                    }
                ),
                encoding="utf-8",
            )
            (root / "state").mkdir()
            (root / "state/releases-observed.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "releases": {
                            tag: {
                                "tag": tag,
                                "build_status": "passed",
                                "qualification_status": qualification,
                                "live_feed_status": "running",
                            }
                            for tag, qualification in [
                                ("prod", "passed"),
                                ("stage", "pending"),
                            ]
                        },
                    }
                ),
                encoding="utf-8",
            )
            deploy_health = root / "deploy-health.json"
            deploy_health.write_text("{}\n", encoding="utf-8")
            calendar = root / "calendar.json"
            calendar.write_text('{"cycles":[]}\n', encoding="utf-8")
            cloud_secret = root / "cloud.secret"
            cloud_secret.write_bytes(bytes(32))
            config = pipeline_health.MonitorConfig(
                artifact_root=root,
                data_root=root,
                health_root=root / "health",
                channel_root=channel_root,
                standalone_current_artifacts_path=None,
                standalone_live_feeds_status_url=None,
                deploy_health_path=deploy_health,
                cloud_status_url="http://127.0.0.1:8104/cloud/v1/status",
                cloud_status_secret_path=cloud_secret,
                build_watch_url="http://127.0.0.1:8105/api/state",
                calendar_path=calendar,
                listen="127.0.0.1:0",
                poll_seconds=60,
            )

            def fetch(url: str, **_kwargs: object) -> tuple[object, None]:
                if url.startswith("http://127.0.0.1:8101/"):
                    return {"marker": "production"}, None
                if url.startswith("http://127.0.0.1:8102/"):
                    return {"marker": "staging"}, None
                return {}, None

            with patch.object(pipeline_health, "fetch_json_url", side_effect=fetch):
                facts = pipeline_health.collect_facts(
                    config,
                    datetime(2026, 8, 24, 12, 0, tzinfo=timezone.utc),
                )

        self.assertEqual(
            facts["channels"]["production"]["inputs"]["live_feeds_status"][
                "payload"
            ]["marker"],
            "production",
        )
        self.assertEqual(
            facts["channels"]["staging"]["inputs"]["live_feeds_status"][
                "payload"
            ]["marker"],
            "staging",
        )
        self.assertEqual(
            facts["channels"]["staging"]["release_state"][
                "qualification_status"
            ],
            "pending",
        )

    def test_staging_failure_is_visible_without_declaring_production_down(self) -> None:
        now = datetime(2026, 8, 24, 12, 0, 0, tzinfo=timezone.utc)
        global_inputs = {
            "release_channels": {"error": None},
            "deploy_health": {"error": None, "payload": {}},
            "build_watch": {"error": None, "payload": {"result": {"status": "pass"}}},
            "faa_cycle_calendar": {"error": None, "payload": {"cycles": []}},
        }

        def channel(role: str, policy: object) -> dict:
            return {
                "role": role,
                "tag": f"{role}-tag",
                "inputs": {
                    "current_artifacts": {"error": None, "payload": []},
                    "product_facts": [{"payload": {"products": [{
                        "family": "nav-db", "cycle": "2609", "weather_camera_site_count": 974,
                    }]}}],
                    "live_feeds_status": {
                        "error": None,
                        "payload": {"schema_version": 2, "products": {}, "product_policies": policy},
                    },
                },
            }

        evaluation = pipeline_health.evaluate_health(
            {
                "sampled_at_utc": pipeline_health.iso_utc(now),
                "inputs": global_inputs,
                "channels": {
                    "production": channel("production", []),
                    "staging": channel("staging", None),
                },
            },
            [],
            now,
        )

        self.assertEqual(evaluation["scopes"]["production"]["status"], "ok")
        self.assertEqual(evaluation["scopes"]["staging"]["status"], "critical")
        self.assertEqual(evaluation["top_line_status"], "ok")
        self.assertEqual(evaluation["overall_status"], "critical")
        self.assertTrue(
            has_metric(
                evaluation,
                "channel.staging.live_feed.product_policy.present",
            )
        )

    def test_supported_sunset_failure_affects_operational_status(self) -> None:
        now = datetime(2026, 8, 24, 12, 0, 0, tzinfo=timezone.utc)
        facts = {
            "sampled_at_utc": pipeline_health.iso_utc(now),
            "inputs": {
                "release_channels": {"error": None},
                "deploy_health": {"error": None, "payload": {}},
                "build_watch": {"error": None, "payload": {"result": {"status": "pass"}}},
                "faa_cycle_calendar": {"error": None, "payload": {"cycles": []}},
            },
            "channels": {
                "production": {
                    "role": "production",
                    "tag": "prod",
                    "inputs": {
                        "current_artifacts": {"error": None, "payload": []},
                        "product_facts": [],
                        "live_feeds_status": {
                            "error": None,
                            "payload": {"products": {}, "product_policies": []},
                        },
                    },
                },
                "release-old": {
                    "role": "sunset",
                    "tag": "old",
                    "inputs": {
                        "current_artifacts": {"error": None, "payload": []},
                        "product_facts": [],
                        "live_feeds_status": {
                            "error": "connection refused",
                            "payload": None,
                        },
                    },
                },
            },
        }

        evaluation = pipeline_health.evaluate_health(facts, [], now)

        self.assertEqual(evaluation["scopes"]["release-old"]["status"], "critical")
        self.assertEqual(evaluation["top_line_status"], "critical")

    def test_staging_qualification_state_is_scoped_and_visible(self) -> None:
        metrics: list[dict] = []

        pipeline_health.add_channel_release_metrics(
            metrics,
            {
                "role": "staging",
                "tag": "candidate",
                "release_state": {
                    "build_status": "passed",
                    "qualification_status": "pending",
                    "live_feed_status": "running",
                },
            },
        )

        by_id = {item["id"]: item for item in metrics}
        self.assertEqual(by_id["release.build_status"]["severity"], "ok")
        self.assertEqual(
            by_id["release.qualification_status"]["severity"], "warning"
        )
        self.assertEqual(by_id["release.live_feed_status"]["severity"], "ok")

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

    def test_release_qualification_severity_distinguishes_sunset_from_production(self) -> None:
        for role in ["production", "staging", "sunset"]:
            for status in ["passed", "pending", "bypassed", "failed", None]:
                with self.subTest(role=role, status=status):
                    metrics = []
                    pipeline_health.add_channel_release_metrics(metrics, {
                        "role": role,
                        "tag": "example",
                        "release_state": {
                            "build_status": "passed",
                            "qualification_status": status,
                            "qualification_bypassed_at_utc": "2026-09-07T15:00:00Z",
                            "qualification_bypass_reason": "forced promotion",
                            "live_feed_status": "running",
                        },
                    })
                    qualification = next(item for item in metrics if item["id"] == "release.qualification_status")
                    expected = "critical"
                    if status == "passed":
                        expected = "ok"
                    elif role in {"staging", "sunset"} and status in {"pending", "bypassed"}:
                        expected = "warning"
                    self.assertEqual(qualification["severity"], expected)
                    if status == "bypassed":
                        self.assertIn("forced promotion", qualification["message"])
                        self.assertIn("2026-09-07T15:00:00Z", qualification["message"])

    def test_current_deployment_evidence_replaces_stale_staging_status(self) -> None:
        for role in ["production", "staging", "sunset"]:
            for status in ["passed", "pending", "failed", None]:
                with self.subTest(role=role, status=status):
                    metrics = []
                    pipeline_health.add_channel_release_metrics(metrics, {
                        "role": role, "tag": "example",
                        "release_state": {
                            "build_status": "passed", "live_feed_status": "running",
                            "qualification_status": "pending",
                            "deployment_status": status,
                            "deployment_error": "wrong About bytes" if status == "failed" else None,
                        },
                    })
                    metric = next(item for item in metrics if item["id"] == "release.qualification_status")
                    self.assertEqual(metric["value"], status)
                    expected = "ok" if status == "passed" else "critical"
                    if status == "pending" and role in {"staging", "sunset"}:
                        expected = "warning"
                    self.assertEqual(metric["severity"], expected)
                    if status == "failed":
                        self.assertIn("wrong About bytes", metric["message"])

    def test_passing_deployment_does_not_hide_forced_admission_after_refresh(self) -> None:
        for status in ["bypassed", "pending"]:
            with self.subTest(status=status):
                metrics = []
                pipeline_health.add_channel_release_metrics(metrics, {
                    "role": "production", "tag": "example",
                    "release_state": {
                        "build_status": "passed", "live_feed_status": "running",
                        "qualification_status": status, "deployment_status": "passed",
                        "qualification_bypassed_at_utc": "2026-09-08T19:00:00Z",
                        "qualification_bypass_reason": "forced promotion",
                    },
                })
                by_id = {item["id"]: item for item in metrics}
                self.assertEqual(by_id["release.qualification_status"]["severity"], "ok")
                self.assertEqual(by_id["release.qualification_bypass"]["severity"], "critical")
                self.assertIn("forced promotion", by_id["release.qualification_bypass"]["message"])
                self.assertIn("2026-09-08T19:00:00Z", by_id["release.qualification_bypass"]["message"])

    def test_deployment_pending_has_bounded_progress_without_hiding_failure(self) -> None:
        now = datetime(2026, 9, 13, 15, 0, tzinfo=timezone.utc)
        for role in ["production", "staging", "sunset"]:
            for age, error, expected in [(0, None, "unknown"), (599, None, "unknown"),
                                         (0, "wrong bytes", "critical"),
                                         (600, None, "critical" if role == "production" else "warning"),
                                         (-1, None, "critical" if role == "production" else "warning")]:
                metrics = []
                pipeline_health.add_channel_release_metrics(metrics, {
                    "role": role, "release_state": {
                        "build_status": "passed", "live_feed_status": "running",
                        "deployment_status": "pending", "deployment_error": error,
                        "deployment_pending_since_utc": pipeline_health.iso_utc(now - timedelta(seconds=age)),
                    },
                }, now)
                metric = next(m for m in metrics if m["id"] == "release.qualification_status")
                self.assertEqual(metric["severity"], expected, (role, age, error))
                self.assertEqual(metric["value"], "pending")

    def test_refresh_progress_failure_and_deadline_do_not_relabel_served_health(self) -> None:
        now = datetime(2026, 9, 13, 15, 0, tzinfo=timezone.utc)
        for status, age, error, expected in [
            ("running", 60, None, "unknown"), ("ready", 60, None, "unknown"),
            ("running", 1800, None, "warning"), ("ready", 1800, None, "warning"),
            ("running", -1, None, "warning"), ("failed", 60, "broken build", "warning"),
            ("running", 60, "broken build", "warning"), ("ready", 60, "broken build", "warning"),
            ("passed", 60, None, "ok"), ("nonsense", 60, None, "warning"),
        ]:
            metrics = []
            pipeline_health.add_channel_release_metrics(metrics, {"role": "production", "release_state": {
                "build_status": "passed", "live_feed_status": "running", "deployment_status": "passed",
                "product_refresh_status": status, "product_refresh_error": error,
                "product_refresh_started_at_utc": pipeline_health.iso_utc(now - timedelta(seconds=age)),
            }}, now)
            by_id = {m["id"]: m for m in metrics}
            self.assertEqual(by_id["release.product_refresh"]["severity"], expected)
            self.assertEqual(by_id["release.qualification_status"]["severity"], "ok")

    def test_live_feed_health_requires_daemon_product_policy(self) -> None:
        now = datetime(2026, 8, 17, 12, 0, 0, tzinfo=timezone.utc)
        facts = {
            "inputs": {
                "current_artifacts": {"error": None, "payload": []},
                "deploy_health": {"error": None, "payload": {}},
                "live_feeds_status": {
                    "error": None,
                    "payload": {"schema_version": 2, "products": {}, "product_policies": None},
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
        self.assertIn("function selectedScope()", html)
        self.assertIn('id="scopeNav"', html)
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
        self.assertEqual(failure_rate["severity"], "ok")
        ongoing = metric(evaluation, "live_feed.metars.failure_duration_seconds")
        self.assertEqual(ongoing["severity"], "critical")
        self.assertEqual(ongoing["value"], 600)

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
        self.assertEqual(failure_rate["severity"], "ok")
        ongoing = metric(evaluation, "live_feed.metars.failure_duration_seconds")
        self.assertEqual(ongoing["severity"], "ok")
        self.assertEqual(ongoing["value"], 0)
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
                                    "procedure_notams_without_ui_anchor": 1,
                                    "source_records_without_location": 1,
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
        self.assertEqual(failures["severity"], "ok")
        self.assertEqual(failures["value"], 1)
        ongoing = metric(evaluation, "live_feed.notams.failure_duration_seconds")
        self.assertEqual(ongoing["severity"], "critical")
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
        unanchored = metric(
            evaluation,
            "live_feed.notams.procedure_notams_without_ui_anchor",
        )
        self.assertEqual(unanchored["value"], 1)
        self.assertEqual(unanchored["severity"], "ok")
        self.assertEqual(unanchored["warning_threshold"], 2)
        missing_location = metric(
            evaluation,
            "live_feed.notams.source_records_without_location",
        )
        self.assertEqual(missing_location["value"], 1)
        self.assertEqual(missing_location["severity"], "warning")
        self.assertEqual(missing_location["warning_threshold"], 1)

        facts["inputs"]["live_feeds_status"]["payload"]["products"]["notams"][
            "quality"
        ]["procedure_notams_without_ui_anchor"] = 2
        increased = evaluate_health(facts, [], now)
        self.assertEqual(
            metric(
                increased,
                "live_feed.notams.procedure_notams_without_ui_anchor",
            )["severity"],
            "warning",
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
        self.assertEqual(record["states"]["cycle_build.latest_result"], {"value": "pass", "severity": "ok"})
        self.assertLess(len(encoded), 1_000)

    def test_history_keeps_alert_open_change_clear_and_missing_transitions(self) -> None:
        facts = {"sampled_at_utc": "2026-09-13T15:00:00Z", "inputs": {}}
        def sample(value, severity, message, previous=None):
            evaluation = {"metrics": [{"id": "checks", "value": value, "severity": severity, "message": message}],
                          "alerts": [{"metric_id": "checks", "severity": severity, "message": message, "scope": "global"}]
                          if severity in {"warning", "critical"} else []}
            return pipeline_health.compact_history_record(facts, evaluation, previous)
        first = sample("passed", "ok", "passed")
        opened = sample("pending", "warning", "overdue 600 seconds", first)
        self.assertEqual(opened["alert_transitions"][0]["event"], "opened")
        self.assertEqual(opened["states"]["checks"]["value"], "pending")
        unchanged = sample("pending", "warning", "overdue 660 seconds", opened)
        self.assertEqual(unchanged["alert_transitions"], [])
        changed = sample("failed", "critical", "wrong About bytes", unchanged)
        self.assertEqual(changed["alert_transitions"][0]["event"], "changed")
        self.assertEqual(changed["alert_transitions"][0]["message"], "wrong About bytes")
        # Disk roundtrip models sampler restart; yesterday's active alert survives.
        reloaded = json.loads(json.dumps(changed))
        cleared = sample("passed", "ok", "checks passed", reloaded)
        self.assertEqual(cleared["alert_transitions"][0]["event"], "cleared")
        self.assertEqual(cleared["active_alerts"], {})
        missing = pipeline_health.compact_history_record(facts, {"metrics": [], "alerts": []}, changed)
        self.assertEqual(missing["alert_transitions"][0]["event"], "unavailable")
        self.assertEqual(pipeline_health.compact_existing_history_record(changed), changed)
        self.assertNotIn("checks", pipeline_health.compact_metric_series([opened, changed])["series"])

    def test_legacy_numeric_history_does_not_invent_lost_alert_timeline(self) -> None:
        old = pipeline_health.compact_existing_history_record({
            "sampled_at_utc": "2026-09-13T15:00:00Z", "history_schema_version": 3,
            "metrics": {"some.count": 1},
        })
        self.assertNotIn("states", old)
        self.assertNotIn("active_alerts", old)
        events = pipeline_health.alert_transitions({"metrics": [], "alerts": [
            {"metric_id": "checks", "severity": "critical", "message": "pending"},
        ]}, old)
        self.assertEqual(events[0]["event"], "observed")

    def test_compact_history_keeps_product_baselines_separate_by_channel(self) -> None:
        facts = {
            "sampled_at_utc": "2026-08-24T12:00:00Z",
            "inputs": {},
            "channels": {
                "production": {"inputs": {"product_facts": []}},
                "staging": {"inputs": {"product_facts": []}},
            },
        }

        record = pipeline_health.compact_history_record(
            facts,
            {"metrics": [], "alerts": [], "top_line_status": "ok"},
        )

        self.assertEqual(
            set(record["channel_product_states"]), {"production", "staging"}
        )
        self.assertNotIn("product_facts_key", record)
        self.assertNotIn("product_counts", record)

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


class WeatherCameraMetricTests(unittest.TestCase):
    def facts(self, counts: list[object]) -> dict:
        return calendar_facts([], product_facts=[{"payload": {"products": [
            {"family": "nav-db", "product_id": f"NAV_DB_TEST_{index}",
             "cycle": f"261{index}", "weather_camera_site_count": count}
            for index, count in enumerate(counts)
        ]}}])

    def evaluate(self, facts: dict) -> dict:
        return pipeline_health.evaluate_health(facts, [], datetime(2026, 9, 7, tzinfo=timezone.utc))

    def test_published_site_count_alerts_below_960_including_loss_of_canada(self) -> None:
        for count, severity in [(974, "ok"), (960, "ok"), (959, "warning"), (756, "warning"), (0, "warning")]:
            with self.subTest(count=count):
                evaluation = self.evaluate(self.facts([count]))
                camera = metric(evaluation, "cycle_product.weather_camera_site_count")
                self.assertEqual(camera["value"], count)
                self.assertEqual(camera["severity"], severity)
                self.assertEqual(camera["warning_threshold"], 960)
                alerts = [alert for alert in evaluation["alerts"] if alert["metric_id"] == camera["id"]]
                self.assertEqual(bool(alerts), severity != "ok")

    def test_overlapping_cycles_cannot_add_counts_or_mask_a_small_inventory(self) -> None:
        camera = metric(self.evaluate(self.facts([974, 756])), "cycle_product.weather_camera_site_count")
        self.assertEqual(camera["value"], 756)
        self.assertEqual(camera["severity"], "warning")
        self.assertEqual(camera["details"]["cycle_counts"], {"2610": 974, "2611": 756})

    def test_missing_invalid_or_partially_missing_counts_are_visible(self) -> None:
        for counts in [[], [None], [974, None], [-1], [True], ["974"]]:
            with self.subTest(counts=counts):
                camera = metric(self.evaluate(self.facts(counts)), "cycle_product.weather_camera_site_count")
                self.assertIsNone(camera["value"])
                self.assertEqual(camera["severity"], "warning")
                self.assertIn("unavailable", camera["message"])

    def test_production_and_staging_camera_counts_remain_separate(self) -> None:
        facts = {**calendar_facts([]), "channels": {
            "production": {**self.facts([974]), "role": "production", "tag": "prod"},
            "staging": {**self.facts([756]), "role": "staging", "tag": "stage"},
        }}
        evaluation = self.evaluate(facts)
        production = metric(evaluation, "channel.production.cycle_product.weather_camera_site_count")
        staging = metric(evaluation, "channel.staging.cycle_product.weather_camera_site_count")
        self.assertEqual(production["value"], 974)
        self.assertEqual(production["severity"], "ok")
        self.assertEqual(staging["value"], 756)
        self.assertEqual(staging["severity"], "warning")


class ReleaseProductDiagnosticsTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.channels = self.root / "channel-current"
        self.channels.mkdir()
        (self.channels / "generation.json").write_text(json.dumps({
            "schema_version": 1, "production": "new", "staging": None, "sunset": ["old"],
        }), encoding="utf-8")
        (self.channels / "live-feed-routes.json").write_text(json.dumps({
            "production": "http://127.0.0.1:8100",
            "releases": {"old": "http://127.0.0.1:8101"},
        }), encoding="utf-8")
        self.config = SimpleNamespace(artifact_root=self.root, channel_root=self.channels)

    def sample(self, publication: str, production: int = 153, sunset: int = 153) -> dict:
        manifests = []
        for tag, count in [("new", production), ("old", sunset)]:
            packaged = f"{tag}/{publication}/packaged"
            directory = self.root / "published" / packaged
            directory.mkdir(parents=True, exist_ok=True)
            (directory / "product-facts.json").write_text(json.dumps({
                "products": [{"cycle": "2609", "error_count": 0, "warning_count": count}],
            }), encoding="utf-8")
            manifest = {"artifact_roots": {"packaged": packaged}}
            manifests.append(manifest)
            packages = self.channels / "releases" / tag / "packages"
            packages.mkdir(parents=True, exist_ok=True)
            (packages / "current_artifacts.json").write_text(json.dumps([manifest]), encoding="utf-8")
        merged = self.channels / "production/packages"
        merged.mkdir(parents=True, exist_ok=True)
        (merged / "current_artifacts.json").write_text(json.dumps(manifests), encoding="utf-8")
        sources, _status = pipeline_health.release_channel_sources(self.config)
        with patch.object(pipeline_health, "fetch_json_url", return_value=({}, None)):
            channels = {
                source["id"]: pipeline_health.collect_channel_facts(self.config, source, None)
                for source in sources
            }
        return {"sampled_at_utc": "2026-09-07T15:00:00Z", "channels": channels}

    def warning(self, facts: dict, history: list, scope: str = "production") -> dict:
        metrics = []
        pipeline_health.add_product_fact_metrics(
            metrics, {"inputs": facts["channels"][scope]["inputs"]}, history,
            history_scope=scope,
        )
        return next(item for item in metrics if item["id"] == "cycle_product.warning_count")

    def history(self, facts: dict) -> dict:
        return pipeline_health.compact_history_record(facts, {"metrics": []})

    def test_each_release_has_its_own_regression_signal(self) -> None:
        baseline = self.sample("baseline")
        history = [self.history(baseline)]
        current = self.sample("current")
        production_inputs = current["channels"]["production"]["inputs"]
        self.assertEqual(len(production_inputs["current_artifacts"]["payload"]), 2)
        self.assertEqual(len(production_inputs["product_facts"]), 1)
        self.assertEqual(self.warning(current, history)["value"], 153)
        self.assertEqual(self.warning(current, history)["severity"], "ok")

        increased = self.sample("increase", production=154)
        warning = self.warning(increased, history)
        self.assertEqual(warning["value"], 154)
        self.assertEqual(warning["warning_threshold"], 154)
        self.assertEqual(warning["severity"], "warning")
        self.assertEqual(self.warning(increased, history, "release-old")["severity"], "ok")

        sunset_increased = self.sample("sunset-increase", sunset=154)
        self.assertEqual(self.warning(sunset_increased, history)["severity"], "ok")
        self.assertEqual(self.warning(sunset_increased, history, "release-old")["severity"], "warning")

    def test_legacy_merged_history_cannot_mask_a_new_regression(self) -> None:
        legacy_single = {"channel_product_states": {"production": {
            "product_facts_key": ["previous/product-facts.json"],
            "product_counts": {"error_count": 0, "warning_count": 153},
        }}}
        legacy_merged = {"channel_product_states": {"production": {
            "product_facts_key": ["old/product-facts.json", "new/product-facts.json"],
            "product_counts": {"error_count": 0, "warning_count": 306},
        }}}
        history = [legacy_single, legacy_merged]
        self.assertEqual(self.warning(self.sample("stable"), history)["severity"], "ok")
        warning = self.warning(self.sample("increase", production=154), history)
        self.assertEqual(warning["severity"], "warning")
        self.assertEqual(warning["warning_threshold"], 154)
        self.assertIn("previous distinct publication: 153", warning["message"])

    def test_missing_release_manifest_is_visible_even_if_merged_manifest_exists(self) -> None:
        self.sample("current")
        (self.channels / "releases/new/packages/current_artifacts.json").unlink()
        sources, _status = pipeline_health.release_channel_sources(self.config)
        with patch.object(pipeline_health, "fetch_json_url", return_value=({}, None)):
            channel = pipeline_health.collect_channel_facts(self.config, sources[0], None)
        metrics = []
        pipeline_health.add_channel_input_metrics(metrics, channel)
        by_id = {item["id"]: item for item in metrics}
        self.assertEqual(by_id["input.current_artifacts.available"]["severity"], "ok")
        self.assertEqual(by_id["input.product_facts_artifacts.available"]["severity"], "critical")
        self.assertEqual(channel["inputs"]["product_facts"], [])


class LiveFeedRecoveryTests(unittest.TestCase):
    start = datetime(2026, 9, 10, 12, 0, tzinfo=timezone.utc)

    def at(self, seconds: int) -> str:
        return pipeline_health.iso_utc(self.start + timedelta(seconds=seconds))

    def attempt(self, seconds: int, result: str, phase: str | None = None) -> dict:
        return {
            "attempted_at_utc": self.at(seconds), "result": result, "phase": phase,
            "error": "test failure" if result == "failure" else None,
            "source_timestamp_utc": self.at(seconds) if result == "success" else None,
        }

    def metrics(self, status: dict, seconds: int, product: str = "tfrs") -> dict:
        display, warning, critical = TEST_LIVE_FEED_HEALTH_POLICIES[product]
        facts = {"inputs": {"live_feeds_status": {"payload": {
            "schema_version": 3 if "failure_episodes" in status else 2,
            "active_sse_clients": 0,
            "products": {product: status},
            "product_policies": [{"product_id": product, "display_name": display,
                                  "operator_health": {"warning_after_seconds": warning,
                                                      "critical_after_seconds": critical}}],
        }}}}
        metrics = []
        pipeline_health.add_live_feed_metrics(metrics, facts, self.start + timedelta(seconds=seconds))
        return {m["id"].removeprefix(f"live_feed.{product}."): m for m in metrics}

    def test_failure_grace_persistence_and_recovery_with_history_retained(self) -> None:
        for explicit_episodes in [False, True]:
            with self.subTest(explicit_episodes=explicit_episodes):
                status = {"last_source_timestamp_utc": self.at(0),
                          "attempts": [self.attempt(0, "failure", "build")]}
                if explicit_episodes:
                    status["failure_episodes"] = {"publication": {
                        "first_failure_at_utc": self.at(0), "last_failure_at_utc": self.at(0),
                        "failure_count": 1, "phase": "build", "error": "test failure",
                    }}
                for age, severity in [(0, "ok"), (119, "ok"), (120, "warning"),
                                      (599, "warning"), (600, "critical")]:
                    metrics = self.metrics(status, age)
                    self.assertEqual(metrics["failure_duration_seconds"]["value"], age)
                    self.assertEqual(metrics["failure_duration_seconds"]["severity"], severity)
                    self.assertEqual(metrics["failure_rate_2h"]["severity"], "ok")
                status["attempts"].append(self.attempt(610, "success"))
                status["last_source_timestamp_utc"] = self.at(610)
                if explicit_episodes:
                    status["failure_episodes"] = {}
                recovered = self.metrics(status, 611)
                self.assertEqual(recovered["failure_duration_seconds"]["value"], 0)
                self.assertTrue(all(m["severity"] == "ok" for m in recovered.values()))
                self.assertEqual(recovered["failure_rate_2h"]["details"]["failure_count"], 1)
                self.assertEqual(recovered["failure_rate_2h"]["details"]["last_error"], "test failure")

    def test_hiccup_recovered_before_first_monitor_sample_never_alarms(self) -> None:
        status = {"last_source_timestamp_utc": self.at(10), "consecutive_failure_count": 0,
                  "attempts": [self.attempt(0, "failure", "publish"), self.attempt(10, "success")]}
        metrics = self.metrics(status, 60)
        self.assertTrue(all(m["severity"] == "ok" for m in metrics.values()))
        self.assertEqual(metrics["failure_rate_2h"]["value"], 0.5)

    def test_legacy_source_and_publication_recover_independently(self) -> None:
        status = {"last_source_timestamp_utc": self.at(30), "consecutive_failure_count": 0,
                  "attempts": [self.attempt(-60, "success"), self.attempt(0, "failure", "publish"),
                               self.attempt(30, "nms_poll", "source")]}
        self.assertEqual(self.metrics(status, 120, "notams")["failure_duration_seconds"]["severity"], "warning")
        status["attempts"].extend([self.attempt(180, "success"), self.attempt(190, "failure", "poll"),
                                   self.attempt(210, "success")])
        metrics = self.metrics(status, 311, "notams")
        self.assertEqual(metrics["failure_duration_seconds"]["value"], 121)
        self.assertEqual(set(metrics["failure_duration_seconds"]["details"]["episodes"]), {"source"})
        status["attempts"].append(self.attempt(330, "nms_poll", "source"))
        self.assertEqual(self.metrics(status, 331, "notams")["failure_duration_seconds"]["value"], 0)

    def test_legacy_source_heartbeat_does_not_mask_stale_publication(self) -> None:
        status = {"last_source_timestamp_utc": self.at(899), "last_success_at_utc": self.at(899),
                  "attempts": [self.attempt(-1, "success"), self.attempt(899, "nms_poll", "source")]}
        metrics = self.metrics(status, 900, "notams")
        self.assertEqual(metrics["stale_seconds"]["value"], 901)
        self.assertEqual(metrics["stale_seconds"]["severity"], "critical")
        status["attempts"] = [self.attempt(899, "nms_poll", "source")]
        self.assertEqual(self.metrics(status, 900, "notams")["stale_seconds"]["severity"], "critical")

    def test_explicit_episode_survives_attempt_history_eviction(self) -> None:
        status = {"last_source_timestamp_utc": self.at(0),
                  "attempts": [self.attempt(9000, "nms_poll", "source")],
                  "failure_episodes": {"publication": {
                      "first_failure_at_utc": self.at(0), "last_failure_at_utc": self.at(0),
                      "failure_count": 1, "phase": "publish", "error": "still broken"}}}
        metrics = self.metrics(status, 9000)
        self.assertEqual(metrics["failure_duration_seconds"]["value"], 9000)
        self.assertEqual(metrics["failure_duration_seconds"]["severity"], "critical")
        self.assertEqual(metrics["failure_duration_seconds"]["details"]["failures"][0]["error"], "still broken")

    def test_malformed_explicit_episodes_cannot_claim_recovery(self) -> None:
        for episodes in [None, [], {"publication": {}}, {"publication": {"first_failure_at_utc": "bad", "failure_count": 1}}]:
            status = {"last_source_timestamp_utc": self.at(0), "failure_episodes": episodes}
            self.assertEqual(self.metrics(status, 120)["failure_duration_seconds"]["severity"], "critical")

    def test_v3_missing_instrumentation_is_not_interpreted_as_legacy(self) -> None:
        self.assertIsNone(pipeline_health.live_feed_failure_episodes({}, [], 3))
        self.assertEqual(pipeline_health.live_feed_failure_episodes({}, [], 2), {})

    def test_episode_started_during_monitor_fetch_is_not_a_coverage_alarm(self) -> None:
        status = {"last_source_timestamp_utc": self.at(0), "failure_episodes": {"publication": {
            "first_failure_at_utc": self.at(1), "last_failure_at_utc": self.at(1),
            "failure_count": 1, "phase": "publish", "error": "just failed",
        }}}
        metric = self.metrics(status, 0)["failure_duration_seconds"]
        self.assertEqual(metric["value"], 0)
        self.assertEqual(metric["severity"], "ok")


class LiveFeedBindingCollectionTests(unittest.TestCase):
    now = datetime(2026, 9, 13, tzinfo=timezone.utc)

    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.generation = self.root / "channel-generations/00000002"
        self.generation.mkdir(parents=True)
        self.channel_root = self.root / "channel-current"
        self.channel_root.symlink_to(self.generation, target_is_directory=True)
        self.config = SimpleNamespace(
            artifact_root=self.root, data_root=self.root, channel_root=self.channel_root,
            standalone_current_artifacts_path=None, standalone_live_feeds_status_url=None,
            deploy_health_path=self.root / "health.json", cloud_status_secret_path=self.root / "secret",
            cloud_status_url="http://cloud/status", build_watch_url="http://build/status",
            calendar_path=self.root / "calendar.json",
        )
        self.metadata = {"schema_version": 2, "generation": 2, "production": "prod",
                         "staging": "stage", "sunset": ["old", "older"]}
        self.routes = {"schema_version": 1, "production": "http://127.0.0.1:8100",
                       "staging": "http://127.0.0.1:8101", "releases": {
                           tag: f"http://127.0.0.1:{8101 if tag == 'stage' else 8100}"
                           for tag in ["prod", "stage", "old", "older"]}}
        self.bindings = {"schema_version": 1, "releases": {
            tag: {"release_tag": tag, "provider": "stage-instance" if tag == "stage" else "prod-instance",
                  "provider_tag": "stage" if tag == "stage" else "prod",
                  "endpoint": endpoint, "reason": "compatible" if tag in {"old", "older"} else "dedicated",
                  "verification": "a" * 64}
            for tag, endpoint in self.routes["releases"].items()}}
        self.observed = {"schema_version": 2, "releases": {
            tag: {"tag": tag, "commit": tag, "release_root": str(self.root / tag),
                  "build_status": "passed", "qualification_status": "passed",
                  "deployment_status": "passed", "live_feed_status": "stopped",
                  "live_feed_instance": f"{tag}-redundant", "live_feed_provider": f"{tag}-redundant"}
            for tag in self.routes["releases"]}, "live_feed_instances": {
                "prod-instance": {"instance_id": "prod-instance", "release_tag": "prod", "status": "running",
                                  "endpoint": self.routes["production"], "launch_digest": "b" * 64,
                                  "manifest_sha256": "c" * 64},
                "stage-instance": {"instance_id": "stage-instance", "release_tag": "stage", "status": "running",
                                   "endpoint": self.routes["staging"]},
            }}
        for channel in ["production", "staging", "releases/prod", "releases/old", "releases/older"]:
            path = self.generation / channel / "packages/current_artifacts.json"
            path.parent.mkdir(parents=True)
            path.write_text("[]")

    def write_metadata(self) -> None:
        for name, document in [("generation.json", self.metadata), ("live-feed-routes.json", self.routes),
                               ("live-feed-bindings.json", self.bindings)]:
            (self.generation / name).write_text(json.dumps(document))
        state = self.root / "state/releases-observed.json"
        state.parent.mkdir(exist_ok=True)
        state.write_text(json.dumps(self.observed))

    def collect(self, *, failed_probe: bool = False) -> dict:
        self.write_metadata()

        def fetch(url: str, **kwargs) -> tuple:
            if failed_probe and url == self.routes["production"] + "/live-feeds/status.json":
                return None, "connection refused"
            return {"schema_version": 3, "active_sse_clients": 2 if ":8101/" in url else 7,
                    "products": {}, "product_policies": []}, None

        with patch.object(pipeline_health, "cloud_status_authorization", return_value=("unused", None)), \
             patch.object(pipeline_health, "fetch_json_url", side_effect=fetch) as requests, \
             patch.object(pipeline_health, "collect_telemetry_expectations", return_value={"contracts": {}}) as telemetry:
            result = pipeline_health.collect_facts(self.config, self.now)
        self.probes = [call.args[0] for call in requests.call_args_list
                       if call.args[0].endswith("/live-feeds/status.json")]
        self.telemetry_calls = telemetry.call_args_list
        return result

    def test_active_bindings_share_one_probe_without_replacing_consumer_product_identity(self) -> None:
        facts = self.collect()
        self.assertEqual(self.probes, [self.routes["production"] + "/live-feeds/status.json",
                                      self.routes["staging"] + "/live-feeds/status.json"])
        result = pipeline_health.evaluate_health(facts, [], self.now)
        for scope in ["production", "release-old", "release-older"]:
            self.assertEqual(metric(result, f"channel.{scope}.release.live_feed_status")["severity"], "ok")
            provider = facts["channels"][scope]["live_feed_provider"]
            self.assertEqual(provider["instance_id"], "prod-instance")
            self.assertEqual(provider["verification"], "a" * 64)
        self.assertEqual(metric(result, "live_feed.active_sse_clients")["value"], 9)
        old_call = next(call for call in self.telemetry_calls if call.args[0]["tag"] == "old")
        self.assertEqual(old_call.args[1], self.observed["releases"]["old"])
        old = facts["channels"]["release-old"]
        self.assertIn("releases/old/packages", old["inputs"]["current_artifacts"]["path"])
        self.assertEqual(old["release_state"]["live_feed_status"], "stopped")

    def test_same_tag_new_instance_does_not_replace_the_still_bound_instance(self) -> None:
        self.observed["live_feed_instances"]["prod-redundant"] = {
            "instance_id": "prod-redundant", "release_tag": "prod", "status": "failed",
            "endpoint": "http://127.0.0.1:8109",
        }
        facts = self.collect()
        self.assertNotIn("http://127.0.0.1:8109/live-feeds/status.json", self.probes)
        self.assertEqual(facts["channels"]["production"]["live_feed_provider"]["instance_id"], "prod-instance")

    def test_failed_shared_probe_is_attributed_to_all_dependents_but_not_staging(self) -> None:
        result = pipeline_health.evaluate_health(self.collect(failed_probe=True), [], self.now)
        self.assertEqual(len(self.probes), 2)
        for scope in ["production", "release-old", "release-older"]:
            name = f"channel.{scope}.input.live_feeds_status.available"
            self.assertTrue(any(alert["metric_id"] == name for alert in result["alerts"]))
        self.assertEqual(metric(result, "channel.staging.input.live_feeds_status.available")["severity"], "ok")

    def test_missing_bound_instance_is_a_failure_without_fallback_to_release_status(self) -> None:
        del self.observed["live_feed_instances"]["prod-instance"]
        for record in self.observed["releases"].values():
            record["live_feed_status"] = "running"
        result = pipeline_health.evaluate_health(self.collect(), [], self.now)
        for scope in ["production", "release-old", "release-older"]:
            item = metric(result, f"channel.{scope}.release.live_feed_status")
            self.assertEqual(item["severity"], "critical")
            self.assertIn("absent from observed state", item["message"])

    def test_instance_identity_mismatch_is_a_failure_even_if_endpoint_answers(self) -> None:
        self.observed["live_feed_instances"]["prod-instance"]["endpoint"] = "http://127.0.0.1:8109"
        result = pipeline_health.evaluate_health(self.collect(), [], self.now)
        self.assertEqual(metric(result, "channel.release-old.release.live_feed_status")["severity"], "critical")
        self.assertIn("identity disagrees", metric(result, "channel.production.release.live_feed_status")["message"])

    def test_dedicated_sunset_uses_own_instance_and_resolution_reason(self) -> None:
        binding = self.bindings["releases"]["old"]
        binding.update(provider="old-instance", provider_tag="old", endpoint="http://127.0.0.1:8102",
                       reason="dedicated: NOTAM catalog differs")
        self.routes["releases"]["old"] = binding["endpoint"]
        self.observed["live_feed_instances"]["old-instance"] = {
            "instance_id": "old-instance", "release_tag": "old", "endpoint": binding["endpoint"], "status": "running",
        }
        result = pipeline_health.evaluate_health(self.collect(), [], self.now)
        self.assertEqual(len(self.probes), 3)
        item = metric(result, "channel.release-old.release.live_feed_status")
        self.assertEqual(item["severity"], "ok")
        self.assertIn("NOTAM catalog differs", item["message"])
        self.assertIn("dedicated to old", item["message"])

    def test_new_generation_missing_sidecar_fails_closed(self) -> None:
        self.write_metadata()
        (self.generation / "live-feed-bindings.json").unlink()
        sources, status = pipeline_health.release_channel_sources(self.config)
        self.assertEqual(sources, [])
        self.assertIn("live-feed-bindings.json", status["error"])

    def test_invalid_binding_schema_does_not_select_legacy_routes(self) -> None:
        for version in [None, True, 2, "1"]:
            with self.subTest(version=version):
                self.bindings["schema_version"] = version
                self.write_metadata()
                sources, status = pipeline_health.release_channel_sources(self.config)
                self.assertEqual(sources, [])
                self.assertIn("binding schema", status["error"])

    def test_incomplete_binding_table_fails_closed(self) -> None:
        del self.bindings["releases"]["older"]
        self.write_metadata()
        sources, status = pipeline_health.release_channel_sources(self.config)
        self.assertEqual(sources, [])
        self.assertIn("do not match", status["error"])

    def test_staging_cannot_share_production(self) -> None:
        self.bindings["releases"]["stage"].update(provider="prod-instance", provider_tag="prod")
        self.write_metadata()
        sources, status = pipeline_health.release_channel_sources(self.config)
        self.assertEqual(sources, [])
        self.assertIn("invalid shared", status["error"])

    def test_binding_route_disagreement_fails_closed(self) -> None:
        self.bindings["releases"]["old"]["endpoint"] = "http://127.0.0.1:8199"
        self.write_metadata()
        sources, status = pipeline_health.release_channel_sources(self.config)
        self.assertEqual(sources, [])
        self.assertIn("disagrees with active route", status["error"])

    def test_production_release_alias_must_match_its_bound_provider(self) -> None:
        self.routes["releases"]["prod"] = "http://127.0.0.1:8199"
        self.write_metadata()
        sources, status = pipeline_health.release_channel_sources(self.config)
        self.assertEqual(sources, [])
        self.assertIn("disagrees with active route", status["error"])


class LiveFeedProviderMetricsTests(unittest.TestCase):
    now = datetime(2026, 9, 13, tzinfo=timezone.utc)

    def channel(self) -> dict:
        return {
            "tag": "old", "role": "sunset",
            "release_state": {
                "build_status": "passed", "qualification_status": "passed",
                "deployment_status": "passed", "live_feed_status": "stopped",
            },
            "live_feed_provider": {
                "instance_id": "prod-catalog-a", "release_tag": "prod", "status": "running",
                "endpoint": "http://127.0.0.1:8100", "reason": "compatible",
                "launch_digest": "a" * 64, "manifest_sha256": "b" * 64, "error": None,
            },
        }

    def metrics(self, channel: dict) -> dict:
        items = []
        pipeline_health.add_channel_release_metrics(items, channel, self.now)
        return {item["id"]: item for item in items}

    def test_shared_sunset_does_not_expect_its_redundant_daemon_running(self) -> None:
        channel = self.channel()
        item = self.metrics(channel)["release.live_feed_status"]
        self.assertEqual(item["severity"], "ok")
        self.assertEqual(item["value"], "running")
        self.assertIn("shared with prod", item["message"])
        self.assertIn("compatible", item["message"])
        self.assertEqual(item["details"]["provider"], channel["live_feed_provider"])
        self.assertEqual(channel["release_state"]["live_feed_status"], "stopped")

    def test_shared_provider_failure_is_not_hidden_by_consumer_daemon_state(self) -> None:
        channel = self.channel()
        channel["release_state"]["live_feed_status"] = "running"
        channel["live_feed_provider"]["status"] = "failed"
        item = self.metrics(channel)["release.live_feed_status"]
        self.assertEqual(item["severity"], "critical")
        self.assertEqual(item["value"], "failed")

    def test_provider_identity_error_is_critical_even_when_instance_is_running(self) -> None:
        channel = self.channel()
        channel["live_feed_provider"]["error"] = "binding endpoint does not match observed instance"
        item = self.metrics(channel)["release.live_feed_status"]
        self.assertEqual(item["severity"], "critical")
        self.assertIn("binding endpoint does not match observed instance", item["message"])

    def test_sharing_does_not_suppress_consumer_release_checks(self) -> None:
        channel = self.channel()
        channel["release_state"].update(
            build_status="failed", deployment_status="failed", product_refresh_status="failed",
            product_refresh_error="sunset product publication failed",
        )
        items = self.metrics(channel)
        self.assertEqual(items["release.build_status"]["severity"], "critical")
        self.assertEqual(items["release.qualification_status"]["severity"], "critical")
        self.assertEqual(items["release.product_refresh"]["severity"], "warning")
        self.assertEqual(items["release.live_feed_status"]["severity"], "ok")


class LiveFeedClientMetricsTests(unittest.TestCase):
    now = datetime(2026, 9, 11, 12, 0, tzinfo=timezone.utc)
    name = "live_feed.active_sse_clients"

    def channel(self, count: object, port: int = 8100, role: str = "production", schema: int = 3) -> dict:
        return {
            "role": role,
            "tag": f"tag-{port}",
            "inputs": {
                "current_artifacts": {"payload": [], "error": None},
                "product_facts": [],
                "live_feeds_status": {
                    "url": f"http://127.0.0.1:{port}/live-feeds/status.json",
                    "error": None,
                    "payload": {
                        "schema_version": schema,
                        "active_sse_clients": count,
                        "products": {},
                        "product_policies": [],
                    },
                },
            },
        }

    def facts(self, channels: dict) -> dict:
        return {
            **calendar_facts([]),
            "sampled_at_utc": pipeline_health.iso_utc(self.now),
            "channels": channels,
        }

    def test_counts_are_scoped_and_total_deduplicates_daemon_aliases(self) -> None:
        facts = self.facts({
            "production": self.channel(7),
            "staging": self.channel(2, 8101, "staging", schema=2),
            "release-old": self.channel(3, 8102, "sunset"),
            "release-alias": self.channel(7, role="sunset"),
        })
        result = pipeline_health.evaluate_health(facts, [], self.now)
        total = metric(result, self.name)
        self.assertEqual(total["value"], 12)
        self.assertEqual(total["scope"], "global")
        for scope, expected in [("production", 7), ("staging", 2), ("release-old", 3), ("release-alias", 7)]:
            item = metric(result, f"channel.{scope}.{self.name}")
            self.assertEqual(item["value"], expected)
            self.assertEqual(item["scope"], scope)
            self.assertEqual(item["release_tag"], facts["channels"][scope]["tag"])
        for item in result["metrics"]:
            if item["id"].endswith(self.name):
                self.assertEqual(item["severity"], "ok")
                self.assertEqual(item["unit"], "connections")
                self.assertNotIn("warning_threshold", item)
                self.assertNotIn("critical_threshold", item)
        self.assertFalse(any(alert["metric_id"].endswith(self.name) for alert in result["alerts"]))

    def test_zero_and_large_counts_are_informational_in_all_supported_schemas(self) -> None:
        for schema in [2, 3, 4]:
            for count in [0, 100_000]:
                with self.subTest(schema=schema, count=count):
                    facts = self.facts({"production": self.channel(count, schema=schema)})
                    result = pipeline_health.evaluate_health(facts, [], self.now)
                    for name in [self.name, f"channel.production.{self.name}"]:
                        item = metric(result, name)
                        self.assertEqual(item["value"], count)
                        self.assertEqual(item["severity"], "ok")

    def test_missing_invalid_and_unknown_schema_counts_are_not_zero(self) -> None:
        sources = []
        for invalid in [None, True, False, -1, 1.5, "2", [], {}]:
            sources.append(self.channel(invalid)["inputs"]["live_feeds_status"])
        missing = self.channel(0)["inputs"]["live_feeds_status"]
        del missing["payload"]["active_sse_clients"]
        sources.append(missing)
        for schema in [1, 5, None, "3", 3.0, "4", 4.0]:
            sources.append(self.channel(2, schema=schema)["inputs"]["live_feeds_status"])
        sources.extend([None, {}, {"payload": []}, {"payload": None, "error": "offline"}])
        stale = self.channel(5)["inputs"]["live_feeds_status"]
        stale["error"] = "fetch failed"
        sources.append(stale)
        for source in sources:
            with self.subTest(source=source):
                self.assertIsNone(pipeline_health.live_feed_client_count(source))

    def test_partial_total_stays_unknown_and_does_not_add_an_alarm(self) -> None:
        for problem in ["offline", "missing_count", "invalid_count", "missing_url", "conflicting_alias", "channel_discovery"]:
            with self.subTest(problem=problem):
                old = self.channel(3, 8101, "sunset")
                source = old["inputs"]["live_feeds_status"]
                if problem == "offline":
                    source.update(payload=None, error="connection refused")
                elif problem == "missing_count":
                    del source["payload"]["active_sse_clients"]
                elif problem == "invalid_count":
                    source["payload"]["active_sse_clients"] = -1
                elif problem == "missing_url":
                    source.pop("url")
                elif problem == "conflicting_alias":
                    source["url"] = self.channel(7)["inputs"]["live_feeds_status"]["url"]
                facts = self.facts({"production": self.channel(7), "release-old": old})
                if problem == "channel_discovery":
                    facts["inputs"]["release_channels"] = {"error": "unreadable generation"}
                result = pipeline_health.evaluate_health(facts, [], self.now)
                total = metric(result, self.name)
                self.assertIsNone(total["value"])
                self.assertEqual(total["severity"], "unknown")
                self.assertEqual(metric(result, f"channel.production.{self.name}")["value"], 7)
                self.assertNotIn(self.name, pipeline_health.compact_evaluation_metrics(result))
                self.assertFalse(any(alert["metric_id"].endswith(self.name) for alert in result["alerts"]))
                if problem == "offline":
                    self.assertTrue(any(alert["metric_id"] == "channel.release-old.input.live_feeds_status.available"
                                        for alert in result["alerts"]))

    def test_absent_or_malformed_channels_do_not_report_an_empty_server(self) -> None:
        for channels in [{}, {"production": None}, {"production": {"inputs": None}}]:
            result = pipeline_health.evaluate_health(self.facts(channels), [], self.now)
            self.assertIsNone(metric(result, self.name)["value"])

    def test_standalone_count_uses_the_same_metric_without_a_duplicate_total(self) -> None:
        facts = {**calendar_facts([]), "inputs": {
            **calendar_facts([])["inputs"], **self.channel(4)["inputs"],
        }}
        result = pipeline_health.evaluate_health(facts, [], self.now)
        self.assertEqual(metric(result, self.name)["value"], 4)
        self.assertEqual(sum(item["id"] == self.name for item in result["metrics"]), 1)

    def test_collector_reuses_each_daemon_snapshot_only_within_one_sample(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            config = SimpleNamespace(
                artifact_root=root, data_root=root, deploy_health_path=root / "health.json",
                cloud_status_secret_path=root / "secret", cloud_status_url="http://cloud/status",
                build_watch_url="http://build/status", calendar_path=root / "calendar.json",
            )
            sources = [
                {"id": role, "role": role, "tag": role, "deployment_managed": False,
                 "current_artifacts_path": root / "current.json",
                 "live_feeds_endpoint": f"http://127.0.0.1:{port}{suffix}"}
                for role, port, suffix in [("production", 8100, ""), ("sunset", 8100, "/"), ("staging", 8101, "")]
            ]
            for failure in [False, True]:
                with self.subTest(failure=failure):
                    snapshots = iter([
                        (None, "offline") if failure else ({"schema_version": 3, "active_sse_clients": 7}, None),
                        ({"schema_version": 3, "active_sse_clients": 2}, None),
                        ({"schema_version": 3, "active_sse_clients": 4}, None),
                        ({"schema_version": 3, "active_sse_clients": 1}, None),
                    ])

                    def fetch(url: str, **_kwargs: object) -> tuple:
                        return next(snapshots) if url.endswith("/live-feeds/status.json") else ({}, None)

                    with patch.object(pipeline_health, "release_channel_sources", return_value=(sources, {})), \
                         patch.object(pipeline_health, "cloud_status_authorization", return_value=("unused", None)), \
                         patch.object(pipeline_health, "fetch_json_url", side_effect=fetch) as mocked:
                        first = pipeline_health.collect_facts(config, self.now)
                        second = pipeline_health.collect_facts(config, self.now + timedelta(minutes=1))
                    self.assertEqual(
                        sum(call.args[0].endswith("/live-feeds/status.json") for call in mocked.call_args_list), 4,
                    )
                    for sample in [first, second]:
                        self.assertEqual(sample["channels"]["production"]["inputs"]["live_feeds_status"],
                                         sample["channels"]["sunset"]["inputs"]["live_feeds_status"])
                    self.assertEqual(pipeline_health.live_feed_client_count(
                        first["channels"]["production"]["inputs"]["live_feeds_status"]), None if failure else 7)
                    self.assertEqual(pipeline_health.live_feed_client_count(
                        second["channels"]["production"]["inputs"]["live_feeds_status"]), 4)

    def test_shared_provider_failure_alerts_every_dependent_release_from_one_probe(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            current = root / "current.json"
            current.write_text("[]")
            config = SimpleNamespace(
                artifact_root=root, data_root=root, deploy_health_path=root / "health.json",
                cloud_status_secret_path=root / "secret", cloud_status_url="http://cloud/status",
                build_watch_url="http://build/status", calendar_path=root / "calendar.json",
            )
            sources = [
                {"id": scope, "role": role, "tag": scope, "deployment_managed": False,
                 "current_artifacts_path": current,
                 "live_feeds_endpoint": f"http://127.0.0.1:{port}"}
                for scope, role, port in [
                    ("production", "production", 8100), ("release-old", "sunset", 8100),
                    ("release-older", "sunset", 8100), ("staging", "staging", 8101),
                ]
            ]

            def fetch(url: str, **_kwargs: object) -> tuple:
                if url == "http://127.0.0.1:8100/live-feeds/status.json":
                    return None, "connection refused"
                return {}, None

            with patch.object(pipeline_health, "release_channel_sources", return_value=(sources, {})), \
                 patch.object(pipeline_health, "cloud_status_authorization", return_value=("unused", None)), \
                 patch.object(pipeline_health, "fetch_json_url", side_effect=fetch) as mocked:
                facts = pipeline_health.collect_facts(config, self.now)
            self.assertEqual(
                [call.args[0] for call in mocked.call_args_list
                 if call.args[0].endswith("/live-feeds/status.json")],
                ["http://127.0.0.1:8100/live-feeds/status.json",
                 "http://127.0.0.1:8101/live-feeds/status.json"],
            )
            result = pipeline_health.evaluate_health(facts, [], self.now)
        for scope in ["production", "release-old", "release-older"]:
            name = f"channel.{scope}.input.live_feeds_status.available"
            self.assertEqual(metric(result, name)["severity"], "critical")
            self.assertTrue(any(alert["metric_id"] == name and alert["scope"] == scope
                                for alert in result["alerts"]))
        self.assertEqual(metric(result, "channel.staging.input.live_feeds_status.available")["severity"], "ok")

    def test_counts_round_trip_through_history_and_graph_with_peaks_and_gaps(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = pipeline_health.history_path_for_date(root, self.now.date())
            for minute, count in [(0, 0), (1, 7), (2, 3), (5, None), (10, 2)]:
                sampled = self.now + timedelta(minutes=minute)
                facts = self.facts({"production": self.channel(count)})
                facts["sampled_at_utc"] = pipeline_health.iso_utc(sampled)
                result = pipeline_health.evaluate_health(facts, [], sampled)
                record = pipeline_health.compact_history_record(facts, result)
                pipeline_health.append_history(path, record)
            records = pipeline_health.read_history(root, now=sampled).records
        self.assertEqual(len(records), 5)
        series = pipeline_health.compact_metric_series(records, now=sampled)
        for name in [self.name, f"channel.production.{self.name}"]:
            self.assertEqual(series["series"][name]["first"], [0, None, 2])
            self.assertEqual(series["series"][name]["last"], [3, None, 2])
            self.assertEqual(series["series"][name]["min"], [0, None, 2])
            self.assertEqual(series["series"][name]["max"], [7, None, 2])
            self.assertEqual(series["series"][name]["severity"], [0, None, 0])


if __name__ == "__main__":
    unittest.main()
