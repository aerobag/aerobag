#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Real tool-cache/relocation test; builds a disposable snapshot of this worktree.

Separate from cheap checks: requires the native compiler and product runtime
tools (GDAL, Ghostscript, Python dependencies). Does not fetch FAA products.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "tools"))
import preprocessor_tool_cache as cache  # noqa: E402


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", type=Path, required=True, help="shared Cargo dependency target directory")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="aerobag-tool-relocation-") as temporary:
        root = Path(temporary)
        source = root / "snapshot"
        source.mkdir()
        names = subprocess.check_output(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=REPO).split(b"\0")
        for raw in filter(None, names):
            relative = Path(os.fsdecode(raw))
            original = REPO / relative
            if not original.exists():
                continue
            target = source / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            if original.is_symlink():
                target.symlink_to(os.readlink(original))
            else:
                shutil.copy2(original, target)
        subprocess.run(["git", "init", "--quiet"], cwd=source, check=True)
        subprocess.run(["git", "add", "."], cwd=source, check=True)
        subprocess.run(["git", "-c", "user.name=Relocation test", "-c", "user.email=test@aerobag.invalid", "commit", "-qm", "Disposable relocation fixture"], cwd=source, check=True)
        commit = cache.output(["git", "rev-parse", "HEAD"], source)
        env = dict(os.environ, AEROBAG_ARTIFACT_WRITE_PATH=str(root / "artifacts"), PYTHONDONTWRITEBYTECODE="1")
        env.pop("AEROBAG_PREPROCESSOR_RESOURCE_ROOT", None)
        with cache.ToolCache(root / "tools", source, args.target, env) as store:
            first = store.ensure(commit, release=False)
            assert not first.hit
            assert not (first.binary.parent / "checkout").exists(), "compilation checkout must be gone"
            before = json.loads(cache.output([str(first.binary), "verify-tool-bundle"], root, env))
            # A hit must work even when compiling is impossible.
            from unittest import mock
            with mock.patch.object(cache, "compile_binary", side_effect=AssertionError("warm hit invoked Cargo")):
                assert store.ensure(commit, release=False).hit
            relocated = root / "unrelated" / "tool"
            shutil.copytree(first.binary.parent, relocated)
            after = json.loads(cache.output([str(relocated / "preprocessor-cli"), "verify-tool-bundle"], root, env))
            assert before == after, "relocation changed cache-key source identities"
            info = json.loads(cache.output([str(relocated / "preprocessor-cli"), "tool-bundle-info"], root, env))
            assert Path(info["resource_root"]) == relocated / "resources"
            # Exercise both bundled TPP scripts on a real, tiny PDF.
            pdf = root / "fixture.pdf"
            subprocess.run(["python3", "-c", "from pypdf import PdfWriter; import sys; w=PdfWriter(); w.add_blank_page(width=300,height=400); w.write(sys.argv[1])", str(pdf)], check=True, env=env)
            scripts = relocated / "resources/product/preprocessor/preprocessor-tpp/scripts"
            pages = json.loads(cache.output(["python3", str(scripts / "find_plate_pages.py"), str(pdf), "TEST"], root, env))
            assert pages == {"TEST": []}
            json.loads(cache.output(["python3", str(scripts / "detect_landscape_rotation.py"), "--json", str(pdf)], root, env))
            terrain = relocated / "resources/product/preprocessor/preprocessor-cli/scripts"
            for script in ["build_water_mask_tiles.py", "build_shaded_relief_tiles.py"]:
                cache.output(["python3", str(terrain / script), "--help"], root, env)
            # Corrupt resources must not silently fall back to Cargo's good copy.
            (scripts / "find_plate_pages.py").write_text("corrupt")
            rejected = subprocess.run([str(relocated / "preprocessor-cli"), "tool-bundle-info"], cwd=root, env=env, capture_output=True, text=True)
            assert rejected.returncode and "mismatch" in rejected.stderr
        print("PASS: real build; checkout removed; warm hit without compilation; relocated fingerprints, chart resources, TPP and terrain scripts; corruption rejected")


if __name__ == "__main__":
    main()
