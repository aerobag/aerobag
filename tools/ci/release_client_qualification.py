#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Qualify the staged client revision against the candidate server checkout."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
import release_reconciler as releases
import service_contract

# Driver/harness code follows the client protocol. Fixture/server compilers and
# their source remain on the candidate revision throughout qualification.
HARNESS = ("tools/e2e", "ui/web-app/tests", "tools/ci/verify_release_e2e_apps.py")


def git(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=root, text=True).strip()


def selection(root: Path) -> dict:
    server = git(root, "rev-parse", "HEAD")
    desired = releases.load_desired_releases(root / "deploy/releases.json")
    client = server
    tag = desired.staging.tag if desired.staging else None
    # A normal main/PR push must not inherit a stale staging assignment.
    if tag and tag in git(root, "tag", "--points-at", server).splitlines():
        selected = releases.release_client_tag(root, tag)
        if selected != tag:
            client = git(root, "rev-parse", f"refs/tags/{selected}^{{commit}}")
            original = json.loads(git(root, "show", f"{client}:{service_contract.CURRENT}"))
            service_contract.require_equal(original, service_contract.load(root))
    return {"schema_version": 1, "server_commit": server, "client_commit": client}


def build(root: Path, output: Path) -> None:
    selected = selection(root)
    target_root = Path(os.environ.get("AEROBAG_UI_TARGET_ROOT", str(
        root / (root / "ui/target-root.txt").read_text().strip()))).resolve()
    environment = {**os.environ, "AEROBAG_E2E_CLIENT_SELECTED": "1",
                   "AEROBAG_UI_TARGET_ROOT": str(target_root)}
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="aerobag-client-qualification-") as directory:
        client_root = root
        retained = selected["client_commit"] != selected["server_commit"]
        if retained:
            client_root = Path(directory) / "client"
            subprocess.run(["git", "worktree", "add", "--detach", str(client_root), selected["client_commit"]], cwd=root, check=True)
        try:
            if retained:
                subprocess.run(["npm", "ci", "--prefix", str(client_root / "ui/web-app")], check=True)
            subprocess.run(["bash", str(client_root / "tools/ci/build_release_e2e_apps.sh"), str(output)],
                           cwd=client_root, env=environment, check=True)
            with (output / "client-harness.tar").open("wb") as stream:
                subprocess.run(["git", "archive", selected["client_commit"], "--", *HARNESS], cwd=root, stdout=stream, check=True)
        finally:
            if retained:
                subprocess.run(["git", "worktree", "remove", "--force", str(client_root)], cwd=root, check=True)
    selected["harness_sha256"] = hashlib.sha256((output / "client-harness.tar").read_bytes()).hexdigest()
    if retained:
        # The cloud test service, like the live-feed fixture compiler, belongs
        # to the candidate. Only the applications and semantic driver are old.
        target = target_root / "services"
        subprocess.run(["cargo", "build", "--manifest-path", str(root / "services/Cargo.toml"),
                        "-p", "aerobag-cloud-server"], check=True,
                       env={**os.environ, "CARGO_TARGET_DIR": str(target)})
        binary = output / "aerobag-cloud-serverd"
        shutil.copyfile(target / "debug/aerobag-cloud-serverd", binary)
        manifest = json.loads((output / "build-manifest.json").read_text())
        manifest["cloud_server"].update(size_bytes=binary.stat().st_size,
                                       sha256=hashlib.sha256(binary.read_bytes()).hexdigest())
        (output / "build-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    (output / "qualification.json").write_text(json.dumps(selected, indent=2) + "\n")


def consume(root: Path, output: Path) -> None:
    selected = json.loads((output / "qualification.json").read_text())
    manifest = json.loads((output / "build-manifest.json").read_text())
    if (selected["schema_version"] != 1 or selected["server_commit"] != git(root, "rev-parse", "HEAD")
            or manifest["git_commit"] != selected["client_commit"]):
        raise ValueError("qualification artifacts identify a different server/client pair")
    archive = output / "client-harness.tar"
    if hashlib.sha256(archive.read_bytes()).hexdigest() != selected["harness_sha256"]:
        raise ValueError("retained client harness digest mismatch")
    with tarfile.open(archive) as stream:
        members = stream.getmembers()
        for member in members:
            path = Path(member.name)
            allowed = any(member.name == prefix or member.name.startswith(prefix + "/")
                          or prefix.startswith(member.name + "/") for prefix in HARNESS)
            if not allowed or path.is_absolute() or ".." in path.parts or not (member.isfile() or member.isdir()):
                raise ValueError("unsafe client harness archive member")
        stream.extractall(root, members=members, filter="data")
    subprocess.run([sys.executable, str(root / "tools/ci/verify_release_e2e_apps.py"), str(output)], check=True)
    print(f"Qualifying client {selected['client_commit']} against server {selected['server_commit']}", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("build", "consume"))
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    (build if args.action == "build" else consume)(ROOT, args.output.resolve())
