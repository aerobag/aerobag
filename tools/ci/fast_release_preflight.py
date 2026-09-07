#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Run the bounded, emulator-free checks required before creating a release tag."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(Path(__file__).resolve().parent))

import local_candidate_qualification as qualification  # noqa: E402


MINIMUM_FREE_BYTES = 4 * 1024 * 1024 * 1024


def receipt_path(commit: str) -> Path:
    git_dir = Path(qualification.git("rev-parse", "--git-common-dir"))
    if not git_dir.is_absolute():
        git_dir = ROOT / git_dir
    return git_dir.resolve() / "aerobag-fast-release-preflight" / f"{commit}.json"


def valid_receipt(commit: str) -> bool:
    path = receipt_path(commit)
    try:
        receipt = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return False
    return (
        receipt.get("schema_version") == 1
        and receipt.get("commit") == commit
        and receipt.get("status") == "passed"
    )


def write_receipt(
    commit: str,
    started_at: str,
    results: list[qualification.LaneResult],
) -> Path:
    path = receipt_path(commit)
    path.parent.mkdir(parents=True, exist_ok=True)
    receipt = {
        "schema_version": 1,
        "commit": commit,
        "status": "passed",
        "started_at": started_at,
        "completed_at": datetime.now(timezone.utc).isoformat(),
        "lanes": [
            {
                "name": result.name,
                "duration_seconds": round(result.duration_seconds, 3),
                "log": str(result.log_path),
            }
            for result in sorted(results, key=lambda result: result.name)
        ],
    }
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    temporary.replace(path)
    return path


def main() -> int:
    commit = qualification.assert_clean_commit()
    if valid_receipt(commit):
        print(f"Fast release preflight already passed: {receipt_path(commit)}")
        return 0

    free = shutil.disk_usage(tempfile.gettempdir()).free
    if free < MINIMUM_FREE_BYTES:
        raise qualification.QualificationError(
            "fast release preflight requires at least 4 GiB free under "
            f"{tempfile.gettempdir()}"
        )

    started_at = datetime.now(timezone.utc).isoformat()
    run_root = Path(tempfile.gettempdir()) / f"aerobag-fast-preflight-{commit[:12]}"
    if run_root.exists():
        shutil.rmtree(run_root)
    run_root.mkdir(parents=True)
    logs = run_root / "logs"
    results: list[qualification.LaneResult] = []

    qualification.prepare_gradle_caches(run_root)
    qualification.prepare_environment()
    print("Running emulator-free release preflight", flush=True)
    results.extend(
        qualification.run_lanes(
            qualification.ordinary_lanes(run_root),
            logs,
            7,
        )
    )
    for lane in qualification.sequential_ci_lanes(run_root):
        results.extend(qualification.run_lanes([lane], logs, 1))

    if qualification.git("status", "--porcelain"):
        raise qualification.QualificationError(
            "release preflight generated tracked source changes"
        )
    path = write_receipt(commit, started_at, results)
    print(f"Fast release preflight passed: {path}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (qualification.QualificationError, subprocess.CalledProcessError) as error:
        print(f"fast release preflight: {error}", file=sys.stderr)
        raise SystemExit(2)
