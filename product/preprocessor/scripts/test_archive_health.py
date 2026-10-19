# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import pipeline_health
import telemetry_contracts


class ArchiveHealthTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.now = datetime(2026, 10, 9, 12, tzinfo=timezone.utc)
        self.payload = {
            "schema_version": 2,
            "telemetry_contract": telemetry_contracts.producer_pins(telemetry_contracts.default_catalog_root())["product-archive"],
            "attempted_at_utc": "2026-10-09T12:00:00Z", "failure_count": 0,
            "capture_gap_count": 0, "buffer_bytes": 1024, "repository_bytes": 2048,
            "state": "succeeded", "phase": "complete",
            "progress_reference_unix_seconds": int(self.now.timestamp()),
        }

    def metrics(self, payload):
        for worker in ("collect", "store"):
            (self.root / f"{worker}.json").write_text(json.dumps(payload))
        facts = {"inputs": {"product_archive": pipeline_health.collect_archive_facts(self.root)}}
        metrics = []
        pipeline_health.add_archive_metrics(metrics, facts, self.now)
        return {m["id"]: m for m in metrics}

    def test_recovered_gaps_are_history_not_active_alarms(self):
        metrics = self.metrics(self.payload)
        self.assertTrue(all(m["severity"] == "ok" for m in metrics.values()))
        metrics = self.metrics({**self.payload, "capture_gap_count": 1})
        self.assertTrue(all(m["severity"] == "ok" for m in metrics.values()))
        self.assertEqual(metrics["product_archive.collect.capture_gap_count"]["value"], 1)

    def test_progress_thresholds_clear_on_recovery_without_acknowledging_history(self):
        for state in ("succeeded", "running", "deferred", "failed"):
            for silent in (False, True):
                for age, collect_severity, store_severity in (
                    (899, "ok", "ok"), (900, "warning", "ok"),
                    (1799, "warning", "ok"), (1800, "warning", "warning"),
                    (3599, "warning", "warning"), (3600, "critical", "critical"),
                ):
                    with self.subTest(state=state, silent=silent, age=age):
                        reference = self.now - timedelta(seconds=age)
                        payload = {**self.payload, "state": state, "capture_gap_count": 2,
                                   "failure_count": int(state == "failed"),
                                   "progress_reference_unix_seconds": int(reference.timestamp()),
                                   "attempted_at_utc": reference.isoformat() if silent else self.payload["attempted_at_utc"]}
                        metrics = self.metrics(payload)
                        for worker, severity, warning in (("collect", collect_severity, 900),
                                                          ("store", store_severity, 1800)):
                            metric = metrics[f"product_archive.{worker}.progress_age_seconds"]
                            self.assertEqual(metric["severity"], severity)
                            self.assertEqual(metric["warning_threshold"], warning)
                            self.assertEqual(metric["critical_threshold"], 3600)
                        recovered = self.metrics({**self.payload, "capture_gap_count": 2})
                        self.assertTrue(all(m["severity"] == "ok" for m in recovered.values()))

    def test_sampling_accepts_reports_written_during_collection_but_not_after_it(self):
        config = SimpleNamespace(health_root=self.root / "health")
        completed_at = self.now + timedelta(milliseconds=250)
        for state in ("running", "succeeded"):
            for offset_ms, severity in ((200, "ok"), (500, "critical")):
                with self.subTest(state=state, offset_ms=offset_ms):
                    clock = self.now
                    reported_at = self.now + timedelta(milliseconds=offset_ms)

                    def collect(_config, started_at):
                        nonlocal clock
                        self.assertEqual(started_at, self.now)
                        payload = {**self.payload, "state": state,
                                   "attempted_at_utc": pipeline_health.iso_utc(reported_at)}
                        for worker in ("collect", "store"):
                            (self.root / f"{worker}.json").write_text(json.dumps(payload))
                        archive = pipeline_health.collect_archive_facts(self.root)
                        clock = completed_at
                        return {"schema_version": 2, "sampled_at_utc": pipeline_health.iso_utc(started_at),
                                "channels": {}, "inputs": {"product_archive": archive, "build_watch": {}}}

                    with patch.object(pipeline_health, "utc_now", side_effect=lambda: clock), \
                         patch.object(pipeline_health, "collect_facts", side_effect=collect):
                        result = pipeline_health.run_sample(config)
                    metrics = {m["id"]: m for m in result["evaluation"]["metrics"]}
                    for worker in ("collect", "store"):
                        self.assertEqual(metrics[f"product_archive.{worker}.coverage"]["severity"], severity)
                    self.assertEqual(result["evaluation"]["generated_at_utc"],
                                     pipeline_health.iso_utc(completed_at))

    def test_missing_invalid_and_unclaimed_telemetry_are_coverage_errors(self):
        for field in ("failure_count", "capture_gap_count", "telemetry_contract", "schema_version",
                      "state", "phase", "progress_reference_unix_seconds"):
            with self.subTest(field=field):
                payload = dict(self.payload)
                del payload[field]
                metrics = self.metrics(payload)
                self.assertEqual(metrics["product_archive.collect.coverage"]["severity"], "critical")
                self.assertNotIn("product_archive.collect.failure_count", metrics)
        for value in (-1, None, True, "0"):
            metrics = self.metrics({**self.payload, "buffer_bytes": value})
            self.assertEqual(metrics["product_archive.collect.coverage"]["severity"], "critical")

    def test_stale_status_alerts_and_explicitly_disabled_has_no_metrics(self):
        metrics = self.metrics({**self.payload, "attempted_at_utc": "2026-10-08T00:00:00Z"})
        self.assertEqual(metrics["product_archive.store.coverage"]["severity"], "critical")
        metrics = []
        pipeline_health.add_archive_metrics(metrics, {"inputs": {"product_archive": None}}, self.now)
        self.assertEqual(metrics, [])

    def test_malformed_envelope_and_timestamp_cannot_crash_the_monitor(self):
        for payload in (None, [], 7, {**self.payload, "attempted_at_utc": 7}):
            with self.subTest(payload=payload):
                metrics = self.metrics(payload)
                self.assertEqual(metrics["product_archive.collect.coverage"]["severity"], "critical")

    def test_deferred_is_visible_without_failure_or_false_success(self):
        metrics = self.metrics({**self.payload, "state": "deferred", "phase": "release-reconciler.lock busy"})
        self.assertTrue(all(m["severity"] == "ok" for m in metrics.values()))
        self.assertIn("deferred", metrics["product_archive.store.activity"]["message"])
        self.assertIn("release-reconciler.lock", metrics["product_archive.store.activity"]["message"])

    def test_fresh_reports_cannot_hide_one_hour_without_completion(self):
        for state in ("deferred", "running", "failed"):
            with self.subTest(state=state):
                payload = {**self.payload, "state": state,
                           "progress_reference_unix_seconds": int(self.now.timestamp()) - 3600}
                metrics = self.metrics(payload)
                self.assertEqual(metrics["product_archive.store.coverage"]["severity"], "ok")
                self.assertEqual(metrics["product_archive.store.progress_age_seconds"]["severity"], "critical")

    def test_progress_age_advances_while_worker_is_silent(self):
        metrics = self.metrics({**self.payload, "state": "running", "attempted_at_utc": "2026-10-09T11:00:00Z",
                                "progress_reference_unix_seconds": int(self.now.timestamp()) - 7200})
        self.assertEqual(metrics["product_archive.store.progress_age_seconds"]["value"], 7200)

    def test_real_failure_is_not_hidden_by_running_or_deferred_state(self):
        for state in ("running", "deferred", "failed"):
            metrics = self.metrics({**self.payload, "state": state, "failure_count": 1, "error": "disk full",
                                    "progress_reference_unix_seconds": int(self.now.timestamp()) - 3600})
            failure = metrics["product_archive.store.failure_count"]
            self.assertEqual(failure["severity"], "ok")
            self.assertEqual(failure["message"], "disk full")
            progress = metrics["product_archive.store.progress_age_seconds"]
            self.assertEqual(progress["severity"], "critical")
            self.assertIn("disk full", progress["message"])

    def test_old_contract_and_invalid_activity_are_not_accepted(self):
        old_pin = {"id": "product-archive-v1", "sha256": "1955a0d664acd800a4ff312f0948d0d5fe20e88bb4b8376ffe4bbbd5c176c61a"}
        for changes in ({"schema_version": 1, "telemetry_contract": old_pin},
                        {"state": "made up"}, {"phase": []}, {"state": []},
                        {"progress_reference_unix_seconds": int(self.now.timestamp()) + 3600}):
            with self.subTest(changes=changes):
                metrics = self.metrics({**self.payload, **changes})
                self.assertEqual(metrics["product_archive.store.coverage"]["severity"], "critical")


if __name__ == "__main__":
    unittest.main()
