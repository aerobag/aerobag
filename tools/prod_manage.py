#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Create release intent commits and reconcile them onto production."""

from __future__ import annotations

import argparse
import difflib
import json
import os
import re
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


TOOLS_DIR = Path(__file__).resolve().parent
REPO_ROOT = TOOLS_DIR.parent
sys.path.insert(0, str(TOOLS_DIR))

import deploy_prod  # noqa: E402
import release_reconciler as releases  # noqa: E402


DEFAULT_CONFIG = REPO_ROOT / "deploy/aerobag-prod.json"
DEFAULT_RELEASES = REPO_ROOT / "deploy/releases.json"


class ManagementError(RuntimeError):
    pass


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Stage or promote the checked-in Aerobag release desired state."
    )
    operation = parser.add_mutually_exclusive_group(required=True)
    operation.add_argument("--stage", action="store_true")
    operation.add_argument("--promote", action="store_true")
    return parser.parse_args()


def run(
    command: list[str],
    *,
    capture: bool = False,
    cwd: Path = REPO_ROOT,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=cwd,
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
    )


def git(*args: str, capture: bool = True) -> str:
    result = run(["git", *args], capture=capture)
    return result.stdout.strip() if result.stdout is not None else ""


def load_release_document(path: Path) -> dict[str, Any]:
    desired = releases.load_desired_releases(path)
    document = json.loads(path.read_text(encoding="utf-8"))
    # Keep all mutation code behind the strict contract parser.
    if desired.production.tag != document["production"]["tag"]:
        raise ManagementError("release desired state changed while it was read")
    return document


def serialize_release_document(document: dict[str, Any]) -> str:
    releases.parse_desired_releases(document)
    return json.dumps(document, indent=2, sort_keys=True) + "\n"


def stage_document(document: dict[str, Any], tag: str) -> dict[str, Any]:
    desired = releases.parse_desired_releases(document)
    if desired.staging is not None:
        raise ManagementError(
            f"staging already names {desired.staging.tag}; promote or clear it explicitly first"
        )
    proposed = json.loads(json.dumps(document))
    proposed["staging"] = {"tag": tag}
    releases.parse_desired_releases(proposed)
    return proposed


def promotion_document(document: dict[str, Any]) -> tuple[dict[str, Any], str, str]:
    desired = releases.parse_desired_releases(document)
    if desired.staging is None:
        raise ManagementError("there is no staging release to promote")
    old_production = desired.production.tag
    candidate = desired.staging.tag
    proposed = json.loads(json.dumps(document))
    proposed["production"] = {"tag": candidate}
    proposed["staging"] = None
    releases.parse_desired_releases(proposed)
    return proposed, old_production, candidate


def next_release_name(existing_tags: set[str], now: datetime | None = None) -> str:
    instant = now or datetime.now(timezone.utc)
    stem = instant.astimezone(timezone.utc).strftime("%Y-%m-%d")
    pattern = re.compile(rf"^{re.escape(stem)}\.(\d+)$")
    used_sequences = [
        int(match.group(1))
        for tag in existing_tags
        if (match := pattern.fullmatch(tag)) is not None
    ]
    sequence = max(used_sequences, default=0) + 1
    return f"{stem}.{sequence}"


def color_diff(path: Path, old: str, new: str) -> str:
    lines = difflib.unified_diff(
        old.splitlines(),
        new.splitlines(),
        fromfile=str(path.relative_to(REPO_ROOT)),
        tofile=f"{path.relative_to(REPO_ROOT)} (proposed)",
        lineterm="",
    )
    colored = []
    enabled = "NO_COLOR" not in os.environ
    for line in lines:
        color = ""
        reset = ""
        if enabled:
            if line.startswith(("---", "+++", "@@")):
                color = "\x1b[36m"
            elif line.startswith("+"):
                color = "\x1b[32m"
            elif line.startswith("-"):
                color = "\x1b[31m"
        if color:
            reset = "\x1b[0m"
        colored.append(f"{color}{line}{reset}")
    return "\n".join(colored)


def existing_release_tags() -> set[str]:
    return set(git("tag", "--list").splitlines())


