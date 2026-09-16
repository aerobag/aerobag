#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Immutable per-build preprocessors, with checkout-backed historical tools.

The cache lock is a lease: hold it through execution, not just installation.
GC takes that same lock nonblocking and never removes tools in use. Entries
are owned by this module, not by the product artifact/node cache.
"""
from __future__ import annotations

import argparse
from dataclasses import dataclass
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time

SCHEMA = 1
CACHE_RELATIVE = "preprocessor-tools"
PREPROCESSOR = Path("product/preprocessor")
RESOURCE_CRATE = PREPROCESSOR / "preprocessor-resources/Cargo.toml"
GRACE_SECONDS = 24 * 3600


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def canonical(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def output(args: list[str], cwd: Path, env: dict | None = None) -> str:
    return subprocess.check_output(args, cwd=cwd, env=env, text=True).strip()


def atomic_json(path: Path, value: object) -> None:
    temporary = path.with_name(path.name + ".tmp")
    with temporary.open("wb") as stream:
        stream.write(canonical(value))
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)


def build_identity(repo: Path, commit: str, release: bool, env: dict, *, build_parent: Path) -> dict:
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("tool cache requires an exact source commit")
    # Capture outside-the-commit Cargo configuration and environment overrides.
    cargo_home = Path(env.get("CARGO_HOME", str(Path.home() / ".cargo")))
    configs = {}
    for base in [cargo_home, build_parent, *build_parent.parents]:
        for name in ("config", "config.toml"):
            path = base / name if base == cargo_home else base / ".cargo" / name
            if path.is_file():
                configs[str(path)] = digest(path)
    variables = {key: value for key, value in sorted(env.items()) if
                 key in {"RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER",
                         "RUSTC_WORKSPACE_WRAPPER", "RUSTUP_TOOLCHAIN", "CARGO_BUILD_TARGET", "CC", "CXX", "AR",
                         "CFLAGS", "CXXFLAGS", "CPPFLAGS", "LDFLAGS"}
                 or key.startswith(("CARGO_PROFILE_", "CARGO_TARGET_")) and key != "CARGO_TARGET_DIR"}
    # Resolve Rust in the historical source's toolchain context, not the current
    # controller checkout. Probe beside the build entries so outside-the-repo
    # toolchain/configuration inheritance also matches actual compilation.
    candidates = [str(base / name) for base in (PREPROCESSOR, Path("product"), Path("."))
                  for name in ("rust-toolchain", "rust-toolchain.toml")]
    tracked = set(output(["git", "ls-tree", "-r", "--name-only", commit, "--", *candidates], repo).splitlines())
    with tempfile.TemporaryDirectory(prefix=".identity-", dir=build_parent) as temporary:
        probe = Path(temporary)
        for relative in candidates:
            if relative in tracked:
                (probe / Path(relative).name).write_text(output(["git", "show", f"{commit}:{relative}"], repo) + "\n")
                break
        rustc = output([env.get("RUSTC", "rustc"), "-vV"], probe, env)
        cargo = output(["cargo", "--version"], probe, env)
    return {"schema_version": SCHEMA, "commit": commit,
            "profile": "release" if release else "debug",
            "rustc": rustc, "cargo": cargo,
            "configuration": configs, "environment": variables}


def compile_binary(checkout: Path, env: dict, *, release: bool) -> Path:
    command = ["cargo", "build", "--locked", "--message-format=json-render-diagnostics", "-p", "preprocessor-cli"]
    if release:
        command.append("--release")
    result = subprocess.run(command, cwd=checkout / PREPROCESSOR, env=env,
                            check=True, text=True, stdout=subprocess.PIPE)
    binaries = [Path(message["executable"]) for line in result.stdout.splitlines()
                if (message := json.loads(line)).get("reason") == "compiler-artifact"
                and message.get("target", {}).get("name") == "preprocessor-cli"
                and message.get("executable")]
    if len(binaries) != 1:
        raise RuntimeError("Cargo did not report one preprocessor executable")
    return binaries[0]


def install_resources(binary: Path, cwd: Path, env: dict) -> str:
    info = json.loads(output([str(binary), "tool-bundle-info"], cwd, env))
    if info["schema_version"] != 1:
        raise ValueError("unsupported tool resource bundle")
    shutil.copytree(info["resource_root"], binary.parent / "resources")
    # Revalidate against the executable's embedded manifest identity.
    output([str(binary), "tool-bundle-info"], binary.parent, env)
    return info["resource_sha256"]


@dataclass(frozen=True)
class Tool:
    binary: Path
    cwd: Path
    key: str
    hit: bool


class ToolCache:
    def __init__(self, root: Path, repo: Path, target: Path, env: dict):
        if root.is_symlink():
            raise ValueError("tool cache root must not be a symlink")
        self.root, self.repo, self.target = root.resolve(), repo.resolve(), target.resolve()
        self.env = dict(env, CARGO_TARGET_DIR=str(self.target), PYTHONDONTWRITEBYTECODE="1")
        # A release must never inherit another binary's resource override.
        self.env.pop("AEROBAG_PREPROCESSOR_RESOURCE_ROOT", None)
        self.lock = None

    def __enter__(self):
        self.root.mkdir(parents=True, exist_ok=True)
        if self.root.is_symlink():
            raise ValueError("tool cache root must not be a symlink")
        entries = self.root / "entries"
        if entries.is_symlink():
            raise ValueError("tool cache entries must not be a symlink")
        entries.mkdir(exist_ok=True)
        self.lock = (self.root / "cache.lock").open("a")
        try:
            fcntl.flock(self.lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BaseException:
            self.lock.close()
            self.lock = None
            raise RuntimeError("preprocessor tool cache is in use; retry after the current build") from None
        return self

    def __exit__(self, *_):
        self.lock.close()
        self.lock = None

    def remove(self, entry: Path) -> None:
        if entry.parent != self.root / "entries" or not re.fullmatch(r"[0-9a-f]{64}", entry.name) or entry.is_symlink():
            raise ValueError(f"not an owned tool entry: {entry}")
        owner = json.loads((entry / "owner.json").read_text())
        if hashlib.sha256(canonical(owner)).hexdigest() != entry.name:
            raise ValueError(f"tool ownership mismatch: {entry}")
        with (entry / "use.lock").open("a") as lease:
            try:
                fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                raise RuntimeError(f"tool entry is in use: {entry}") from None
            checkout = entry / "checkout"
            if checkout.exists():
                subprocess.run(["git", "worktree", "remove", "--force", str(checkout)],
                               cwd=self.repo, check=True, stdout=subprocess.DEVNULL)
            shutil.rmtree(entry)

    def valid(self, entry: Path, identity: dict) -> bool:
        try:
            if entry.is_symlink():
                return False
            record = json.loads((entry / "tool-entry.json").read_text())
            if record["identity"] != identity or json.loads((entry / "owner.json").read_text()) != identity:
                return False
            binary = entry / "preprocessor-cli"
            if binary.is_symlink() or not os.access(binary, os.X_OK) or digest(binary) != record["binary_sha256"]:
                return False
            if record["mode"] == "bundle":
                root = entry / "resources"
                manifest = root / "manifest.json"
                if root.is_symlink() or manifest.is_symlink() or digest(manifest) != record["resource_sha256"]:
                    return False
                files = json.loads(manifest.read_text())["files"]
                for name, expected in files.items():
                    path = root / name
                    if path.resolve().is_relative_to(root) is False or path.is_symlink() or digest(path) != expected:
                        return False
            elif record["mode"] == "checkout":
                checkout = entry / "checkout"
                if output(["git", "rev-parse", "HEAD"], checkout) != identity["commit"]:
                    return False
                if output(["git", "status", "--porcelain", "--untracked-files=all"], checkout):
                    return False
            else:
                return False
            return True
        except (OSError, ValueError, KeyError, TypeError, AttributeError, subprocess.SubprocessError):
            return False

    def ensure(self, commit: str, *, release: bool) -> Tool:
        if self.lock is None:
            raise RuntimeError("tool cache must remain leased through execution")
        identity = build_identity(self.repo, commit, release, self.env, build_parent=self.root / "entries")
        key = hashlib.sha256(canonical(identity)).hexdigest()
        entry = self.root / "entries" / key
        hit = self.valid(entry, identity)
        if not hit:
            if entry.exists():
                self.remove(entry)
            entry.mkdir()
            atomic_json(entry / "owner.json", identity)
            checkout = entry / "checkout"
            try:
                subprocess.run(["git", "worktree", "add", "--detach", str(checkout), commit],
                               cwd=self.repo, check=True, stdout=subprocess.DEVNULL)
                print(f"preprocessor-tool-cache miss commit={commit} key={key}", flush=True)
                binary = compile_binary(checkout, self.env, release=release)
                installed = entry / "preprocessor-cli"
                shutil.copy2(binary, installed)
                record = {"identity": identity, "binary_sha256": digest(installed), "mode": "checkout"}
                if (checkout / RESOURCE_CRATE).is_file():
                    record.update(mode="bundle", resource_sha256=install_resources(installed, checkout, self.env))
                    subprocess.run(["git", "worktree", "remove", "--force", str(checkout)], cwd=self.repo, check=True)
                atomic_json(entry / "tool-entry.json", record)
                if not self.valid(entry, identity):
                    raise ValueError("new tool bundle failed integrity verification")
            except BaseException:
                self.remove(entry)
                raise
        else:
            print(f"preprocessor-tool-cache hit commit={commit} key={key}", flush=True)
        atomic_json(entry / "last-used.json", {"at": time.time()})
        cwd = entry / "checkout" if (entry / "checkout").exists() else entry / "resources"
        return Tool(entry / "preprocessor-cli", cwd, key, hit)

    def gc(self, protected_commits: set[str], *, now: float | None = None) -> list[Path]:
        if self.lock is None:
            raise RuntimeError("GC requires the cache lease")
        now = time.time() if now is None else now
        entries = []
        for entry in (self.root / "entries").iterdir():
            if not re.fullmatch(r"[0-9a-f]{64}", entry.name) or entry.is_symlink():
                continue
            try:
                owner = json.loads((entry / "owner.json").read_text())
                if hashlib.sha256(canonical(owner)).hexdigest() != entry.name:
                    continue
                used = float(json.loads((entry / "last-used.json").read_text())["at"]) if (entry / "last-used.json").is_file() else entry.stat().st_mtime
                entries.append((entry, owner["commit"], used))
            except (OSError, ValueError, KeyError, TypeError):
                continue  # Unknown ownership is not permission to delete.
        # Keep the most recently used identity per active commit; old toolchain
        # variants and retired releases get the same bounded grace period.
        latest = {commit: max(used for _, other, used in entries if other == commit)
                  for commit in protected_commits if any(other == commit for _, other, _ in entries)}
        removed = []
        for entry, commit, used in entries:
            if now - used < GRACE_SECONDS or latest.get(commit) == used:
                continue
            try:
                self.remove(entry)
            except RuntimeError:
                continue
            removed.append(entry)
        return removed


def collect_retired_tools(root: Path, repo: Path, protected_commits: set[str], *, now: float) -> None:
    if not root.exists():
        return
    cache = ToolCache(root, repo, root / "unused-target", os.environ)
    try:
        cache.__enter__()
    except RuntimeError:
        return  # An active builder holds the lease; maintenance retries later.
    try:
        for entry in cache.gc(protected_commits, now=now):
            print(f"Removed retired preprocessor tool {entry.name}", flush=True)
    finally:
        cache.__exit__()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--target", type=Path, required=True)
    parser.add_argument("--ref", required=True)
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--path-output", type=Path, required=True)
    args = parser.parse_args()
    commit = output(["git", "rev-parse", f"{args.ref}^{{commit}}"], args.repo)
    with ToolCache(args.root, args.repo, args.target, os.environ) as cache:
        tool = cache.ensure(commit, release=args.release)
        args.path_output.write_text(str(tool.binary) + "\n")


if __name__ == "__main__":
    main()
