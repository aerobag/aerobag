#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Account for emulator disk use; explicitly prune compiler SDKs on hosted CI only."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

BUILD_PACKAGES = ("ndk", "platforms", "build-tools", "cmake")
HOSTED_SDK = Path("/usr/local/lib/android/sdk")
GIB = 1024 ** 3
SIZE_SCAN_BUDGET_SECONDS = 5


def hosted_sdk(env):
    if (env.get("GITHUB_ACTIONS"), env.get("RUNNER_ENVIRONMENT"), env.get("RUNNER_OS")) != (
        "true", "github-hosted", "Linux",
    ):
        raise ValueError("SDK cleanup requires a disposable GitHub-hosted Linux runner")
    sdk = env.get("ANDROID_HOME", "")
    if sdk != str(HOSTED_SDK) or Path(sdk).resolve() != HOSTED_SDK:
        raise ValueError(f"refusing cleanup of unrecognized SDK root: {sdk!r}")
    if env.get("ANDROID_SDK_ROOT", sdk) != sdk:
        raise ValueError("ANDROID_HOME and ANDROID_SDK_ROOT disagree")
    return Path(sdk)


def clean_build_tools(sdk):
    targets = [sdk / package for package in BUILD_PACKAGES]
    # Validate the whole set first; never follow links out of the SDK.
    if any(target.is_symlink() for target in targets):
        raise ValueError("refusing SDK build-directory symlink cleanup")
    removed = []
    for target in targets:
        if target.exists():
            shutil.rmtree(target)
            removed.append(target.name)
    return removed


def measure_allocated_sizes(rows):
    # One budget for the whole optional diagnostic, not a fresh wait per directory.
    deadline = time.monotonic() + SIZE_SCAN_BUDGET_SECONDS
    for row in rows.values():
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            row["size_error"] = "size scan budget exhausted"
            continue
        try:
            allocated = subprocess.run(
                ["du", "-sxB1", "--", row["path"]], check=True, text=True,
                capture_output=True, timeout=remaining,
            )
            count = int(allocated.stdout.split(maxsplit=1)[0])
            if count < 0:
                raise ValueError("negative allocated size")
            row["allocated_bytes"] = count
        except subprocess.TimeoutExpired as error:
            row["size_error"] = str(error)
            deadline = 0
        except (OSError, subprocess.CalledProcessError, ValueError, IndexError) as error:
            row["size_error"] = str(error)


def format_gib(value):
    return "unavailable" if value is None else f"{value / GIB:.2f}"


def report(phase, paths, summary=None, *, measure_sizes=False):
    rows = {}
    for name, path in paths.items():
        path = Path(path)
        row = {
            "path": str(path), "allocated_bytes": None,
            "total_bytes": None, "free_bytes": None,
        }
        try:
            if not path.exists():
                continue
            usage = shutil.disk_usage(path)
            row.update(total_bytes=usage.total, free_bytes=usage.free)
        except OSError as error:
            row["disk_error"] = str(error)
        rows[name] = row
    if measure_sizes:
        measure_allocated_sizes(rows)
    result = {"phase": phase, "runner_image": os.environ.get("ImageVersion"), "paths": rows}
    print(json.dumps(result), flush=True)
    for name, row in rows.items():
        for key in ("disk_error", "size_error"):
            if key in row:
                print(f"Emulator disk diagnostic unavailable ({phase}/{name}/{key}): {row[key]}", file=sys.stderr)
    if summary:
        try:
            with Path(summary).open("a") as output:
                output.write(f"\n### Emulator disk: {phase}\n\n| Path | Allocated GiB | Free GiB |\n| --- | ---: | ---: |\n")
                for name, row in rows.items():
                    allocated = format_gib(row["allocated_bytes"]) if measure_sizes else "not requested"
                    output.write(f"| {name} | {allocated} | {format_gib(row['free_bytes'])} |\n")
        except OSError as error:
            print(f"Emulator disk summary unavailable: {error}", file=sys.stderr)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--phase", required=True)
    parser.add_argument("--clean-unused-sdk", action="store_true")
    parser.add_argument("--avd-path", type=Path)
    parser.add_argument("--measure-sizes", action="store_true", help="Optional post-test recursive sizes (shared 5s budget)")
    args = parser.parse_args()
    # Refuse unsafe cleanup before even inspecting directories.
    sdk = hosted_sdk(os.environ) if args.clean_unused_sdk else Path(
        os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT") or "/usr/lib/android-sdk"
    )
    paths = {"workspace": Path.cwd(), "android_sdk": sdk}
    if args.avd_path:
        paths["avd"] = args.avd_path
    else:
        paths["avd_android_home"] = Path.home() / ".android/avd"
        paths["avd_config_home"] = Path.home() / ".config/.android/avd"
        if os.environ.get("ANDROID_AVD_HOME"):
            paths["avd"] = Path(os.environ["ANDROID_AVD_HOME"])
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    report(args.phase, paths, summary, measure_sizes=args.measure_sizes and not args.clean_unused_sdk)
    if args.clean_unused_sdk:
        print(f"Removed unused SDK build packages: {clean_build_tools(sdk)}", file=sys.stderr)
        report("after-sdk-cleanup", paths, summary, measure_sizes=args.measure_sizes)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"Emulator disk preparation failed: {error}", file=sys.stderr)
        raise SystemExit(1)