def assert_main_not_behind(*, require_synchronized: bool) -> None:
    branch = git("branch", "--show-current")
    if branch != "main":
        raise ManagementError(f"release management requires branch main, not {branch or 'detached HEAD'}")
    counts = git("rev-list", "--left-right", "--count", "origin/main...HEAD")
    try:
        behind, ahead = (int(value) for value in counts.split())
    except ValueError as error:
        raise ManagementError(f"could not compare main with origin/main: {counts}") from error
    if behind:
        raise ManagementError(
            f"main is {behind} commit(s) behind origin/main; pull and rebase first"
        )
    if require_synchronized and ahead:
        raise ManagementError(
            f"main is {ahead} commit(s) ahead of origin/main; promotion requires a synchronized checkout"
        )


def assert_clean_checkout() -> None:
    status = git("status", "--porcelain")
    if status:
        raise ManagementError("promotion requires a clean checkout")


def write_atomic(path: Path, content: str) -> None:
    with tempfile.NamedTemporaryFile(
        mode="w",
        encoding="utf-8",
        dir=path.parent,
        prefix=f".{path.name}.",
        delete=False,
    ) as stream:
        stream.write(content)
        temporary = Path(stream.name)
    os.replace(temporary, path)


def assert_remote_idle(config: dict[str, Any]) -> None:
    try:
        deploy_prod.assert_release_reconciliation_idle(config, dry_run=False)
    except subprocess.CalledProcessError as error:
        raise ManagementError(
            "production release reconciliation is still running; wait for it to finish before changing release intent"
        ) from error


def load_remote_observed(config: dict[str, Any]) -> dict[str, Any]:
    path = f"{config['artifact_root']}/state/releases-observed.json"
    result = deploy_prod.run_ssh(
        config,
        f"cat {deploy_prod.shell_quote(path)}",
        capture=True,
        dry_run=False,
    )
    try:
        value = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise ManagementError(f"production release state is invalid: {error}") from error
    if not isinstance(value, dict):
        raise ManagementError("production release state is not an object")
    return value


def assert_staging_is_qualified(
    config: dict[str, Any], desired: releases.DesiredReleases
) -> None:
    if desired.staging is None:
        raise ManagementError("there is no staging release to promote")
    observed = load_remote_observed(config)
    if observed.get("production") != desired.production.tag:
        raise ManagementError(
            "production has not converged on the production release in deploy/releases.json"
        )
    if observed.get("staging") != desired.staging.tag:
        raise ManagementError(
            f"{desired.staging.tag} is not the currently active staging release"
        )
    record = observed.get("releases", {}).get(desired.staging.tag)
    if not isinstance(record, dict) or record.get("qualification_status") != "passed":
        raise ManagementError(
            f"staging release {desired.staging.tag} has not passed qualification"
        )


def print_proposal(title: str, commands: list[str], diff: str, note: str | None = None) -> None:
    print(title)
    print()
    if note:
        print(note)
        print()
    for command in commands:
        print(f"  {command}")
    print()
    print(diff)
    print()


def confirmed() -> bool:
    return input("proceed? y/N ").strip().lower() in {"y", "yes"}


def deploy(config_path: Path) -> None:
    run([str(TOOLS_DIR / "deploy_prod.py"), "--config", str(config_path)], capture=False)
    config = deploy_prod.load_config(config_path)
    print("waiting for production release reconciliation to finish...", flush=True)
    deploy_prod.run_ssh(
        config,
        """set -euo pipefail
unit=aerobag-build-product.service
while true; do
  active_state="$(systemctl show "$unit" --property=ActiveState --value)"
  result="$(systemctl show "$unit" --property=Result --value)"
  case "$active_state" in
    active|activating|reloading|deactivating)
      sleep 5
      ;;
    inactive)
      if [ "$result" = success ]; then
        echo "release reconciliation completed successfully"
        exit 0
      fi
      ;;
    failed)
      ;;
  esac
  echo "release reconciliation failed: active_state=$active_state result=$result" >&2
  systemctl status "$unit" --no-pager >&2 || true
  journalctl -u "$unit" -n 100 --no-pager >&2 || true
  exit 1
done""",
        dry_run=False,
    )


