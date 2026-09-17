#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Cloud contract snapshots are append-only, compared with their first commit."""

from __future__ import annotations

from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
CONTRACTS = "ui/core-rust/crates/app-core/src/cloud/contracts"


def git(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=root, text=True).strip()


def check(root: Path) -> list[str]:
    if git(root, "rev-parse", "--is-shallow-repository") == "true":
        return ["Cloud contract history requires a full checkout (fetch-depth: 0)"]
    history = git(root, "log", "--reverse", "--diff-filter=A", "--format=commit:%H",
                  "--name-only", "HEAD", "--", f"{CONTRACTS}/*.json")
    first_commit: dict[str, str] = {}
    commit = ""
    for line in history.splitlines():
        if line.startswith("commit:"):
            commit = line.removeprefix("commit:")
        elif line.strip():
            first_commit.setdefault(line, commit)
    errors = []
    for name, revision in first_commit.items():
        expected = subprocess.check_output(["git", "show", f"{revision}:{name}"], cwd=root)
        path = root / name
        if not path.is_file() or path.read_bytes() != expected:
            errors.append(f"Published cloud contract changed: {name}; add a new account version instead")
    return errors


def main() -> int:
    errors = check(ROOT)
    if errors:
        print("\n".join(errors))
        return 1
    print("Cloud account contract history is append-only")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
