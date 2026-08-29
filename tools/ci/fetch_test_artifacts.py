#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Any


COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")


class LockError(ValueError):
    pass


@dataclass(frozen=True)
class Fixture:
    name: str
    path: PurePosixPath
    contract_version: int
    manifest_path: PurePosixPath | None
    manifest_version_field: str | None
    required_globs: tuple[str, ...]


@dataclass(frozen=True)
class Lock:
    repository: str
    commit: str
    fixtures: dict[str, Fixture]


def relative_path(value: Any, label: str) -> PurePosixPath:
    if not isinstance(value, str) or not value:
        raise LockError(f"{label} must be a non-empty string")
    path = PurePosixPath(value)
    if path.is_absolute() or ".." in path.parts or "." in path.parts:
        raise LockError(f"{label} must be a normalized relative path")
    return path


def load_lock(path: Path) -> Lock:
    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise LockError(f"cannot read fixture lock {path}: {error}") from error
    if not isinstance(raw, dict) or raw.get("schema_version") != 1:
        raise LockError("fixture lock schema_version must be 1")
    repository = raw.get("repository")
    if not isinstance(repository, str) or not repository:
        raise LockError("fixture lock repository must be a non-empty string")
    commit = raw.get("commit")
    if not isinstance(commit, str) or not COMMIT_RE.fullmatch(commit):
        raise LockError("fixture lock commit must be a full lowercase Git commit")
    fixture_values = raw.get("fixtures")
    if not isinstance(fixture_values, dict) or not fixture_values:
        raise LockError("fixture lock fixtures must be a non-empty object")

    fixtures: dict[str, Fixture] = {}
    for name, value in fixture_values.items():
        if not isinstance(name, str) or not name or not isinstance(value, dict):
            raise LockError("fixture names must map to objects")
        contract_version = value.get("contract_version")
        if not isinstance(contract_version, int) or contract_version < 1:
            raise LockError(f"fixture {name} contract_version must be a positive integer")
        fixture_path = relative_path(value.get("path"), f"fixture {name} path")
        manifest = value.get("manifest")
        manifest_path = None
        manifest_version_field = None
        if manifest is not None:
            if not isinstance(manifest, dict):
                raise LockError(f"fixture {name} manifest must be an object")
            manifest_path = relative_path(
                manifest.get("path"), f"fixture {name} manifest path"
            )
            manifest_version_field = manifest.get("version_field")
            if not isinstance(manifest_version_field, str) or not manifest_version_field:
                raise LockError(
                    f"fixture {name} manifest version_field must be a non-empty string"
                )
        required_globs_value = value.get("required_globs", [])
        if not isinstance(required_globs_value, list) or not all(
            isinstance(pattern, str) and pattern
            for pattern in required_globs_value
        ):
            raise LockError(f"fixture {name} required_globs must contain strings")
        fixtures[name] = Fixture(
            name=name,
            path=fixture_path,
            contract_version=contract_version,
            manifest_path=manifest_path,
            manifest_version_field=manifest_version_field,
            required_globs=tuple(required_globs_value),
        )
    return Lock(repository=repository, commit=commit, fixtures=fixtures)


def run_git(destination: Path, *arguments: str) -> str:
    environment = os.environ.copy()
    environment["GIT_TERMINAL_PROMPT"] = "0"
    result = subprocess.run(
        ["git", "-C", str(destination), *arguments],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=environment,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(f"git {' '.join(arguments)} failed: {detail}")
    return result.stdout.strip()


def nested_value(value: Any, field: str) -> Any:
    current = value
    for component in field.split("."):
        if not isinstance(current, dict) or component not in current:
            raise LockError(f"manifest does not contain version field {field}")
        current = current[component]
    return current


def validate_fixture(root: Path, fixture: Fixture) -> None:
    fixture_root = root.joinpath(*fixture.path.parts)
    if not fixture_root.is_dir():
        raise LockError(f"fixture {fixture.name} is missing {fixture.path}")
    if fixture.manifest_path is not None:
        manifest_path = fixture_root.joinpath(*fixture.manifest_path.parts)
        try:
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            raise LockError(
                f"cannot read {fixture.name} manifest {manifest_path}: {error}"
            ) from error
        version = nested_value(manifest, fixture.manifest_version_field or "")
        if version != fixture.contract_version:
            raise LockError(
                f"fixture {fixture.name} contract is {version}; "
                f"lock requires {fixture.contract_version}"
            )
    for pattern in fixture.required_globs:
        if not any(fixture_root.glob(pattern)):
            raise LockError(
                f"fixture {fixture.name} has no files matching required glob {pattern}"
            )


def fetch(
    lock: Lock,
    fixture_names: list[str],
    destination: Path,
    repository_cache: Path | None = None,
) -> None:
    unknown = sorted(set(fixture_names) - lock.fixtures.keys())
    if unknown:
        raise LockError(f"unknown fixtures: {', '.join(unknown)}")
    selected = [lock.fixtures[name] for name in dict.fromkeys(fixture_names)]
    if not selected:
        raise LockError("at least one --fixture is required")
    if destination.exists() and any(destination.iterdir()):
        raise LockError(f"destination is not empty: {destination}")
    destination.mkdir(parents=True, exist_ok=True)

    run_git(destination, "init", "--quiet")
    run_git(destination, "remote", "add", "origin", lock.repository)
    fetch_remote = "origin"
    if repository_cache is not None:
        cache = repository_cache.expanduser().resolve()
        run_git(destination, "remote", "add", "cache", str(cache))
        fetch_remote = "cache"
    run_git(destination, "sparse-checkout", "init", "--cone")
    run_git(
        destination,
        "sparse-checkout",
        "set",
        *(fixture.path.as_posix() for fixture in selected),
    )
    run_git(
        destination,
        "fetch",
        "--quiet",
        "--depth=1",
        "--filter=blob:none",
        fetch_remote,
        lock.commit,
    )
    run_git(destination, "checkout", "--quiet", "--detach", "FETCH_HEAD")
    actual_commit = run_git(destination, "rev-parse", "HEAD")
    if actual_commit != lock.commit:
        raise LockError(
            f"checked out fixture commit {actual_commit}; lock requires {lock.commit}"
        )
    for fixture in selected:
        validate_fixture(destination, fixture)


def default_lock_path() -> Path:
    return Path(__file__).resolve().parents[2] / "test-artifacts.lock.json"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Fetch pinned test fixture subtrees with sparse checkout."
    )
    parser.add_argument("--fixture", action="append", required=True)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--lock", type=Path, default=default_lock_path())
    parser.add_argument(
        "--repository-cache",
        type=Path,
        help="optional local Git repository containing the pinned fixture commit",
    )
    args = parser.parse_args()
    try:
        lock = load_lock(args.lock)
        fetch(
            lock,
            args.fixture,
            args.destination.resolve(),
            args.repository_cache,
        )
    except (LockError, RuntimeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(args.destination.resolve())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
