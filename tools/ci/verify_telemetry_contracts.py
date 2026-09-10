#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Verify immutable telemetry definitions and explicitly reviewed coverage losses."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "product/preprocessor/scripts"))
import telemetry_contracts as contracts  # noqa: E402
import pipeline_health  # noqa: E402

CATALOG = "contracts/telemetry"


def git(repo: Path, *args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=repo, text=True).strip()


def verify(repo: Path, base_ref: str = "HEAD") -> None:
    root = repo / CATALOG
    contracts.validate_coverage(root)
    current = contracts.producer_pins(root)
    for producer, pin in current.items():
        descriptor = contracts.load_contract(pin, producer, root)
        registered = {measurement for owner, measurement in pipeline_health.PRODUCT_METRIC_REQUIREMENTS.values()
                      if owner == producer}
        unmonitored = set(descriptor["measurements"]) - registered
        if unmonitored:
            raise ValueError(f"promised measurements have no monitoring rules: {sorted(unmonitored)}")

    # Inspect the actual base, not a mutable lock regenerated from current files.
    # HEAD compares a dirty working tree; hosted CI supplies the push/PR base SHA.
    names = git(repo, "ls-tree", "-r", "--name-only", base_ref, "--", CATALOG).splitlines()
    for name in names:
        if not name.endswith(".json"):
            continue
        before = subprocess.check_output(["git", "show", f"{base_ref}:{name}"], cwd=repo)
        document = json.loads(before)
        if "id" in document or name.endswith("/legacy-releases.json"):
            path = repo / name
            if not path.is_file() or path.read_bytes() != before:
                raise ValueError(f"published telemetry definition is immutable: {name}; add a new contract ID instead")
        elif name.endswith("/producers.json"):
            contracts.validate_coverage(root, document["producers"])
        elif name.endswith("/coverage-policy.json"):
            contracts.validate_coverage(root, document["required"])

    for commit, entry in contracts.legacy_registry(root).items():
        for tag in entry["tags"]:
            contracts.legacy_pins(commit, tag, root)

    # Also compare with production: a series of local commits or a stale main
    # checkout must not evade the release boundary's coverage check.
    desired = json.loads((repo / "deploy/releases.json").read_text())
    tag = desired["production"]["tag"]
    commit = git(repo, "rev-parse", f"refs/tags/{tag}^{{commit}}")
    source = f"{CATALOG}/producers.json"
    if git(repo, "ls-tree", "--name-only", commit, "--", source):
        previous = json.loads(git(repo, "show", f"{commit}:{source}"))["producers"]
    else:
        previous = contracts.legacy_pins(commit, tag, root)
    contracts.validate_coverage(root, previous)
    print("Telemetry contracts verified: immutable definitions, release pins, monitoring coverage")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-ref", default="HEAD")
    args = parser.parse_args()
    try:
        verify(ROOT, args.base_ref)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"Telemetry contract verification failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
