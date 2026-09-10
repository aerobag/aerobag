# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import copy
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch
from datetime import datetime, timezone

sys.path.insert(0, str(Path(__file__).resolve().parent))
import telemetry_contracts as contracts
import pipeline_health

ROOT = contracts.default_catalog_root()
OLD_TAG = "2026-08-30.3"
OLD_COMMIT = "196a6319fa435dbb570f27e2a515d45ca8242668"
NEW_TAG = "2026-09-08.1"
NEW_COMMIT = "59dba72490c8a60889395e43cda40b29dd0e9494"
FUTURE_COMMIT = "a" * 40


def pin(version: int) -> dict:
    name = f"product-facts-v{version}"
    return {"id": name, "sha256": hashlib.sha256((ROOT / f"{name}.json").read_bytes()).hexdigest()}


def expectation(version: int) -> dict:
    selected = pin(version)
    return {"pin": selected, "contract": contracts.load_contract(selected, "product-facts")}


def payload(count: int = 974, *, version: int = 3) -> dict:
    result = {"schema_version": 1, "products": [{
        "product_id": "NAV_DB_TEST_2609_01", "family": "nav-db", "cycle": "2609",
        "error_count": 0, "warning_count": 153, "weather_camera_site_count": count,
    }]}
    if version == 3:
        result["telemetry_contract"] = pin(version)
    return result


def evaluate_product(document: dict, version: int, role: str = "sunset") -> list[dict]:
    channel = {
        "role": role,
        "inputs": {"product_facts": [{"payload": document}]},
        "telemetry": {"contracts": {"product-facts": expectation(version)}, "error": None},
    }
    metrics = []
    pipeline_health.add_product_fact_metrics(metrics, channel, [])
    pipeline_health.apply_telemetry_contracts(metrics, channel)
    return metrics


def metric(metrics: list[dict], name: str) -> dict:
    return next(value for value in metrics if value["id"] == name)


