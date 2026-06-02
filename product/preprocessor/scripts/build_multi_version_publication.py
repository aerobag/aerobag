#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


SCRIPT = Path(__file__).resolve()
REPO_ROOT = SCRIPT.parents[3]
PREPROCESSOR_DIR = Path("product/preprocessor")
VERSION_MANIFEST_RE = re.compile(r"^(?:current_artifacts|version_artifacts)\s+(.+)$")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Build one product publication per git ref, then merge their "
            "version_artifacts manifests into list-form current_artifacts.json."
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
        help="directory for temporary git worktrees (default: a temp directory)",
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
        help="shared packaged publication root passed to build-product",
    )
    parser.add_argument(
        "--profile",
        choices=["production", "validation"],
        default=None,
        help="product build profile passed to build-product",
    )
    parser.add_argument(
        "--as-of-utc",
        default=None,
        help="RFC3339 UTC timestamp for the merged current_artifacts files",
    )
    parser.add_argument(
        "--keep-worktrees",
        action="store_true",
        help="leave temporary worktrees in place for inspection",
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
    return subprocess.run(
        args,
        cwd=cwd,
        env=env,
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.STDOUT if capture else None,
    )


def artifact_root(repo_root: Path) -> Path:
    env_value = os.environ.get("AEROBAG_ARTIFACT_WRITE_PATH")
    if env_value:
        path = Path(env_value).expanduser()
        return path if path.is_absolute() else (repo_root / path).resolve()
    raw = (repo_root / ".aerobag-artifact-write-path").read_text(encoding="utf-8").strip()
    path = Path(raw)
    return path if path.is_absolute() else (repo_root / path).resolve()


def default_build_root(root: Path, profile: str | None) -> Path:
    if profile == "validation":
        return root / "published_packaged_validation"
    return root / "published_packaged"


def safe_ref_name(ref: str, sha: str) -> str:
    safe = re.sub(r"[^A-Za-z0-9_.-]+", "-", ref).strip("-")
    return f"{safe or 'ref'}-{sha[:12]}"


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
    if path.exists():
        shutil.rmtree(path)


def build_ref(
    *,
    repo_root: Path,
    ref: str,
    sha: str,
    worktree: Path,
    env: dict[str, str],
    build_root: Path,
    profile: str | None,
    build_args: list[str],
) -> Path:
    remove_worktree(repo_root, worktree)
    run(["git", "worktree", "add", "--detach", str(worktree), sha], cwd=repo_root)
    run(["cargo", "build", "-p", "preprocessor-cli"], cwd=worktree / PREPROCESSOR_DIR, env=env)

    binary = Path(env["CARGO_TARGET_DIR"]) / "debug" / "preprocessor-cli"
    command = [str(binary), "build-product", "--build-root", str(build_root)]
    if profile:
        command.extend(["--profile", profile])
    command.extend(build_args)
    result = run(command, cwd=worktree / PREPROCESSOR_DIR, env=env, capture=True)
    print(result.stdout, end="" if result.stdout.endswith("\n") else "\n")

    version_manifest = None
    for line in result.stdout.splitlines():
        match = VERSION_MANIFEST_RE.match(line.strip())
        if match:
            version_manifest = Path(match.group(1)).resolve()
    if version_manifest is None:
        raise RuntimeError(f"{ref} build did not report a version_artifacts path")
    if not version_manifest.is_file():
        raise RuntimeError(f"{ref} reported missing manifest {version_manifest}")
    return version_manifest


def main() -> int:
    args = parse_args()
    repo_root = args.repo_root.resolve()
    root = artifact_root(repo_root)
    build_root = (args.build_root or default_build_root(root, args.profile)).resolve()
    target_dir = (args.target_dir or (root / "target")).resolve()
    worktree_root_owned = args.worktree_root is None
    worktree_root = (
        Path(tempfile.mkdtemp(prefix="aerobag-multi-version-"))
        if args.worktree_root is None
        else args.worktree_root.resolve()
    )
    worktree_root.mkdir(parents=True, exist_ok=True)
    target_dir.mkdir(parents=True, exist_ok=True)

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
            worktree = worktree_root / safe_ref_name(ref, sha)
            worktrees.append(worktree)
            print(f"building ref={ref} sha={sha} worktree={worktree}", flush=True)
            manifest = build_ref(
                repo_root=repo_root,
                ref=ref,
                sha=sha,
                worktree=worktree,
                env=env,
                build_root=build_root,
                profile=args.profile,
                build_args=build_args,
            )
            manifests.append(manifest)

        merge_binary = target_dir / "debug" / "preprocessor-cli"
        merge_command = [str(merge_binary), "merge-current-artifacts", "--build-root", str(build_root)]
        if args.profile:
            merge_command.extend(["--profile", args.profile])
        if args.as_of_utc:
            merge_command.extend(["--as-of-utc", args.as_of_utc])
        for manifest in manifests:
            merge_command.extend(["--manifest", str(manifest)])
        result = run(merge_command, cwd=repo_root / PREPROCESSOR_DIR, env=env, capture=True)
        print(result.stdout, end="" if result.stdout.endswith("\n") else "\n")
    finally:
        if args.keep_worktrees:
            print(f"kept worktrees under {worktree_root}", flush=True)
        else:
            for worktree in worktrees:
                remove_worktree(repo_root, worktree)
            if worktree_root_owned:
                shutil.rmtree(worktree_root, ignore_errors=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
