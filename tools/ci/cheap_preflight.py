#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Run every inexpensive CI suite on the current working tree before committing."""

from __future__ import annotations

import argparse
from dataclasses import replace
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

import local_candidate_qualification as qualification


ROOT = qualification.ROOT


def worktree_fingerprint() -> str:
    digest = hashlib.sha256(subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT))
    digest.update(subprocess.check_output(
        ["git", "diff", "--binary", "HEAD"], cwd=ROOT,
    ))
    untracked = subprocess.check_output(
        ["git", "ls-files", "--others", "--exclude-standard", "-z"], cwd=ROOT,
    )
    for name in sorted(filter(None, untracked.split(b"\0"))):
        path = ROOT / os.fsdecode(name)
        digest.update(name + b"\0")
        digest.update(os.fsencode(os.readlink(path)) if path.is_symlink() else path.read_bytes())
    return digest.hexdigest()


def cheap_lanes(run_root: Path) -> list[qualification.Lane]:
    ui_target = Path(os.environ.get(
        "AEROBAG_UI_TARGET_ROOT", str(ROOT / (ROOT / "ui/target-root.txt").read_text().strip()),
    )).resolve()
    environment = {
        "AEROBAG_ARTIFACT_READ_PATH": str(run_root / "no-artifacts"),
        "AEROBAG_REPO_ROOT": str(ROOT),
        "AEROBAG_UI_TARGET_ROOT": str(ui_target),
        "AEROBAG_WEB_WORKSPACE_DIR": str(ui_target / "web/workspace"),
    }
    # Share ordinary-CI suite membership with release preflight. No path-based
    # selection: a Rust enum can break a JavaScript harness contract.
    lanes = qualification.ordinary_lanes(run_root)
    # Each Node lane owns its preparation: never race npm ci or script copying
    # against the parallel web checks, and keep warm harness dependencies reusable.
    lanes = [
        replace(lane, env={
            **(lane.env or {}),
            "AEROBAG_WEB_WORKSPACE_DIR": str(ui_target / "web/harness-workspace"),
        }) if lane.name == "ci-harness-contracts" else lane
        for lane in lanes
    ]
    lanes.extend([
        qualification.Lane("ci-generated-ui", (
            "/usr/bin/python3", "tools/ci/check_generated_ui_sources.py",
        )),
        qualification.Lane("ci-web-unit", (
            str(ROOT / "ui/web-app/scripts/run-target-workspace.sh"), "inner:check",
        )),
        qualification.Lane("ci-android-jvm", (
            str(ROOT / "ui/android-app/scripts/test.sh"), "testDebugUnitTest",
        ), env={"ANDROID_BUILD_NATIVE_LIBRARIES": "false"}),
        qualification.Lane("ci-diff", ("git", "diff", "--check", "HEAD")),
    ])
    return [replace(lane, env={**environment, **(lane.env or {})}) for lane in lanes]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--list", action="store_true", help="show all suites without running them")
    parser.add_argument("--jobs", type=int, default=min(8, os.cpu_count() or 1))
    parser.add_argument("--timeout-seconds", type=int, default=180,
                        help="per-suite deadline, including compilation (default: 180)")
    args = parser.parse_args(argv)
    if args.jobs < 1 or args.timeout_seconds < 1:
        parser.error("jobs and timeout must be positive")
    # Fresh logs and explicit absent production artifacts for every run. There
    # are no HEAD-only success receipts for a possibly dirty working tree.
    run_root = Path(tempfile.mkdtemp(prefix="aerobag-cheap-preflight-"))
    lanes = [replace(lane, timeout_seconds=args.timeout_seconds) for lane in cheap_lanes(run_root)]
    if args.list:
        for lane in lanes:
            print(f"{lane.name}: {json.dumps(lane.command)}")
        return 0
    print(f"Cheap preflight logs: {run_root}", flush=True)
    before = worktree_fingerprint()
    started = time.monotonic()
    try:
        # Android's ordinary build regenerates sources. Check their incoming
        # bytes first so generation cannot repair stale files into a green run.
        qualification.run_lanes(
            [lane for lane in lanes if lane.name == "ci-generated-ui"], run_root / "logs", 1,
        )
        qualification.run_lanes(
            [lane for lane in lanes if lane.name != "ci-generated-ui"], run_root / "logs", args.jobs,
        )
    except (qualification.QualificationError, subprocess.CalledProcessError) as error:
        print(f"Cheap preflight failed: {error}")
        return 1
    if worktree_fingerprint() != before:
        print("Cheap preflight failed: working-tree files changed during the run; check the final tree again")
        return 1
    print(f"Cheap preflight passed: {len(lanes)} suites in {time.monotonic() - started:.1f}s")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
