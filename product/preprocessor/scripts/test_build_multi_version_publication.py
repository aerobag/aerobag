#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import subprocess
import tempfile
import unittest
from pathlib import Path

import build_multi_version_publication as publication


class MultiVersionPublicationWorktreeTests(unittest.TestCase):
    def test_abandoned_worktree_is_removed_before_new_checkout(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = self.create_repo(root / "repo")
            worktree_root = root / "worktrees"
            abandoned = worktree_root / "master"
            abandoned.mkdir(parents=True)
            (abandoned / ".git").write_text(
                "gitdir: /missing/repository/worktrees/master\n", encoding="utf-8"
            )
            (abandoned / "stale-file").write_text("stale", encoding="utf-8")

            publication.remove_abandoned_worktrees(repo, worktree_root)

            self.assertFalse(abandoned.exists())
            sha = self.git(repo, "rev-parse", "HEAD").stdout.strip()
            checkout = worktree_root / "run-test" / "master"
            publication.create_worktree(repo, checkout, sha)
            self.assertEqual(
                self.git(checkout, "rev-parse", "HEAD").stdout.strip(), sha
            )
            publication.remove_worktree(repo, checkout)
            publication.prune_worktree_metadata(repo)
            self.assertFalse(checkout.exists())

    def test_worktree_root_lock_rejects_concurrent_coordinator(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            worktree_root = Path(tmp)
            first = publication.acquire_worktree_lock(worktree_root)
            try:
                with self.assertRaisesRegex(RuntimeError, "another multi-version publication"):
                    publication.acquire_worktree_lock(worktree_root)
            finally:
                first.close()

    def create_repo(self, path: Path) -> Path:
        path.mkdir()
        self.git(path, "init", "--quiet")
        self.git(
            path,
            "-c",
            "user.name=Aerobag Test",
            "-c",
            "user.email=test@aerobag.invalid",
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "initial",
        )
        return path

    def git(self, cwd: Path, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["git", *args],
            cwd=cwd,
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )


if __name__ == "__main__":
    unittest.main()
