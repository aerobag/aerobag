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
        description="Change or reconcile Aerobag's checked-in production release intent."
    )
    operation = parser.add_mutually_exclusive_group(required=True)
    operation.add_argument("--stage", action="store_true")
    operation.add_argument("--promote", action="store_true")
    operation.add_argument("--reconcile", action="store_true")
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
    releases.parse_desired_releases(document)
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
        raise ManagementError(
            f"release management requires branch main, not {branch or 'detached HEAD'}"
        )
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
            f"main is {ahead} commit(s) ahead of origin/main; release management "
            "requires a synchronized checkout"
        )


def assert_clean_checkout(operation: str) -> None:
    status = git("status", "--porcelain")
    if status:
        raise ManagementError(f"can't {operation} with uncommitted work")


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
            "production release reconciliation is already running; wait for it to finish"
        ) from error


def load_remote_observed(config: dict[str, Any]) -> releases.ObservedState:
    path = f"{config['artifact_root']}/state/releases-observed.json"
    quoted_path = deploy_prod.shell_quote(path)
    result = deploy_prod.run_ssh(
        config,
        f"if test -f {quoted_path}; then cat {quoted_path}; fi",
        capture=True,
        dry_run=False,
    )
    if not result.stdout.strip():
        return releases.ObservedState.empty()
    try:
        value = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise ManagementError(f"production release state is invalid: {error}") from error
    return releases.ObservedState.from_dict(value)


def reconciliation_plan(
    desired: releases.DesiredReleases,
    observed: releases.ObservedState,
) -> releases.ReconciliationPlan:
    resolved = releases.resolve_desired_tags(REPO_ROOT, desired)
    for tag, identity in resolved.items():
        record = observed.releases.get(tag)
        if record is not None:
            releases.verify_release_identity(identity, record)
    return releases.plan_reconciliation(desired, observed)


def remote_runtime_failures(
    config: dict[str, Any],
    desired: releases.DesiredReleases,
    observed: releases.ObservedState,
    controller_commit: str,
) -> list[str]:
    checks = [
        (
            f"test -d {deploy_prod.shell_quote(config['source_root'] + '/.git')}",
            "controller source is not installed",
        ),
        (
            f"test \"$(cat {deploy_prod.shell_quote(deploy_prod.DEPLOYED_REV_FILE)} "
            f"2>/dev/null || true)\" = {deploy_prod.shell_quote(controller_commit)}",
            "installed controller revision differs",
        ),
        (
            f"test -L {deploy_prod.shell_quote(config['artifact_root'] + '/channel-current')}",
            "active channel link is missing",
        ),
    ]
    required_units = [
        "nginx.service",
        "aerobag-cloud-server.service",
        "aerobag-client-debug-log.service",
        "aerobag-build-watch.service",
        "aerobag-pipeline-health.service",
        "aerobag-build-product.timer",
        "aerobag-health.timer",
        "aerobag-cloud-backup.timer",
        *(f"aerobag-live-feeds-release@{tag}.service" for tag in desired.tags()),
    ]
    checks.extend(
        (f"systemctl is-active --quiet {deploy_prod.shell_quote(unit)}", f"{unit} is not active")
        for unit in required_units
    )
    for tag in desired.tags():
        record = observed.releases.get(tag)
        if record is None:
            continue
        for path, description in (
            (record.release_root, f"release output for {tag} is missing"),
            (record.product_manifest, f"product manifest for {tag} is missing"),
            (record.qualification_record, f"qualification record for {tag} is missing"),
        ):
            if path is not None:
                checks.append(
                    (f"test -e {deploy_prod.shell_quote(path)}", description)
                )

    lines = [
        f"{command} || printf '%s\\n' {deploy_prod.shell_quote(description)}"
        for command, description in checks
    ]
    result = deploy_prod.run_ssh(
        config,
        "\n".join(lines),
        capture=True,
        dry_run=False,
    )
    return [line for line in result.stdout.splitlines() if line]


def describe_assignments(desired: releases.DesiredReleases) -> str:
    staging = desired.staging.tag if desired.staging is not None else "none"
    sunset = ", ".join(binding.tag for binding in desired.sunset) or "none"
    return f"production={desired.production.tag}, staging={staging}, sunset={sunset}"


def print_success(message: str) -> None:
    if "NO_COLOR" in os.environ:
        print(message)
    else:
        print(f"\x1b[32m{message}\x1b[0m")


