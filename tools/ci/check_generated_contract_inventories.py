#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Regenerate Rust contract inventories in isolation and check every required file."""

from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
CONTRACTS = Path("crates/product-contracts/contracts")
OUTPUTS = ("client-data-contracts.json", "live-feed-compatibility.json")


def compare(generated: Path, checked_in: Path) -> list[str]:
    errors = []
    for name in OUTPUTS:
        actual, expected = generated / name, checked_in / name
        if not actual.is_file():
            errors.append(f"generator omitted required inventory: {name}")
        if not expected.is_file():
            errors.append(f"checked-in inventory is missing: {expected}")
        elif actual.is_file() and actual.read_bytes() != expected.read_bytes():
            errors.append(f"checked-in inventory is stale: {expected}")
    if generated.is_dir():
        for member in sorted(generated.iterdir()):
            if member.name not in OUTPUTS or not member.is_file():
                errors.append(f"generator produced unexpected inventory member: {member.name}")
    return errors


def check(repo_root: Path) -> list[str]:
    with tempfile.TemporaryDirectory(prefix="aerobag-generated-contracts-") as directory:
        temporary = Path(directory)
        output = temporary / "contracts"
        empty_artifacts = temporary / "no-artifacts"
        empty_artifacts.mkdir()
        subprocess.run([
            "cargo", "+1.94.1", "run", "--quiet", "--locked",
            "--manifest-path", str(repo_root / "crates/Cargo.toml"),
            "-p", "product-contracts", "--bin", "export-contract-inventory", "--",
            "--output-dir", str(output),
        ], cwd=repo_root, check=True, env={
            **os.environ, "AEROBAG_ARTIFACT_READ_PATH": str(empty_artifacts),
        })
        return compare(output, repo_root / CONTRACTS)


def main() -> int:
    errors = check(ROOT)
    if errors:
        print("Generated contract inventories do not match:\n" + "\n".join(errors))
        return 1
    print("Both Rust-generated client and live-feed contract inventories match")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
