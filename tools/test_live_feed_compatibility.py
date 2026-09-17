#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import copy
import sys
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))

import live_feed_compatibility as compatibility


def requirement(publication: str = "client-publication") -> dict:
    return {
        "schema_version": 1,
        "startup_publication": {"path": f"/publications/{publication}.json", "sha256": "c" * 64},
        "wire_contracts": copy.deepcopy(compatibility.WIRE_INVENTORY),
        "notam_catalog": {
            "schema_version": compatibility.CLIENT_INVENTORY["nav_db"]["required_exact_keys"]["airport/notam-catalog"],
            "sha256": "a" * 64, "airport_count": 3,
        },
    }


def provider(tag: str = "prod", publication: str = "startup-publication") -> dict:
    result = requirement(publication)
    result.update({
        "process_instance_id": "process-1",
        "executable_sha256": "d" * 64,
        "release_tag": tag,
        "launch_instance_id": f"{tag}-instance",
        "configured_products": list(result["wire_contracts"]["products"]),
        "projection_ready": True,
        "published_state_id": "state-1",
        "ready": True,
    })
    return result


def leaf_paths(document: dict, path: tuple[str, ...] = ()):
    for key, value in document.items():
        if isinstance(value, dict):
            yield from leaf_paths(value, (*path, key))
        else:
            yield (*path, key)


def parent_at(document: dict, path: tuple[str, ...]) -> dict:
    for key in path[:-1]:
        document = document[key]
    return document


