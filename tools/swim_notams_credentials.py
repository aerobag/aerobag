# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import json
from pathlib import Path
from typing import Any


REQUIRED_SWIM_NOTAM_FIELDS = [
    "providerUrl",
    "queue",
    "connectionFactory",
    "username",
    "password",
    "vpn",
]


def _canonical_environment(record: dict[str, Any]) -> str | None:
    value = record.get("aerobagEnvironment", record.get("aerobag_environment"))
    return str(value) if value is not None else None


def _reject_secret_output_path(path: Path, forbidden_roots: list[Path]) -> None:
    resolved = path.expanduser().resolve()
    for root in forbidden_roots:
        root = root.expanduser().resolve()
        try:
            resolved.relative_to(root)
        except ValueError:
            continue
        raise SystemExit(
            f"refusing to write SWIM NOTAM credential under unsafe root {root}: {resolved}"
        )


def write_environment_credential(
    *,
    bundle_path: Path,
    environment: str,
    output_path: Path,
    forbidden_roots: list[Path],
    dry_run: bool = False,
) -> bool:
    """Extract one SWIM NOTAM credential from an operator-owned environment bundle.

    Returns False when the bundle is absent. The caller can use that to leave
    SWIM disabled in dev setups that have no credentials.
    """

    bundle_path = bundle_path.expanduser().resolve()
    output_path = output_path.expanduser().resolve()
    if not bundle_path.is_file():
        return False
    _reject_secret_output_path(output_path, forbidden_roots)

    payload = json.loads(bundle_path.read_text(encoding="utf-8"))
    subscriptions = payload.get("subscriptions")
    if not isinstance(subscriptions, dict):
        raise SystemExit(f"{bundle_path} must contain a subscriptions object")
    selected = subscriptions.get(environment)
    if not isinstance(selected, dict):
        raise SystemExit(f"{bundle_path} has no subscriptions.{environment} object")

    selected_environment = _canonical_environment(selected)
    if selected_environment is not None and selected_environment != environment:
        raise SystemExit(
            f"{bundle_path} subscriptions.{environment} declares environment "
            f"{selected_environment!r}, expected {environment!r}"
        )

    credential = dict(selected)
    credential.pop("aerobag_environment", None)
    credential["aerobagEnvironment"] = environment
    missing = [
        field
        for field in REQUIRED_SWIM_NOTAM_FIELDS
        if credential.get(field) is None or str(credential.get(field)).strip() == ""
    ]
    if missing:
        raise SystemExit(
            f"{bundle_path} subscriptions.{environment} missing required fields: "
            + ", ".join(missing)
        )

    if dry_run:
        print(
            f"+ would extract SWIM NOTAM {environment} credential "
            f"{bundle_path} -> {output_path}",
            flush=True,
        )
        return True

    output_path.parent.mkdir(parents=True, mode=0o700, exist_ok=True)
    tmp = output_path.with_name(f".{output_path.name}.tmp")
    tmp.write_text(json.dumps(credential, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    tmp.chmod(0o600)
    tmp.replace(output_path)
    output_path.chmod(0o600)
    return True
