# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

from datetime import datetime, timezone
import json
from pathlib import Path
import tempfile
import unittest

import pipeline_health
import telemetry_contracts


class ArchiveHealthTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.now = datetime(2026, 10, 9, 12, tzinfo=timezone.utc)
        self.payload = {
            "schema_version": 1,
            "telemetry_contract": telemetry_contracts.producer_pins(telemetry_contracts.default_catalog_root())["product-archive"],
            "attempted_at_utc": "2026-10-09T12:00:00Z", "failure_count": 0,
            "capture_gap_count": 0, "buffer_bytes": 1024, "repository_bytes": 2048,
        }

    def metrics(self, payload):
        for worker in ("collect", "store"):
            (self.root / f"{worker}.json").write_text(json.dumps(payload))
        facts = {"inputs": {"product_archive": pipeline_health.collect_archive_facts(self.root)}}
        metrics = []
        pipeline_health.add_archive_metrics(metrics, facts, self.now)
        return {m["id"]: m for m in metrics}

    def test_valid_status_and_persistent_gap_alarm(self):
        metrics = self.metrics(self.payload)
        self.assertTrue(all(m["severity"] == "ok" for m in metrics.values()))
        metrics = self.metrics({**self.payload, "capture_gap_count": 1})
        self.assertEqual(metrics["product_archive.collect.capture_gap_count"]["severity"], "critical")

    def test_missing_invalid_and_unclaimed_telemetry_are_coverage_errors(self):
        for field in ("failure_count", "capture_gap_count", "telemetry_contract", "schema_version"):
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


if __name__ == "__main__":
    unittest.main()
