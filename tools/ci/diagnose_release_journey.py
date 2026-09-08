#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Replay one journey against retained app bytes; never issue qualification receipts."""

from __future__ import annotations

import argparse
from dataclasses import replace
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

import local_candidate_qualification as qualification
from verify_release_e2e_apps import VerificationError, verify_bundle


def journey_metadata(journey: str, platform: str) -> dict:
    script = (
        "import {journeyById} from './tools/e2e/release-journey-registry.mjs';"
        "const journey = journeyById(process.argv[1]);"
        "if (!journey || !journey.platforms.includes(process.argv[2])) process.exit(2);"
        "console.log(JSON.stringify(journey));"
    )
    try:
        return json.loads(subprocess.check_output(
            ["node", "--input-type=module", "-e", script, journey, platform],
            cwd=qualification.ROOT, text=True, timeout=15,
        ))
    except (subprocess.SubprocessError, ValueError) as error:
        raise qualification.QualificationError(
            f"unknown or unsupported journey {journey!r} on {platform}"
        ) from error


def diagnostic_lane(
    source: Path, output: Path, journey: str, platform: str, repetitions: int,
    priority: str, net_log: bool,
) -> qualification.Lane:
    fixture = source / "release-journey-materialized/fixture.json"
    apps = source / "release-e2e-apps"
    workspace = source / "release-ui-target/web/workspace"
    baseline = source / "android-release-journey-baseline.tar"
    for path in (fixture, workspace / "node_modules"):
        if not path.exists():
            raise qualification.QualificationError(f"retained run input is missing: {path}")
    if platform == "android" and not baseline.is_file():
        raise qualification.QualificationError(f"retained Android baseline is missing: {baseline}")
    lane = (
        qualification.web_lane(priority, output, fixture, apps, repetitions, journey=journey)
        if platform == "web" else
        qualification.android_shard_lane(3, output, fixture, apps, repetitions, journey=journey)
    )
    env = {
        **lane.env,
        "AEROBAG_WEB_WORKSPACE_DIR": str(workspace),
    }
    if platform == "android":
        env.update({
            "AVD_INSTANCE_NAME": f"aerobag-diagnostic-{output.name.rsplit('-', 1)[-1]}",
            "AEROBAG_ANDROID_BASELINE_ARCHIVE": str(baseline),
        })
    if net_log:
        env["AEROBAG_CHROME_NET_LOG"] = str(output / "chrome-net-{repeat}.json")
    return replace(lane, name=f"diagnose-{platform}-{journey}", env=env)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--from-run", type=Path, required=True)
    parser.add_argument("--journey", required=True)
    parser.add_argument("--platform", choices=("web", "android"), required=True)
    parser.add_argument("--repetitions", type=int, default=1)
    parser.add_argument("--net-log", action="store_true", help="retain Chrome netlogs even on success")
    args = parser.parse_args()
    if args.repetitions < 1:
        parser.error("--repetitions must be positive")
    if args.net_log and args.platform != "web":
        parser.error("--net-log currently requires --platform web")
    metadata = journey_metadata(args.journey, args.platform)
    source = args.from_run.resolve()
    apps = source / "release-e2e-apps"
    verify_bundle(apps)
    manifest = json.loads((apps / "build-manifest.json").read_text(encoding="utf-8"))
    output = Path(tempfile.mkdtemp(prefix="aerobag-journey-diagnostic-"))
    lane = diagnostic_lane(
        source, output, args.journey, args.platform, args.repetitions,
        metadata["priority"], args.net_log,
    )
    record = {
        "kind": "diagnostic-only",
        "source_run": str(source),
        "app_commit": manifest["git_commit"],
        "harness_commit": qualification.git("rev-parse", "HEAD"),
        "harness_dirty": bool(qualification.git("status", "--porcelain")),
        "fixture_sha256": qualification.sha256(source / "release-journey-materialized/fixture.json"),
        "journey": args.journey,
        "platform": args.platform,
        "repetitions": args.repetitions,
        "cpu_throttle_rate": os.environ.get("AEROBAG_E2E_CPU_THROTTLE_RATE", "1"),
    }
    (output / "diagnostic.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    print(f"DIAGNOSTIC ONLY: retained app {record['app_commit']}; current harness {record['harness_commit']}", flush=True)
    print("This run cannot qualify a release. Rebuild app inputs after application changes.", flush=True)
    print(f"Diagnostic directory: {output}", flush=True)
    result = qualification.run_lane(lane, output / "logs")
    record["returncode"] = result.returncode
    (output / "diagnostic.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    return result.returncode


if __name__ == "__main__":
    try:
        with qualification.qualification_lock():
            raise SystemExit(main())
    except (qualification.QualificationError, VerificationError) as error:
        print(f"journey diagnostic: {error}", file=sys.stderr)
        raise SystemExit(2)
