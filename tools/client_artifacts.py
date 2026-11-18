# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Retain immutable client bytes independently of retired server deployments."""
from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import tempfile
import time

import build_release
import service_contract


def retain(root: Path, release_root: Path) -> Path:
    metadata = json.loads((release_root / "release.json").read_text())
    client = metadata["client"]
    contract = service_contract.validate(json.loads((release_root / "service-contract.json").read_text()))
    build_release.retained_client(release_root, contract)
    destination = root / f"{client['tag']}-{client['commit'][:12]}"
    identity = {"schema_version": 1, "client": client, "service_contract": contract}
    root.mkdir(parents=True, exist_ok=True)
    if not destination.exists():
        temporary = Path(tempfile.mkdtemp(prefix=".client-", dir=root))
        try:
            for kind in ("web", "downloads"):
                shutil.copytree(release_root / kind, temporary / kind, copy_function=os.link)
            (temporary / "client.json").write_text(json.dumps(identity, sort_keys=True) + "\n")
            build_release.normalize_release_permissions(temporary)
            os.replace(temporary, destination)
        finally:
            if temporary.exists():
                shutil.rmtree(temporary)
    if validate(destination) != identity:
        raise ValueError("immutable client identity was reused with different bytes")
    return destination


def validate(path: Path) -> dict:
    build_release.validate_release_permissions(path)
    identity = json.loads((path / "client.json").read_text())
    if identity["schema_version"] != 1:
        raise ValueError("unsupported client artifact identity")
    service_contract.validate(identity["service_contract"])
    client = identity["client"]
    for kind in ("web", "downloads"):
        if build_release.directory_sha256(path / kind) != client["artifacts"][kind]["sha256"]:
            raise ValueError("retained client artifacts changed")
    return identity


def supported(root: Path, definitions: dict[str, dict]) -> dict[str, Path]:
    result = {}
    for path in sorted(root.glob("*/client.json")):
        identity = validate(path.parent)
        descriptor = identity["service_contract"]
        if descriptor["contract"] not in definitions:
            continue
        service_contract.require_equal(definitions[descriptor["contract"]], descriptor)
        tag = identity["client"]["tag"]
        if tag in result:
            raise ValueError("duplicate retained client tag")
        result[tag] = path.parent
    return result


def collect(root: Path, active_contracts: set[str], *, now: float | None = None) -> None:
    """Keep every client for supported contracts; retire others after drain grace."""
    instant = time.time() if now is None else now
    for path in sorted(root.glob("*/client.json")):
        if path.parent.is_symlink() or path.is_symlink():
            raise ValueError("client retention must not follow symlinks")
        identity = validate(path.parent)
        marker = root / ".retirement" / f"{path.parent.name}.json"
        if identity["service_contract"]["contract"] in active_contracts:
            marker.unlink(missing_ok=True)
        elif not marker.exists():
            import release_reconciler
            release_reconciler._write_json_atomic(marker, {"until": instant + 3600})
        elif json.loads(marker.read_text())["until"] <= instant:
            shutil.rmtree(path.parent)
            marker.unlink()