def stage(config_path: Path, releases_path: Path) -> int:
    config = deploy_prod.load_config(config_path)
    git("fetch", "--tags", "origin", capture=False)
    assert_main_not_behind(require_synchronized=False)
    assert_remote_idle(config)

    document = load_release_document(releases_path)
    desired = releases.parse_desired_releases(document)
    if desired.staging is not None:
        assert_main_not_behind(require_synchronized=True)
        assert_clean_checkout()
        tag = desired.staging.tag
        print_proposal(
            f"This command will resume staging {tag}, running this command:",
            ["tools/deploy_prod.py --config deploy/aerobag-prod.json"],
            f"  deploy/releases.json already assigns staging to {tag}; no source change is needed.",
        )
        if not confirmed():
            print("aborted")
            return 1
        assert_remote_idle(config)
        deploy(config_path)
        return 0

    tag = next_release_name(existing_release_tags())
    proposed = stage_document(document, tag)
    old_text = serialize_release_document(document)
    new_text = serialize_release_document(proposed)
    dirty = git("status", "--porcelain")
    commands = []
    if dirty:
        commands.extend(
            [
                "git add -A",
                f'git commit -m "Prepare release {tag}"',
            ]
        )
    else:
        commands.append("Use current HEAD (checkout is clean)")
    commands.extend(
        [
            f'git tag -a {tag} -m "Aerobag {tag}"',
            "git push origin main",
            f"git push origin {tag}",
            f"Modify deploy/releases.json to make staging {tag}",
            "git add deploy/releases.json",
            f'git commit -m "Stage {tag}"',
            "git push origin main",
            "tools/deploy_prod.py --config deploy/aerobag-prod.json",
        ]
    )
    print_proposal(
        f"This command will stage {tag}, running these commands:",
        commands,
        color_diff(releases_path, old_text, new_text),
        note=(f"The release commit will include:\n{dirty}" if dirty else None),
    )
    if not confirmed():
        print("aborted")
        return 1

    assert_remote_idle(config)
    if tag in existing_release_tags():
        raise ManagementError(f"release tag {tag} appeared during confirmation; retry")
    if dirty:
        git("add", "-A", capture=False)
        git("commit", "-m", f"Prepare release {tag}", capture=False)
    git("tag", "-a", tag, "-m", f"Aerobag {tag}", capture=False)
    git("push", "origin", "main", capture=False)
    git("push", "origin", tag, capture=False)
    write_atomic(releases_path, new_text)
    git("add", str(releases_path.relative_to(REPO_ROOT)), capture=False)
    git("commit", "-m", f"Stage {tag}", capture=False)
    git("push", "origin", "main", capture=False)
    deploy(config_path)
    return 0


def promote(config_path: Path, releases_path: Path) -> int:
    config = deploy_prod.load_config(config_path)
    git("fetch", "--tags", "origin", capture=False)
    assert_main_not_behind(require_synchronized=True)
    assert_clean_checkout()
    assert_remote_idle(config)

    document = load_release_document(releases_path)
    desired = releases.parse_desired_releases(document)
    assert_staging_is_qualified(config, desired)
    proposed, old_production, candidate = promotion_document(document)
    old_text = serialize_release_document(document)
    new_text = serialize_release_document(proposed)
    commands = [
        f"Modify deploy/releases.json to promote {candidate} and clear staging",
        "git add deploy/releases.json",
        f'git commit -m "Promote {candidate}"',
        "git push origin main",
        "tools/deploy_prod.py --config deploy/aerobag-prod.json",
    ]
    print_proposal(
        f"This command will promote {candidate} to production, running these commands:",
        commands,
        color_diff(releases_path, old_text, new_text),
        note=(
            f"This does not retain production release {old_production} as a sunset release. "
            "If installed clients still need it, abort and perform a complete desired-state "
            "promotion edit that includes its sunset deadline."
        ),
    )
    if not confirmed():
        print("aborted")
        return 1

    assert_remote_idle(config)
    write_atomic(releases_path, new_text)
    git("add", str(releases_path.relative_to(REPO_ROOT)), capture=False)
    git("commit", "-m", f"Promote {candidate}", capture=False)
    git("push", "origin", "main", capture=False)
    deploy(config_path)
    return 0


def main() -> int:
    args = parse_args()
    try:
        if args.stage:
            return stage(DEFAULT_CONFIG, DEFAULT_RELEASES)
        return promote(DEFAULT_CONFIG, DEFAULT_RELEASES)
    except (ManagementError, releases.ReleaseConfigError) as error:
        print(f"prod_manage: {error}", file=sys.stderr)
        return 2
    except subprocess.CalledProcessError as error:
        command = " ".join(str(part) for part in error.cmd)
        print(
            f"prod_manage: command failed with exit {error.returncode}: {command}",
            file=sys.stderr,
        )
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
