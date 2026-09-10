# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Producer telemetry promises, resolved independently of publication payloads.

The catalog is controller configuration; release.json pins a producer's promise.
Neither missing fields nor a payload's own contract claim select that promise.
"""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path
from typing import Any


def default_catalog_root() -> Path:
    script = Path(__file__).resolve()
    if script.parent.name == "scripts":
        return script.parents[3] / "contracts/telemetry"
    return Path("/etc/aerobag/telemetry")


def read_object(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise ValueError(f"cannot read telemetry metadata {path}: {error}") from error
    if not isinstance(value, dict) or type(value.get("schema_version")) is not int or value["schema_version"] != 1:
        raise ValueError(f"invalid telemetry metadata {path}")
    return value


def load_contract(pin: Any, producer: str, root: Path | None = None) -> dict[str, Any]:
    root = root or default_catalog_root()
    if (
        not isinstance(pin, dict) or set(pin) != {"id", "sha256"}
        or not isinstance(pin.get("id"), str)
        or not re.fullmatch(r"[a-z][a-z0-9-]*", pin["id"])
        or not isinstance(pin.get("sha256"), str)
        or not re.fullmatch(r"[0-9a-f]{64}", pin["sha256"])
    ):
        raise ValueError(f"invalid {producer} telemetry contract pin")
    path = root / f"{pin['id']}.json"
    document = read_object(path)
    if hashlib.sha256(path.read_bytes()).hexdigest() != pin["sha256"]:
        raise ValueError(f"telemetry contract digest mismatch: {pin['id']}")
    if document.get("id") != pin["id"] or document.get("producer") != producer:
        raise ValueError(f"telemetry contract identity mismatch: {pin['id']}")
    if type(document.get("requires_claim")) is not bool:
        raise ValueError(f"missing claim policy: {pin['id']}")
    if type(document.get("payload_schema_version")) is not int:
        raise ValueError(f"missing payload schema: {pin['id']}")
    measurements = document.get("measurements")
    if not isinstance(measurements, dict) or not measurements:
        raise ValueError(f"missing measurements: {pin['id']}")
    for name, measurement in measurements.items():
        if (
            not isinstance(measurement, dict)
            or measurement.get("type") != "nonnegative-integer"
            or any(not isinstance(measurement.get(key), str) or not measurement[key]
                   for key in ("field", "family", "unit", "meaning"))
        ):
            raise ValueError(f"invalid measurement definition: {pin['id']}/{name}")
    return document


def producer_pins(root: Path) -> dict[str, Any]:
    pins = read_object(root / "producers.json").get("producers")
    if not isinstance(pins, dict) or not pins:
        raise ValueError("missing producer telemetry contract pins")
    for producer, pin in pins.items():
        load_contract(pin, producer, root)
    return pins


def legacy_registry(root: Path) -> dict[str, Any]:
    releases = read_object(root / "legacy-releases.json").get("releases")
    if not isinstance(releases, dict) or not releases:
        raise ValueError("invalid legacy telemetry registry")
    for commit, entry in releases.items():
        if (
            not re.fullmatch(r"[0-9a-f]{40}", commit)
            or not isinstance(entry, dict)
            or not isinstance(entry.get("tags"), list) or not entry["tags"]
            or any(not isinstance(tag, str) or not tag for tag in entry["tags"])
            or not isinstance(entry.get("producers"), dict) or not entry["producers"]
        ):
            raise ValueError(f"invalid legacy telemetry binding: {commit}")
    return releases


def legacy_pins(commit: str, tag: str, root: Path) -> dict[str, Any]:
    releases = legacy_registry(root)
    entry = releases.get(commit)
    if not isinstance(entry, dict) or tag not in entry.get("tags", []):
        raise ValueError(f"no telemetry contract for producer {tag} ({commit}); not a registered legacy release")
    pins = entry.get("producers")
    if not isinstance(pins, dict) or not pins:
        raise ValueError(f"invalid legacy telemetry binding: {commit}")
    for producer, pin in pins.items():
        load_contract(pin, producer, root)
    return pins


def build_pins(repo_root: Path, tag: str, commit: str, catalog_root: Path | None = None) -> dict[str, Any]:
    """Read the selected producer worktree, never the controller's current pins."""
    source = repo_root / "contracts/telemetry"
    if (source / "producers.json").exists():
        validate_coverage(source)
        pins = producer_pins(source)
        # The deployed monitor must understand the producer before we build it.
        for producer, pin in pins.items():
            load_contract(pin, producer, catalog_root)
        return pins
    return legacy_pins(commit, tag, catalog_root or default_catalog_root())


