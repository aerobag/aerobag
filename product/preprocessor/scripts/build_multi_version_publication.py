#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import argparse
import fcntl
import os
import re
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


SCRIPT = Path(__file__).resolve()
REPO_ROOT = SCRIPT.parents[3]
PREPROCESSOR_DIR = Path("product/preprocessor")
PRODUCT_ARTIFACTS_RE = re.compile(r"^product_artifacts\s+(.+)$")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Build one product publication per git ref, then merge their "
            "product_artifacts manifests into published/current_artifacts.json."
        )
    )
    parser.add_argument("refs", nargs="+", help="git commits, tags, or branch names to build")
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=REPO_ROOT,
        help=f"repository root (default: {REPO_ROOT})",
    )
    parser.add_argument(
        "--worktree-root",
        type=Path,
        default=None,
        help=(
            "directory for ephemeral git worktrees "
            "(default: artifact-root/worktrees/multi-version)"
        ),
    )
    parser.add_argument(
        "--target-dir",
        type=Path,
        default=None,
        help="shared Cargo target directory (default: artifact-root/target)",
    )
    parser.add_argument(
        "--build-root",
        type=Path,
        default=None,
        help="artifact root passed to build-product; it owns cache/ and published/",
    )
    parser.add_argument(
        "--release",
        action="store_true",
        help="build and run release preprocessor-cli binaries from the shared target dir",
    )
    parser.add_argument(
        "--as-of-utc",
        default=None,
        help="RFC3339 UTC timestamp recorded in the merged current_artifacts file",
    )
    parser.add_argument(
        "build_args",
        nargs=argparse.REMAINDER,
        help="extra arguments after -- are passed to build-product",
    )
    return parser.parse_args()


def run(
    args: list[str],
    *,
    cwd: Path,
    env: dict[str, str] | None = None,
    capture: bool = False,
) -> subprocess.CompletedProcess[str]:
    print(f"+ cd {cwd} && {' '.join(args)}", flush=True)
    result = subprocess.run(
        args,
        cwd=cwd,
        env=env,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.STDOUT if capture else None,
    )
    if result.returncode != 0:
        if capture and result.stdout:
            print(result.stdout, end="" if result.stdout.endswith("\n") else "\n")
        raise subprocess.CalledProcessError(result.returncode, result.args, result.stdout)
    return result


def artifact_root(repo_root: Path) -> Path:
    env_value = os.environ.get("AEROBAG_ARTIFACT_WRITE_PATH")
    if env_value:
        path = Path(env_value).expanduser()
        return path if path.is_absolute() else (repo_root / path).resolve()
    raw = (repo_root / ".aerobag-artifact-write-path").read_text(encoding="utf-8").strip()
    path = Path(raw)
    return path if path.is_absolute() else (repo_root / path).resolve()


def safe_ref_name(ref: str, sha: str) -> str:
    safe = re.sub(r"[^A-Za-z0-9_.-]+", "-", ref).strip("-")
    return f"{safe or 'ref'}-{sha[:12]}"


def build_timestamp() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def resolve_commit(repo_root: Path, ref: str) -> str:
    result = run(
        ["git", "rev-parse", "--verify", f"{ref}^{{commit}}"],
        cwd=repo_root,
        capture=True,
    )
    return result.stdout.strip()