class LiveFeedCompatibilityTests(unittest.TestCase):
    def test_valid_cold_runtime_envelope_is_not_a_sharing_certificate(self) -> None:
        actual = provider()
        actual.update(ready=False, projection_ready=False, published_state_id=None,
                      configured_products=[], notam_catalog=None, startup_publication=None)
        self.assertIs(compatibility.validate_runtime_envelope(actual), actual)
        self.assertFalse(compatibility.compare_live_feed_compatibility(requirement(), actual).compatible)

    def test_runtime_shape_validation_rejects_truthy_readiness_values(self) -> None:
        for name in ("ready", "projection_ready"):
            for value in (1, "true", None):
                with self.subTest(name=name, value=value), self.assertRaises(compatibility.CompatibilityEvidenceError):
                    actual = provider()
                    actual[name] = value
                    compatibility.validate_runtime_envelope(actual)

    def test_complete_exact_inventory_and_catalog_match_across_builds_and_publications(self) -> None:
        result = compatibility.compare_live_feed_compatibility(requirement(), provider())
        self.assertTrue(result.compatible, result.reason)

    def test_every_wire_version_encoding_and_parameter_change_denies_sharing(self) -> None:
        original = provider()
        for path in leaf_paths(original["wire_contracts"]):
            with self.subTest(path=path):
                actual = copy.deepcopy(original)
                parent = parent_at(actual["wire_contracts"], path)
                previous = parent[path[-1]]
                parent[path[-1]] = previous + 1 if type(previous) is int else previous + "-changed"
                self.assertFalse(compatibility.compare_live_feed_compatibility(requirement(), actual).compatible)

    def test_identically_incomplete_inventories_never_approve_sharing(self) -> None:
        original = requirement()
        for path in leaf_paths(original["wire_contracts"]):
            with self.subTest(path=path):
                client = copy.deepcopy(original)
                del parent_at(client["wire_contracts"], path)[path[-1]]
                actual = provider()
                actual["wire_contracts"] = copy.deepcopy(client["wire_contracts"])
                self.assertFalse(compatibility.compare_live_feed_compatibility(client, actual).compatible)

    def test_identically_unknown_inventory_fields_and_products_are_rejected(self) -> None:
        for path in ((), ("protocols",), ("products",), ("products", "notams", "formats")):
            with self.subTest(path=path):
                client = requirement()
                target = client["wire_contracts"]
                for key in path:
                    target = target[key]
                target["unknown"] = {"schema_version": 1, "encoding": "new"}
                actual = provider()
                actual["wire_contracts"] = copy.deepcopy(client["wire_contracts"])
                self.assertFalse(compatibility.compare_live_feed_compatibility(client, actual).compatible)

    def test_matching_legacy_manifest_and_notam_schema_is_insufficient(self) -> None:
        client, actual = requirement(), provider()
        client["wire_contracts"] = actual["wire_contracts"] = {"manifest_schema": 3, "product_contracts": {"notams": 7}}
        self.assertFalse(compatibility.compare_live_feed_compatibility(client, actual).compatible)

    def test_missing_evidence_selects_dedicated_with_specific_reason(self) -> None:
        for client, actual, reason in (
            (None, provider(), "publication compatibility metadata unavailable"),
            (requirement(), None, "production compatibility metadata unavailable"),
        ):
            with self.subTest(reason=reason):
                result = compatibility.compare_live_feed_compatibility(client, actual)
                self.assertFalse(result.compatible)
                self.assertEqual(result.reason, reason)

    def test_malformed_or_unknown_envelopes_are_not_legacy_fallbacks(self) -> None:
        for value in ({}, [], "v3", True, {"schema_version": 1}, {"schema_version": 2}):
            with self.subTest(value=value):
                self.assertFalse(compatibility.compare_live_feed_compatibility(value, provider()).compatible)
                self.assertFalse(compatibility.compare_live_feed_compatibility(requirement(), value).compatible)

    def test_each_required_product_must_be_available(self) -> None:
        for product in provider()["configured_products"]:
            with self.subTest(product=product):
                actual = provider()
                actual["configured_products"].remove(product)
                result = compatibility.compare_live_feed_compatibility(requirement(), actual)
                self.assertFalse(result.compatible)
                self.assertIn("unavailable", result.reason)

    def test_product_availability_is_strict_and_not_truthiness_based(self) -> None:
        for products in (None, {}, "notams", [True], ["unknown"], ["notams", "notams"]):
            with self.subTest(products=products):
                actual = provider()
                actual["configured_products"] = products
                self.assertFalse(compatibility.compare_live_feed_compatibility(requirement(), actual).compatible)

    def test_readiness_requires_exact_true(self) -> None:
        for ready in (False, None, 0, 1, "true", [], {}):
            with self.subTest(ready=ready):
                actual = provider()
                actual["ready"] = ready
                self.assertFalse(compatibility.compare_live_feed_compatibility(requirement(), actual).compatible)

    def test_projection_readiness_cannot_be_bypassed_by_outer_ready_flag(self) -> None:
        for ready in (False, None, 0, 1, "true"):
            with self.subTest(ready=ready):
                actual = provider()
                actual["projection_ready"] = ready
                self.assertFalse(compatibility.compare_live_feed_compatibility(requirement(), actual).compatible)

    def test_executable_process_launch_and_publication_identity_are_required(self) -> None:
        for key in ("executable_sha256", "release_tag", "launch_instance_id", "process_instance_id", "published_state_id"):
            for value in (None, "", " ", True):
                with self.subTest(key=key, value=value):
                    actual = provider()
                    actual[key] = value
                    self.assertFalse(compatibility.compare_live_feed_compatibility(requirement(), actual).compatible)
        for value in (None, {}, {"path": "relative", "sha256": "e" * 64}, {"path": "/file.json", "sha256": "bad"}):
            with self.subTest(source=value):
                client, actual = requirement(), provider()
                client["startup_publication"] = actual["startup_publication"] = value
                self.assertFalse(compatibility.compare_live_feed_compatibility(client, actual).compatible)

    def test_missing_envelope_fields_are_not_filled_from_current_release(self) -> None:
        for make in (requirement, provider):
            for key in make():
                with self.subTest(envelope=make.__name__, key=key):
                    evidence = make()
                    del evidence[key]
                    client, actual = (evidence, provider()) if make is requirement else (requirement(), evidence)
                    self.assertFalse(compatibility.compare_live_feed_compatibility(client, actual).compatible)

    def test_evidence_schema_requires_known_integer(self) -> None:
        for schema in (None, True, 1.0, "1", 0, 2):
            with self.subTest(schema=schema):
                client, actual = requirement(), provider()
                client["schema_version"] = actual["schema_version"] = schema
                self.assertFalse(compatibility.compare_live_feed_compatibility(client, actual).compatible)

    def test_catalog_addition_removal_and_equal_count_substitution_deny_sharing(self) -> None:
        for count in (2, 3, 4):
            with self.subTest(count=count):
                actual = provider()
                actual["notam_catalog"].update(airport_count=count, sha256="b" * 64)
                result = compatibility.compare_live_feed_compatibility(requirement(), actual)
                self.assertFalse(result.compatible)
                self.assertEqual(result.reason, "NOTAM catalog differs")

    def test_inconsistent_count_is_not_ignored_even_with_same_fingerprint(self) -> None:
        actual = provider()
        actual["notam_catalog"]["airport_count"] += 1
        self.assertFalse(compatibility.compare_live_feed_compatibility(requirement(), actual).compatible)

    def test_catalog_schema_rollover_preserves_exact_sunset_comparison(self) -> None:
        for schema in (1, 2):
            with self.subTest(schema=schema):
                client, actual = requirement(), provider()
                client["notam_catalog"]["schema_version"] = schema
                actual["notam_catalog"]["schema_version"] = schema
                result = compatibility.compare_live_feed_compatibility(client, actual)
                self.assertTrue(result.compatible, result.reason)
                actual["notam_catalog"]["schema_version"] = 3 - schema
                self.assertFalse(compatibility.compare_live_feed_compatibility(client, actual).compatible)

    def test_catalog_identity_requires_complete_known_and_well_typed_fields(self) -> None:
        for field, values in {
            "schema_version": [None, True, 1.0, "1", 99],
            "sha256": [None, "", "a" * 63, "A" * 64, "g" * 64],
            "airport_count": [None, 0, -1, True, 3.0, "3", 2**64],
        }.items():
            for value in values:
                with self.subTest(field=field, value=value):
                    client, actual = requirement(), provider()
                    client["notam_catalog"][field] = actual["notam_catalog"][field] = value
                    self.assertFalse(compatibility.compare_live_feed_compatibility(client, actual).compatible)

    def test_inventory_payload_versions_reject_bool_float_zero_and_negative(self) -> None:
        for value in (True, 1.0, 0, -1, 2**32, "3"):
            with self.subTest(value=value):
                client, actual = requirement(), provider()
                for evidence in (client, actual):
                    evidence["wire_contracts"]["protocols"]["discovery"]["schema_version"] = value
                self.assertFalse(compatibility.compare_live_feed_compatibility(client, actual).compatible)

    def test_missing_rust_inventory_cannot_authorize_sharing(self) -> None:
        client, actual = requirement(), provider()
        with mock.patch.object(compatibility, "WIRE_INVENTORY", None):
            result = compatibility.compare_live_feed_compatibility(client, actual)
        self.assertFalse(result.compatible)
        self.assertIn("Rust-owned", result.reason)

    def test_future_exporter_schema_requires_an_explicit_python_reader_update(self) -> None:
        client, actual = requirement(), provider()
        client["wire_contracts"]["schema_version"] = actual["wire_contracts"]["schema_version"] = 2
        with mock.patch.object(compatibility, "WIRE_INVENTORY", client["wire_contracts"]):
            result = compatibility.compare_live_feed_compatibility(client, actual)
        self.assertFalse(result.compatible)
        self.assertIn("unsupported", result.reason)


if __name__ == "__main__":
    unittest.main()