class TelemetryContractsTests(unittest.TestCase):
    def test_legacy_bindings_are_exact_and_do_not_relabel_published_releases(self):
        for tag, commit, version in [(OLD_TAG, OLD_COMMIT, 1), (NEW_TAG, NEW_COMMIT, 2)]:
            with self.subTest(tag=tag):
                release = {"tag": tag, "commit": commit}
                original = copy.deepcopy(release)
                resolved = contracts.expected_contracts(release, tag=tag, commit=commit)
                self.assertEqual(resolved["product-facts"], expectation(version))
                self.assertEqual(release, original)
        legacy = contracts.read_object(ROOT / "legacy-releases.json")["releases"]
        self.assertEqual(len(legacy), 18)
        with self.assertRaisesRegex(ValueError, "not a registered legacy release"):
            contracts.legacy_pins(OLD_COMMIT, "some-new-tag", ROOT)

    def test_unknown_or_missing_identity_never_selects_legacy(self):
        for release, commit in [
            ({"tag": "new", "commit": FUTURE_COMMIT}, FUTURE_COMMIT),
            ({"tag": "new", "commit": FUTURE_COMMIT, "telemetry_contracts": {}}, FUTURE_COMMIT),
            ({"tag": "new", "commit": OLD_COMMIT}, FUTURE_COMMIT),
            ({"tag": "new", "commit": FUTURE_COMMIT}, ""),
        ]:
            with self.subTest(release=release, commit=commit), self.assertRaises(ValueError):
                contracts.expected_contracts(release, tag="new", commit=commit)

    def test_release_pin_and_digest_are_checked(self):
        metadata = {"tag": "new", "commit": FUTURE_COMMIT, "telemetry_contracts": {"product-facts": pin(3)}}
        self.assertEqual(contracts.expected_contracts(metadata, tag="new", commit=FUTURE_COMMIT)["product-facts"], expectation(3))
        for bad in [{"id": "../escape", "sha256": "0" * 64},
                    {"id": "product-facts-v3", "sha256": "0" * 64},
                    {"id": "unknown-v1", "sha256": "0" * 64}, None]:
            metadata["telemetry_contracts"]["product-facts"] = bad
            with self.subTest(pin=bad), self.assertRaises(ValueError):
                contracts.expected_contracts(metadata, tag="new", commit=FUTURE_COMMIT)
        metadata = {"tag": NEW_TAG, "commit": NEW_COMMIT, "telemetry_contracts": {"product-facts": pin(1)}}
        with self.assertRaisesRegex(ValueError, "disagree"):
            contracts.expected_contracts(metadata, tag=NEW_TAG, commit=NEW_COMMIT)

    def test_build_reads_the_producer_worktree_not_controller_head(self):
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            self.assertEqual(contracts.build_pins(repo, OLD_TAG, OLD_COMMIT), {"product-facts": pin(1)})
            with self.assertRaisesRegex(ValueError, "not a registered legacy release"):
                contracts.build_pins(repo, "new", FUTURE_COMMIT)
            shutil.copytree(ROOT, repo / "contracts/telemetry")
            self.assertEqual(contracts.build_pins(repo, "new", FUTURE_COMMIT), {"product-facts": pin(3)})
            source = repo / "contracts/telemetry/producers.json"
            source.write_text(json.dumps({"schema_version": 1, "producers": {"product-facts": pin(1)}}))
            with self.assertRaisesRegex(ValueError, "coverage removed"):
                contracts.build_pins(repo, "new", FUTURE_COMMIT)

    def test_only_known_unsupported_measurement_is_neutral_in_every_channel(self):
        document = payload(version=1)
        del document["products"][0]["weather_camera_site_count"]
        for role in ["production", "staging", "sunset"]:
            with self.subTest(role=role):
                metrics = evaluate_product(document, 1, role)
                camera = metric(metrics, "cycle_product.weather_camera_site_count")
                self.assertEqual(camera["severity"], "not_instrumented")
                self.assertIsNone(camera["value"])
                self.assertNotIn("warning_threshold", camera)
                self.assertIn("Not instrumented", camera["message"])
                self.assertEqual(metric(metrics, "cycle_product.warning_count")["value"], 153)

    def test_promised_missing_and_invalid_fields_alarm_including_sunset(self):
        for name in ["weather_camera_site_count", "warning_count", "error_count"]:
            for value in [None, "974", True, -1, 1.5]:
                document = payload()
                document["products"][0][name] = value
                with self.subTest(name=name, value=value):
                    result = metric(evaluate_product(document, 3), f"cycle_product.{name}")
                    self.assertEqual(result["severity"], "warning")
                    self.assertIsNone(result["value"])
                    self.assertIn("Required telemetry", result["message"])

    def test_wrong_or_removed_claim_cannot_downgrade_promises(self):
        for claim in [None, pin(1), pin(2), {"id": "product-facts-v3", "sha256": "0" * 64}]:
            with self.subTest(claim=claim):
                document = payload()
                if claim is None:
                    del document["telemetry_contract"]
                else:
                    document["telemetry_contract"] = claim
                metrics = evaluate_product(document, 3)
                self.assertEqual(metric(metrics, "telemetry.product-facts.contract")["severity"], "warning")
                self.assertEqual(metric(metrics, "cycle_product.weather_camera_site_count")["severity"], "unknown")

    def test_legacy_without_claim_still_has_required_camera_measurement(self):
        document = payload(version=2)
        del document["products"][0]["weather_camera_site_count"]
        result = metric(evaluate_product(document, 2), "cycle_product.weather_camera_site_count")
        self.assertEqual(result["severity"], "warning")

    def test_valid_counts_use_monitor_thresholds_not_producer_thresholds(self):
        for count, severity in [(974, "ok"), (960, "ok"), (959, "warning"), (0, "warning")]:
            document = payload(count)
            document["minimum_weather_camera_site_count"] = 0
            camera = metric(evaluate_product(document, 3), "cycle_product.weather_camera_site_count")
            self.assertEqual(camera["severity"], severity)
            self.assertEqual(camera["warning_threshold"], 960)

    def test_envelope_and_partial_publication_failures_do_not_look_healthy(self):
        good = payload()
        for bad in [None, {}, {"schema_version": 1, "products": []},
                    {**good, "products": [None]}, {**good, "schema_version": 2}]:
            with self.subTest(bad=bad):
                errors, _ = contracts.check_payloads(expectation(3), [{"payload": good}, {"payload": bad}])
                self.assertTrue(errors)
        errors, _ = contracts.check_payloads(expectation(3), [])
        self.assertTrue(errors)

    def test_new_measurements_use_the_generic_gate(self):
        expected = expectation(3)
        expected["contract"]["measurements"]["cycle_product.future_count"] = {
            "field": "future_count", "family": "nav-db", "type": "nonnegative-integer",
            "unit": "count", "meaning": "A future feature's measurement.",
        }
        with patch.dict(pipeline_health.PRODUCT_METRIC_REQUIREMENTS, {
            "cycle_product.future_count": ("product-facts", "cycle_product.future_count"),
        }):
            channel = {"inputs": {"product_facts": [{"payload": payload()}]},
                       "telemetry": {"contracts": {"product-facts": expected}}}
            metrics = [{"id": "cycle_product.future_count", "value": 0, "severity": "ok"}]
            pipeline_health.apply_telemetry_contracts(metrics, channel)
            self.assertEqual(metrics[0]["severity"], "warning")
            del expected["contract"]["measurements"]["cycle_product.future_count"]
            pipeline_health.apply_telemetry_contracts(metrics[:1], channel)
            self.assertEqual(metrics[0]["severity"], "not_instrumented")

    def test_evaluation_keeps_neutral_rows_out_of_alerts_and_history_values(self):
        document = payload(version=1)
        del document["products"][0]["weather_camera_site_count"]
        channel = {
            "role": "sunset", "tag": OLD_TAG, "deployment_managed": True,
            "release_state": {"build_status": "passed", "deployment_status": "passed", "live_feed_status": "running"},
            "inputs": {"product_facts": [{"payload": document}], "live_feeds_status": {"payload": None}},
            "telemetry": {"contracts": {"product-facts": expectation(1)}},
        }
        evaluation = pipeline_health.evaluate_health(
            {"inputs": {"build_watch": {"payload": None}}, "channels": {"old": channel}}, [], datetime(2026, 9, 10, tzinfo=timezone.utc),
        )
        name = "channel.old.cycle_product.weather_camera_site_count"
        self.assertEqual(metric(evaluation["metrics"], name)["severity"], "not_instrumented")
        self.assertNotIn(name, [alert["metric_id"] for alert in evaluation["alerts"]])
        self.assertNotIn(name, pipeline_health.compact_evaluation_metrics(evaluation))
        del channel["telemetry"]
        evaluation = pipeline_health.evaluate_health(
            {"inputs": {"build_watch": {"payload": None}}, "channels": {"old": channel}}, [], datetime(2026, 9, 10, tzinfo=timezone.utc),
        )
        self.assertEqual(metric(evaluation["metrics"], name)["severity"], "unknown")
        self.assertIn("channel.old.telemetry.product-facts.contract", [alert["metric_id"] for alert in evaluation["alerts"]])

    def test_missing_release_receipt_is_coverage_alarm_not_legacy(self):
        with tempfile.TemporaryDirectory() as temporary:
            context = pipeline_health.collect_telemetry_expectations({"tag": "new"}, {
                "commit": FUTURE_COMMIT, "release_root": temporary,
            })
        self.assertTrue(context["error"])
        metrics = [{"id": "cycle_product.weather_camera_site_count", "value": 974, "severity": "ok"}]
        pipeline_health.apply_telemetry_contracts(metrics, {"telemetry": context})
        self.assertEqual(metrics[0]["severity"], "unknown")
        self.assertEqual(metrics[1]["severity"], "warning")

    def test_malformed_catalog_is_reported_as_a_coverage_error(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            catalog = root / "catalog"
            shutil.copytree(ROOT, catalog)
            (root / "release.json").write_text(json.dumps({"tag": OLD_TAG, "commit": OLD_COMMIT}))
            for registry in [None, [], {}, {OLD_COMMIT: None}, {OLD_COMMIT: {"tags": "not-a-list"}}]:
                (catalog / "legacy-releases.json").write_text(json.dumps({"schema_version": 1, "releases": registry}))
                with self.subTest(registry=registry), patch.object(contracts, "default_catalog_root", return_value=catalog):
                    context = pipeline_health.collect_telemetry_expectations({"tag": OLD_TAG}, {
                        "commit": OLD_COMMIT, "release_root": str(root),
                    })
                    self.assertIn("invalid legacy telemetry", context["error"])
                    self.assertEqual(context["contracts"], {})

    def test_installed_helper_uses_the_installed_catalog(self):
        with patch.object(contracts, "__file__", "/usr/local/bin/telemetry_contracts.py"):
            self.assertEqual(contracts.default_catalog_root(), Path("/etc/aerobag/telemetry"))

    def test_coverage_removal_needs_exact_explicit_reviewed_exception(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "telemetry"
            shutil.copytree(ROOT, root)
            (root / "producers.json").write_text(json.dumps({"schema_version": 1, "producers": {"product-facts": pin(1)}}))
            policy = contracts.read_object(root / "coverage-policy.json")
            for exceptions in [[], [{"reason": "intentional", "approved_by": "operator"}]]:
                policy["exceptions"] = exceptions
                (root / "coverage-policy.json").write_text(json.dumps(policy))
                with self.assertRaisesRegex(ValueError, "explicit reviewed"):
                    contracts.validate_coverage(root)
            policy["exceptions"] = [{
                "producer": "product-facts", "from": pin(3), "to": pin(1),
                "measurements": ["contract_claim", "cycle_product.weather_camera_site_count"],
                "reason": "Explicit test-only removal", "approved_by": "test operator",
            }]
            (root / "coverage-policy.json").write_text(json.dumps(policy))
            contracts.validate_coverage(root)

    def test_changed_semantics_and_removed_producer_count_as_coverage_loss(self):
        before = expectation(3)["contract"]
        after = copy.deepcopy(before)
        after["measurements"]["cycle_product.warning_count"]["family"] = "nav-db"
        self.assertEqual(contracts.coverage_losses(before, after), ["cycle_product.warning_count"])
        self.assertIn("contract_claim", contracts.coverage_losses(before, None))


if __name__ == "__main__":
    unittest.main()