def assert_staging_is_qualified(
    config: dict[str, Any], desired: releases.DesiredReleases
) -> None:
    if desired.staging is None:
        raise ManagementError("there is no staging release to promote")
    observed = load_remote_observed(config)
    if observed.production != desired.production.tag:
        raise ManagementError(
            "production has not converged on the production release in deploy/releases.json"
        )
    if observed.staging != desired.staging.tag:
        raise ManagementError(
            f"{desired.staging.tag} is not the currently active staging release"
        )
    record = observed.releases.get(desired.staging.tag)
    if record is None or record.qualification_status != "passed":
        raise ManagementError(
            f"staging release {desired.staging.tag} has not passed qualification; "
            "use --reconcile before promoting"
        )
    plan = reconciliation_plan(
        releases.effective_desired_releases(desired), observed
    )
    if not plan.converged:
        raise ManagementError(
            f"staging release {desired.staging.tag} has not fully converged; "
            "use --reconcile before promoting"
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
    run(
        [
            str(TOOLS_DIR / "deploy_prod.py"),
            "--config",
            str(config_path),
            "--wait-for-reconciliation",
        ],
        capture=False,
    )


def failed_command_summary(command: str | list[str]) -> str:
    return deploy_prod.failed_command_summary(command)


def stage(config_path: Path, releases_path: Path) -> int:
    assert_clean_checkout("stage")
    git("fetch", "--tags", "origin", capture=False)
    assert_main_not_behind(require_synchronized=True)

    document = load_release_document(releases_path)
    desired = releases.parse_desired_releases(document)
    head = git("rev-parse", "HEAD")
    if desired.staging is not None:
        staged = releases.resolve_release_tag(REPO_ROOT, desired.staging.tag)
        if staged.commit == head:
            raise ManagementError(
                f"this commit is already staged as {desired.staging.tag}; "
                "use --reconcile to check production state"
            )

    config = deploy_prod.load_config(config_path)
    assert_remote_idle(config)
    tag = next_release_name(existing_release_tags())
    proposed = stage_document(document, tag)
    old_text = serialize_release_document(document)
    new_text = serialize_release_document(proposed)
    replacement_note = None
    if desired.staging is not None:
        replacement_note = (
            f"This replaces staging release {desired.staging.tag}. The old release tag remains "
            "immutable, but it will no longer be assigned to the staging channel."
        )
    commands = [
        f"Modify deploy/releases.json to make staging {tag}",
        "git add deploy/releases.json",
        f'git commit -m "Stage {tag}"',
        f'git tag -a {tag} -m "Aerobag {tag}"',
        f"git push --atomic origin main {tag}",
        "tools/prod_manage.py --reconcile",
    ]
    print_proposal(
        f"This command will stage {tag}, running these commands:",
        commands,
        color_diff(releases_path, old_text, new_text),
        note=replacement_note,
    )
    if not confirmed():
        print("aborted")
        return 1

    assert_remote_idle(config)
    git("fetch", "--tags", "origin", capture=False)
    if tag in existing_release_tags():
        raise ManagementError(f"release tag {tag} appeared during confirmation; retry")
    write_atomic(releases_path, new_text)
    git("add", str(releases_path.relative_to(REPO_ROOT)), capture=False)
    git("commit", "-m", f"Stage {tag}", capture=False)
    git("tag", "-a", tag, "-m", f"Aerobag {tag}", capture=False)
    git("push", "--atomic", "origin", "main", tag, capture=False)
    return reconcile(config_path, releases_path)


def promote(config_path: Path, releases_path: Path) -> int:
    assert_clean_checkout("promote")
    git("fetch", "--tags", "origin", capture=False)
    assert_main_not_behind(require_synchronized=True)

    document = load_release_document(releases_path)
    desired = releases.parse_desired_releases(document)
    if desired.staging is None:
        raise ManagementError(
            "nothing is staged; nothing to promote. Use --reconcile to confirm "
            "promotion is complete"
        )

    config = deploy_prod.load_config(config_path)
    assert_remote_idle(config)
    assert_staging_is_qualified(config, desired)
    proposed, old_production, candidate = promotion_document(document)
    old_text = serialize_release_document(document)
    new_text = serialize_release_document(proposed)
    commands = [
        f"Modify deploy/releases.json to promote {candidate} and clear staging",
        "git add deploy/releases.json",
        f'git commit -m "Promote {candidate}"',
        "git push origin main",
        "tools/prod_manage.py --reconcile",
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
    return reconcile(config_path, releases_path)


def reconcile(config_path: Path, releases_path: Path) -> int:
    assert_clean_checkout("reconcile")
    git("fetch", "--tags", "origin", capture=False)
    assert_main_not_behind(require_synchronized=True)

    document = load_release_document(releases_path)
    desired = releases.effective_desired_releases(
        releases.parse_desired_releases(document)
    )
    config = deploy_prod.load_config(config_path)
    assert_remote_idle(config)
    observed = load_remote_observed(config)
    plan = reconciliation_plan(desired, observed)
    controller_commit = git("rev-parse", "HEAD")
    runtime_failures = (
        remote_runtime_failures(config, desired, observed, controller_commit)
        if plan.converged
        else []
    )
    assignments = describe_assignments(desired)
    if plan.converged and not runtime_failures:
        print_success(f"Production is reconciled: {assignments}")
        return 0

    actions = ", ".join(
        action.kind if action.tag is None else f"{action.kind}({action.tag})"
        for action in plan.actions
    )
    detail = (
        actions
        or plan.blocked_reason
        or "; ".join(runtime_failures)
        or "deployment state differs"
    )
    print(f"Production needs reconciliation ({detail}): {assignments}")
    deploy(config_path)

    observed = load_remote_observed(config)
    final_plan = reconciliation_plan(desired, observed)
    final_runtime_failures = (
        remote_runtime_failures(config, desired, observed, controller_commit)
        if final_plan.converged
        else []
    )
    if not final_plan.converged or final_runtime_failures:
        detail = final_plan.blocked_reason or ", ".join(
            action.kind if action.tag is None else f"{action.kind}({action.tag})"
            for action in final_plan.actions
        )
        if final_runtime_failures:
            detail = "; ".join(final_runtime_failures)
        raise ManagementError(
            f"deployment completed without converging release state: {detail}"
        )
    print_success(f"Production reconciliation completed: {assignments}")
    return 0


def main() -> int:
    args = parse_args()
    try:
        if args.stage:
            return stage(DEFAULT_CONFIG, DEFAULT_RELEASES)
        if args.promote:
            return promote(DEFAULT_CONFIG, DEFAULT_RELEASES)
        return reconcile(DEFAULT_CONFIG, DEFAULT_RELEASES)
    except (ManagementError, releases.ReleaseConfigError) as error:
        print(f"prod_manage: {error}", file=sys.stderr)
        return 2
    except subprocess.CalledProcessError as error:
        print(
            f"prod_manage: command failed with exit {error.returncode}: "
            f"{failed_command_summary(error.cmd)}",
            file=sys.stderr,
        )
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