def remove_worktree(repo_root: Path, path: Path) -> None:
    subprocess.run(
        ["git", "worktree", "remove", "--force", str(path)],
        cwd=repo_root,
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    if path.is_dir() and not path.is_symlink():
        shutil.rmtree(path)
    elif path.exists() or path.is_symlink():
        path.unlink()


def acquire_worktree_lock(worktree_root: Path):
    lock_path = worktree_root / ".coordinator.lock"
    lock_file = lock_path.open("a+", encoding="utf-8")
    try:
        fcntl.flock(lock_file.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        lock_file.close()
        raise RuntimeError(
            f"another multi-version publication owns worktree root {worktree_root}"
        ) from None
    return lock_file


def prune_worktree_metadata(repo_root: Path) -> None:
    run(["git", "worktree", "prune"], cwd=repo_root)


def remove_abandoned_worktrees(repo_root: Path, worktree_root: Path) -> None:
    for path in sorted(worktree_root.iterdir()):
        if path.name == ".coordinator.lock":
            continue
        print(f"removing abandoned multi-version worktree state {path}", flush=True)
        remove_worktree(repo_root, path)
    prune_worktree_metadata(repo_root)


def create_worktree(repo_root: Path, path: Path, sha: str) -> None:
    if path.exists():
        raise RuntimeError(f"ephemeral worktree path already exists: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    run(["git", "worktree", "add", "--detach", str(path), sha], cwd=repo_root)


def build_ref(
    *,
    repo_root: Path,
    ref: str,
    sha: str,
    worktree: Path,
    env: dict[str, str],
    build_root: Path,
    publish_label: str,
    publish_timestamp: str,
    release: bool,
    build_args: list[str],
) -> Path:
    create_worktree(repo_root, worktree, sha)
    cargo_command = ["cargo", "build"]
    if release:
        cargo_command.append("--release")
    cargo_command.extend(["-p", "preprocessor-cli"])
    run(cargo_command, cwd=worktree / PREPROCESSOR_DIR, env=env)

    target_profile = "release" if release else "debug"
    binary = Path(env["CARGO_TARGET_DIR"]) / target_profile / "preprocessor-cli"
    command = [
        str(binary),
        "build-product",
        "--build-root",
        str(build_root),
        "--publish-label",
        publish_label,
        "--publish-timestamp",
        publish_timestamp,
    ]
    command.extend(build_args)
    result = run(command, cwd=worktree / PREPROCESSOR_DIR, env=env, capture=True)
    print(result.stdout, end="" if result.stdout.endswith("\n") else "\n")

    product_artifacts = None
    for line in result.stdout.splitlines():
        match = PRODUCT_ARTIFACTS_RE.match(line.strip())
        if match:
            product_artifacts = Path(match.group(1)).resolve()
    if product_artifacts is None:
        raise RuntimeError(f"{ref} build did not report a product_artifacts path")
    if not product_artifacts.is_file():
        raise RuntimeError(f"{ref} reported missing manifest {product_artifacts}")
    return product_artifacts


def main() -> int:
    args = parse_args()
    repo_root = args.repo_root.resolve()
    root = artifact_root(repo_root)
    build_root = (args.build_root or root).resolve()
    target_dir = (args.target_dir or (root / "target")).resolve()
    worktree_root = (
        args.worktree_root.resolve()
        if args.worktree_root is not None
        else root / "worktrees" / "multi-version"
    )
    worktree_root.mkdir(parents=True, exist_ok=True)
    target_dir.mkdir(parents=True, exist_ok=True)
    worktree_lock = acquire_worktree_lock(worktree_root)
    remove_abandoned_worktrees(repo_root, worktree_root)
    run_worktree_root = worktree_root / f"run-{build_timestamp()}-{os.getpid()}"
    run_worktree_root.mkdir()

    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(target_dir)
    env.setdefault("AEROBAG_ARTIFACT_WRITE_PATH", str(root))

    build_args = args.build_args
    if build_args and build_args[0] == "--":
        build_args = build_args[1:]

    worktrees: list[Path] = []
    manifests: list[Path] = []
    try:
        for ref in args.refs:
            sha = resolve_commit(repo_root, ref)
            ref_name = safe_ref_name(ref, sha)
            worktree = run_worktree_root / ref_name
            timestamp = build_timestamp()
            worktrees.append(worktree)
            print(
                f"building ref={ref} sha={sha} publish_label={ref_name} "
                f"publish_timestamp={timestamp} worktree={worktree}",
                flush=True,
            )
            manifest = build_ref(
                repo_root=repo_root,
                ref=ref,
                sha=sha,
                worktree=worktree,
                env=env,
                build_root=build_root,
                publish_label=ref_name,
                publish_timestamp=timestamp,
                release=args.release,
                build_args=build_args,
            )
            manifests.append(manifest)

        target_profile = "release" if args.release else "debug"
        merge_binary = target_dir / target_profile / "preprocessor-cli"
        merge_command = [
            str(merge_binary),
            "merge-current-artifacts",
            "--build-root",
            str(build_root),
        ]
        if args.as_of_utc:
            merge_command.extend(["--as-of-utc", args.as_of_utc])
        for manifest in manifests:
            merge_command.extend(["--manifest", str(manifest)])
        result = run(merge_command, cwd=repo_root / PREPROCESSOR_DIR, env=env, capture=True)
        print(result.stdout, end="" if result.stdout.endswith("\n") else "\n")

        gc_command = [
            str(merge_binary),
            "gc",
            "--build-root",
            str(build_root),
        ]
        run(gc_command, cwd=repo_root / PREPROCESSOR_DIR, env=env, capture=True)
    finally:
        for worktree in reversed(worktrees):
            remove_worktree(repo_root, worktree)
        if run_worktree_root.exists():
            shutil.rmtree(run_worktree_root)
        prune_worktree_metadata(repo_root)
        worktree_lock.close()
        print(f"removed ephemeral worktrees under {worktree_root}", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
