#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Fail-closed comparison of authoritative publication and direct-daemon evidence.

The Rust-generated descriptor owns both the complete inventory topology and
payload versions. Python validates its shape and compares supplied evidence;
it never reconstructs old release contracts or hashes airport catalogs.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any


CONTRACTS_ROOT = Path(__file__).resolve().parents[1] / "crates/product-contracts/contracts"
WIRE_DESCRIPTOR_SCHEMA_VERSION = 1
COMPATIBILITY_EVIDENCE_SCHEMA_VERSION = 1


def _load_contract(name: str) -> Any:
    try:
        return json.loads((CONTRACTS_ROOT / name).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


WIRE_INVENTORY = _load_contract("live-feed-compatibility.json")
CLIENT_INVENTORY = _load_contract("client-data-contracts.json")


class CompatibilityEvidenceError(ValueError):
    pass


@dataclass(frozen=True)
class CompatibilityDecision:
    compatible: bool
    reason: str


def _object(value: Any, keys: set[str], context: str) -> dict[str, Any]:
    if not isinstance(value, dict) or value.keys() != keys:
        raise CompatibilityEvidenceError(f"{context} is missing, malformed, incomplete or unknown")
    return value


def _string(value: Any, context: str) -> None:
    if not isinstance(value, str) or not value.strip():
        raise CompatibilityEvidenceError(f"{context} must be a non-empty string")


def _sha256(value: Any, context: str) -> None:
    if not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{64}", value):
        raise CompatibilityEvidenceError(f"{context} must be lowercase SHA-256 hex")


def _same_shape(value: Any, expected: Any, context: str) -> None:
    if isinstance(expected, dict):
        document = _object(value, set(expected), context)
        for key, child in expected.items():
            _same_shape(document[key], child, f"{context}.{key}")
    elif type(expected) is int:
        if type(value) is not int or not 0 < value <= 2**32 - 1:
            raise CompatibilityEvidenceError(f"{context} must be a positive u32")
    elif isinstance(expected, str):
        _string(value, context)
    else:
        raise CompatibilityEvidenceError(f"unsupported generated inventory shape at {context}")


def validate_wire_inventory(value: Any) -> None:
    if not isinstance(WIRE_INVENTORY, dict):
        raise CompatibilityEvidenceError("Rust-owned complete wire inventory is unavailable")
    if WIRE_INVENTORY.get("schema_version") != WIRE_DESCRIPTOR_SCHEMA_VERSION:
        raise CompatibilityEvidenceError("unsupported Rust-owned wire inventory descriptor schema")
    _same_shape(value, WIRE_INVENTORY, "wire inventory")
    if value["schema_version"] != WIRE_DESCRIPTOR_SCHEMA_VERSION:
        raise CompatibilityEvidenceError("unknown wire inventory descriptor schema")


def validate_catalog_identity(value: Any) -> None:
    document = _object(value, {"schema_version", "sha256", "airport_count"}, "NOTAM catalog identity")
    try:
        schema = CLIENT_INVENTORY["nav_db"]["required_exact_keys"]["airport/notam-catalog"]
    except (KeyError, TypeError) as error:
        raise CompatibilityEvidenceError("Rust-owned catalog schema is unavailable") from error
    if type(document["schema_version"]) is not int or document["schema_version"] != schema:
        raise CompatibilityEvidenceError("unknown NOTAM catalog schema")
    _sha256(document["sha256"], "NOTAM catalog fingerprint")
    count = document["airport_count"]
    if type(count) is not int or not 0 < count <= 2**64 - 1:
        raise CompatibilityEvidenceError("NOTAM catalog airport count must be a positive u64")


def compare_wire_and_catalog(
    client_wire: Any, required_catalog: Any, producer_wire: Any, loaded_catalog: Any,
) -> CompatibilityDecision:
    try:
        validate_wire_inventory(client_wire)
        validate_wire_inventory(producer_wire)
        validate_catalog_identity(required_catalog)
        validate_catalog_identity(loaded_catalog)
    except CompatibilityEvidenceError as error:
        return CompatibilityDecision(False, str(error))
    if client_wire != producer_wire:
        return CompatibilityDecision(False, "live-feed wire contracts differ")
    if required_catalog != loaded_catalog:
        return CompatibilityDecision(False, "NOTAM catalog differs")
    return CompatibilityDecision(True, "complete wire inventory and NOTAM catalog match")


def _evidence_schema(document: dict[str, Any]) -> None:
    if (
        type(document["schema_version"]) is not int
        or document["schema_version"] != COMPATIBILITY_EVIDENCE_SCHEMA_VERSION
    ):
        raise CompatibilityEvidenceError("unknown compatibility evidence schema")


def _startup_publication(value: Any) -> None:
    source = _object(value, {"path", "sha256"}, "startup publication")
    _string(source["path"], "startup publication path")
    if not Path(source["path"]).is_absolute() or ".." in Path(source["path"]).parts:
        raise CompatibilityEvidenceError("startup publication path must be absolute without traversal")
    _sha256(source["sha256"], "startup publication identity")


def validate_runtime_envelope(value: Any) -> dict[str, Any]:
    """Validate runtime evidence without treating sharing readiness as liveness."""
    actual = _object(value, {
        "schema_version", "executable_sha256", "release_tag", "launch_instance_id",
        "process_instance_id", "wire_contracts", "configured_products", "notam_catalog",
        "startup_publication", "projection_ready", "published_state_id", "ready",
    }, "daemon compatibility metadata")
    _evidence_schema(actual)
    _sha256(actual["executable_sha256"], "executable identity")
    _string(actual["process_instance_id"], "process instance identity")
    for name in ("release_tag", "launch_instance_id", "published_state_id"):
        if actual[name] is not None:
            _string(actual[name], name)
    for name in ("ready", "projection_ready"):
        if type(actual[name]) is not bool:
            raise CompatibilityEvidenceError(f"{name} must be a boolean")
    if actual["startup_publication"] is not None:
        _startup_publication(actual["startup_publication"])
    if actual["notam_catalog"] is not None:
        validate_catalog_identity(actual["notam_catalog"])
    validate_wire_inventory(actual["wire_contracts"])
    products = actual["configured_products"]
    if (
        not isinstance(products, list) or any(not isinstance(product, str) for product in products)
        or len(set(products)) != len(products)
        or not set(products) <= set(actual["wire_contracts"]["products"])
    ):
        raise CompatibilityEvidenceError("production product availability is malformed or unknown")
    return actual


def compare_live_feed_compatibility(requirement: Any, provider: Any) -> CompatibilityDecision:
    if requirement is None:
        return CompatibilityDecision(False, "publication compatibility metadata unavailable")
    if provider is None:
        return CompatibilityDecision(False, "production compatibility metadata unavailable")
    try:
        client = _object(requirement, {
            "schema_version", "wire_contracts", "notam_catalog", "startup_publication",
        }, "publication compatibility metadata")
        _evidence_schema(client)
        _startup_publication(client["startup_publication"])
        actual = validate_runtime_envelope(provider)
        _startup_publication(actual["startup_publication"])
        _string(actual["release_tag"], "release identity")
        _string(actual["launch_instance_id"], "launch instance identity")
        if actual["ready"] is not True or actual["projection_ready"] is not True:
            raise CompatibilityEvidenceError("production compatibility projection is not ready")
        _string(actual["published_state_id"], "published projection state")
        validate_wire_inventory(client["wire_contracts"])
        products = actual["configured_products"]
        if set(products) != set(client["wire_contracts"]["products"]):
            raise CompatibilityEvidenceError("required live-feed products are unavailable")
    except CompatibilityEvidenceError as error:
        return CompatibilityDecision(False, str(error))
    return compare_wire_and_catalog(
        client["wire_contracts"], client["notam_catalog"], actual["wire_contracts"], actual["notam_catalog"],
    )


def verification_evidence(evidence: Any) -> Any:
    """Snapshot compatibility facts, excluding ordinary product-version advances."""
    if not isinstance(evidence, dict):
        return evidence
    stable = {key: value for key, value in evidence.items() if key != "published_state_id"}
    state = evidence.get("published_state_id")
    stable["published_state_present"] = isinstance(state, str) and bool(state.strip())
    return stable