def expected_contracts(
    release: Any, *, tag: str, commit: str, root: Path | None = None,
) -> dict[str, Any]:
    root = root or default_catalog_root()
    if not isinstance(commit, str) or not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("missing or invalid deployed producer commit")
    if not isinstance(release, dict) or release.get("tag") != tag or release.get("commit") != commit:
        raise ValueError("release metadata does not match the deployed producer identity")
    pins = release.get("telemetry_contracts")
    legacy = legacy_registry(root).get(commit)
    if legacy is not None:
        known = legacy_pins(commit, tag, root)
        if "telemetry_contracts" in release and pins != known:
            raise ValueError("release telemetry pins disagree with the exact legacy producer binding")
        pins = known
    elif not isinstance(pins, dict) or not pins:
        raise ValueError("release has no pinned telemetry contracts")
    return {
        producer: {"pin": pin, "contract": load_contract(pin, producer, root)}
        for producer, pin in pins.items()
    }


def check_payloads(expected: dict[str, Any], entries: list[dict[str, Any]]) -> tuple[list[str], dict[str, list[str]]]:
    """Return envelope errors and per-measurement violations; never infer support."""
    contract, pin = expected["contract"], expected["pin"]
    errors: list[str] = []
    invalid: dict[str, list[str]] = {}
    if not entries:
        errors.append("no product-facts publications")
    for entry in entries:
        payload = entry.get("payload") if isinstance(entry, dict) else None
        if not isinstance(payload, dict):
            errors.append("product-facts payload unavailable")
            continue
        if type(payload.get("schema_version")) is not int or payload["schema_version"] != contract["payload_schema_version"]:
            errors.append("product-facts payload schema mismatch")
        if (contract["requires_claim"] or "telemetry_contract" in payload) and payload.get("telemetry_contract") != pin:
            errors.append(f"publication telemetry claim missing or mismatched; expected {pin['id']}")
        products = payload.get("products")
        if not isinstance(products, list) or not products:
            errors.append("product-facts products missing or empty")
            continue
        for product in products:
            if not isinstance(product, dict) or not isinstance(product.get("family"), str):
                errors.append("product-facts product identity missing or invalid")
                continue
            for measurement_id, measurement in contract["measurements"].items():
                if measurement["family"] not in {"*", product["family"]}:
                    continue
                value = product.get(measurement["field"])
                if type(value) is not int or value < 0:
                    invalid.setdefault(measurement_id, []).append(
                        str(product.get("product_id") or product["family"])
                    )
    return sorted(set(errors)), invalid


def coverage_losses(before: dict[str, Any], after: dict[str, Any] | None) -> list[str]:
    """Changed semantics are a loss too; reuse of a metric name is not continuity."""
    if after is None:
        return sorted([*before["measurements"], "contract_claim"] if before["requires_claim"] else before["measurements"])
    losses = [name for name, definition in before["measurements"].items()
              if after["measurements"].get(name) != definition]
    if before["requires_claim"] and not after["requires_claim"]:
        losses.append("contract_claim")
    return sorted(losses)


def validate_coverage(root: Path, required: dict[str, Any] | None = None) -> None:
    current = producer_pins(root)
    policy = read_object(root / "coverage-policy.json")
    required = required if required is not None else policy.get("required")
    if not isinstance(required, dict) or not required:
        raise ValueError("telemetry coverage policy has no required producers")
    exceptions = policy.get("exceptions")
    if not isinstance(exceptions, list):
        raise ValueError("telemetry coverage exceptions must be an explicit list")
    for producer, before_pin in required.items():
        before = load_contract(before_pin, producer, root)
        after_pin = current.get(producer)
        after = load_contract(after_pin, producer, root) if after_pin is not None else None
        losses = coverage_losses(before, after)
        if not losses:
            continue
        approved = any(
            isinstance(exception, dict)
            and exception.get("producer") == producer
            and exception.get("from") == before_pin
            and exception.get("to") == after_pin
            and exception.get("measurements") == losses
            and isinstance(exception.get("reason"), str) and exception["reason"].strip()
            and isinstance(exception.get("approved_by"), str) and exception["approved_by"].strip()
            for exception in exceptions
        )
        if not approved:
            raise ValueError(f"telemetry coverage removed/changed for {producer}: {', '.join(losses)}; requires an explicit reviewed coverage-policy exception")
