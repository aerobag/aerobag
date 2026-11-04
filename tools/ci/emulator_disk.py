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

BUILD_PACKAGES = ("ndk", "platforms", "build-tools", "cmake")
HOSTED_SDK = Path("/usr/local/lib/android/sdk")
GIB = 1024 ** 3


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


def report(phase, paths, summary=None):
    rows = {}
    for name, path in paths.items():
        path = Path(path)
        if not path.exists():
            continue
        usage = shutil.disk_usage(path)
        allocated = subprocess.run(
            ["du", "-sxB1", "--", str(path)], check=True, text=True,
            stdout=subprocess.PIPE, timeout=60,
        )
        rows[name] = {
            "path": str(path), "allocated_bytes": int(allocated.stdout.split()[0]),
            "total_bytes": usage.total, "free_bytes": usage.free,
        }
    result = {"phase": phase, "runner_image": os.environ.get("ImageVersion"), "paths": rows}
    print(json.dumps(result), flush=True)
    if summary:
        with Path(summary).open("a") as output:
            output.write(f"\n### Emulator disk: {phase}\n\n| Path | Allocated GiB | Free GiB |\n| --- | ---: | ---: |\n")
            for name, row in rows.items():
                output.write(f"| {name} | {row['allocated_bytes'] / GIB:.2f} | {row['free_bytes'] / GIB:.2f} |\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--phase", required=True)
    parser.add_argument("--clean-unused-sdk", action="store_true")
    parser.add_argument("--avd-path", type=Path)
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
    report(args.phase, paths, summary)
    if args.clean_unused_sdk:
        print(f"Removed unused SDK build packages: {clean_build_tools(sdk)}", file=sys.stderr)
        report("after-sdk-cleanup", paths, summary)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"Emulator disk preparation failed: {error}", file=sys.stderr)
        raise SystemExit(1)
