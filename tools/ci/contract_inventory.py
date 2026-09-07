# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any


class ContractInventoryError(ValueError):
    pass


@dataclass(frozen=True)
class PublicationContracts:
    current_manifest_schema: int
    bundle_manifest_schema: int


@dataclass(frozen=True)
class LiveFeedContracts:
    manifest_schema: int
    product_contracts: dict[str, int]


@dataclass(frozen=True)
class FixtureClientContracts:
    publication: PublicationContracts
    package_contracts: dict[str, str]
    live_feeds: LiveFeedContracts | None


@dataclass(frozen=True)
class ClientContractInventory:
    publication: PublicationContracts
    package_contracts: dict[str, str]
    live_feeds: LiveFeedContracts


def _positive_integer(value: Any, label: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or value < 1:
        raise ContractInventoryError(f"{label} must be a positive integer")
    return value


def _object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ContractInventoryError(f"{label} must be an object")
    return value


def _publication(value: Any, label: str) -> PublicationContracts:
    raw = _object(value, label)
    if set(raw) != {"current_manifest_schema", "bundle_manifest_schema"}:
        raise ContractInventoryError(f"{label} has unexpected or missing fields")
    return PublicationContracts(
        current_manifest_schema=_positive_integer(
            raw["current_manifest_schema"], f"{label}.current_manifest_schema"
        ),
        bundle_manifest_schema=_positive_integer(
            raw["bundle_manifest_schema"], f"{label}.bundle_manifest_schema"
        ),
    )


def _package_contracts(value: Any, label: str) -> dict[str, str]:
    raw = _object(value, label)
    if not raw or not all(
        isinstance(family, str)
        and family
        and isinstance(contract, str)
        and contract
        for family, contract in raw.items()
    ):
        raise ContractInventoryError(f"{label} must map family names to contract IDs")
    return dict(sorted(raw.items()))


def _live_feeds(value: Any, label: str) -> LiveFeedContracts:
    raw = _object(value, label)
    if set(raw) != {"manifest_schema", "product_contracts"}:
        raise ContractInventoryError(f"{label} has unexpected or missing fields")
    product_values = _object(raw["product_contracts"], f"{label}.product_contracts")
    products = {
        product: _positive_integer(contract, f"{label}.product_contracts.{product}")
        for product, contract in product_values.items()
        if isinstance(product, str) and product
    }
    if len(products) != len(product_values):
        raise ContractInventoryError(f"{label}.product_contracts has an invalid product name")
    return LiveFeedContracts(
        manifest_schema=_positive_integer(
            raw["manifest_schema"], f"{label}.manifest_schema"
        ),
        product_contracts=dict(sorted(products.items())),
    )


def parse_fixture_client_contracts(
    value: Any,
    label: str,
) -> FixtureClientContracts:
    raw = _object(value, label)
    allowed = {"publication", "package_contracts", "live_feeds"}
    if not {"publication", "package_contracts"}.issubset(raw) or not set(raw) <= allowed:
        raise ContractInventoryError(f"{label} has unexpected or missing fields")
    return FixtureClientContracts(
        publication=_publication(raw["publication"], f"{label}.publication"),
        package_contracts=_package_contracts(
            raw["package_contracts"], f"{label}.package_contracts"
        ),
        live_feeds=(
            _live_feeds(raw["live_feeds"], f"{label}.live_feeds")
            if "live_feeds" in raw
            else None
        ),
    )


def load_client_contract_inventory(path: Path) -> ClientContractInventory:
    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ContractInventoryError(f"cannot read client contract inventory {path}: {error}") from error
    root = _object(raw, "client contract inventory")
    if root.get("schema_version") != 1:
        raise ContractInventoryError("client contract inventory schema_version must be 1")
    expected = {"schema_version", "publication", "package_contracts", "live_feeds", "nav_db"}
    if set(root) != expected:
        raise ContractInventoryError("client contract inventory has unexpected or missing fields")
    packages = _package_contracts(root["package_contracts"], "client package_contracts")
    nav_db = _object(root["nav_db"], "client nav_db")
    if nav_db.get("contract_id") != packages.get("nav-db"):
        raise ContractInventoryError("NAVDB descriptor and package contract disagree")
    return ClientContractInventory(
        publication=_publication(root["publication"], "client publication"),
        package_contracts=packages,
        live_feeds=_live_feeds(root["live_feeds"], "client live_feeds"),
    )


def assert_compatible(
    required: ClientContractInventory,
    offered: FixtureClientContracts,
    label: str,
) -> None:
    if offered.publication != required.publication:
        raise ContractInventoryError(
            f"{label} provides publication schemas {offered.publication}; "
            f"client requires {required.publication}"
        )
    for family, contract in offered.package_contracts.items():
        required_contract = required.package_contracts.get(family)
        if required_contract != contract:
            raise ContractInventoryError(
                f"{label} provides {family} contract {contract}; "
                f"client requires {required_contract or 'no such family'}"
            )
    if offered.live_feeds is None:
        return
    if offered.live_feeds.manifest_schema != required.live_feeds.manifest_schema:
        raise ContractInventoryError(
            f"{label} provides live-feed schema {offered.live_feeds.manifest_schema}; "
            f"client requires {required.live_feeds.manifest_schema}"
        )
    for product, contract in offered.live_feeds.product_contracts.items():
        required_contract = required.live_feeds.product_contracts.get(product)
        if required_contract != contract:
            raise ContractInventoryError(
                f"{label} provides live-feed {product} contract {contract}; "
                f"client requires {required_contract or 'no product contract'}"
            )
