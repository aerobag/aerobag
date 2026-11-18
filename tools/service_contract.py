#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Read immutable Rust-owned service contracts; never infer an old client's contract."""
from __future__ import annotations

import json
import re
from pathlib import Path

DIRECTORY = Path("crates/product-contracts/contracts")
CURRENT = DIRECTORY / "service-contract.json"
NAME = re.compile(r"C[1-9][0-9]*\Z")


def validate(value: object) -> dict:
    if not isinstance(value, dict) or set(value) != {
        "schema_version", "contract", "client_data", "live_feeds", "behavior",
    }:
        raise ValueError("missing or malformed service contract")
    if type(value["schema_version"]) is not int or value["schema_version"] != 1:
        raise ValueError("unsupported service contract descriptor")
    if not isinstance(value["contract"], str) or not NAME.fullmatch(value["contract"]):
        raise ValueError("invalid service contract name")
    if not all(isinstance(value[key], dict) and value[key] for key in ("client_data", "live_feeds")):
        raise ValueError("service contract inventories are missing")
    behavior = value["behavior"]
    if (not isinstance(behavior, list) or not behavior
            or any(not isinstance(item, str) or not item for item in behavior)
            or len(set(behavior)) != len(behavior)):
        raise ValueError("invalid service behavior inventory")
    return value


def load(root: Path, *, required: bool = True) -> dict | None:
    source = root / CURRENT
    if not source.exists() and not required:
        return None
    value = validate(json.loads(source.read_text(encoding="utf-8")))
    frozen = root / DIRECTORY / "service" / f"{value['contract']}.json"
    if value != validate(json.loads(frozen.read_text(encoding="utf-8"))):
        raise ValueError("service contract differs from its immutable definition")
    for key, filename in (("client_data", "client-data-contracts.json"),
                          ("live_feeds", "live-feed-compatibility.json")):
        if value[key] != json.loads((root / DIRECTORY / filename).read_text(encoding="utf-8")):
            raise ValueError(f"{key} inventory changed; publish a new service contract")
    return value


def require_equal(client: object, server: object) -> dict:
    client, server = validate(client), validate(server)
    if client != server:
        raise ValueError("client and server service contracts differ; stage a new client release")
    return server


def root_url(origin: str, name: str) -> str:
    if not NAME.fullmatch(name):
        raise ValueError("invalid service contract name")
    return f"{origin.rstrip('/')}/contracts/{name}"
