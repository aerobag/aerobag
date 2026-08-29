#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path

import fetch_test_artifacts


class FetchTestArtifactsTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / "source"
        self.source.mkdir()
        self.git("init", "--quiet", "-b", "main")
        self.git("config", "user.name", "Fixture Test")
        self.git("config", "user.email", "fixture-test@example.invalid")
        (self.source / "alpha").mkdir()
        (self.source / "alpha" / "manifest.json").write_text(
            '{"schema_version": 2}\n', encoding="utf-8"
        )
        (self.source / "alpha" / "payload.dat").write_text(
            "selected\n", encoding="utf-8"
        )
        (self.source / "beta").mkdir()
        (self.source / "beta" / "payload.dat").write_text(
            "not selected\n", encoding="utf-8"
        )
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "fixture root")
        self.commit = self.git("rev-parse", "HEAD").stdout.strip()

    def git(self, *arguments: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["git", "-C", str(self.source), *arguments],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
        )

    def write_lock(self, contract_version: int = 2) -> Path:
        path = self.root / "test-artifacts.lock.json"
        path.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "repository": str(self.source),
                    "commit": self.commit,
                    "fixtures": {
                        "alpha": {
                            "path": "alpha",
                            "contract_version": contract_version,
                            "manifest": {
                                "path": "manifest.json",
                                "version_field": "schema_version",
                            },
                            "required_globs": ["*.dat"],
                        },
                        "beta": {
                            "path": "beta",
                            "contract_version": 1,
                            "required_globs": ["*.dat"],
                        },
                    },
                }
            )
            + "\n",
            encoding="utf-8",
        )
        return path

    def test_fetches_only_selected_fixture_at_exact_commit(self) -> None:
        lock = fetch_test_artifacts.load_lock(self.write_lock())
        destination = self.root / "checkout"

        fetch_test_artifacts.fetch(lock, ["alpha"], destination)

        self.assertEqual(
            self.commit,
            subprocess.run(
                ["git", "-C", str(destination), "rev-parse", "HEAD"],
                check=True,
                text=True,
                stdout=subprocess.PIPE,
            ).stdout.strip(),
        )
        self.assertTrue((destination / "alpha" / "payload.dat").is_file())
        self.assertFalse((destination / "beta").exists())

    def test_local_repository_cache_avoids_the_configured_remote(self) -> None:
        lock_path = self.write_lock()
        value = json.loads(lock_path.read_text(encoding="utf-8"))
        value["repository"] = "https://invalid.example.test/fixtures.git"
        lock_path.write_text(json.dumps(value), encoding="utf-8")
        lock = fetch_test_artifacts.load_lock(lock_path)
        destination = self.root / "cached-checkout"

        fetch_test_artifacts.fetch(
            lock,
            ["alpha"],
            destination,
            repository_cache=self.source,
        )

        self.assertEqual(
            self.commit,
            subprocess.run(
                ["git", "-C", str(destination), "rev-parse", "HEAD"],
                check=True,
                text=True,
                stdout=subprocess.PIPE,
            ).stdout.strip(),
        )

    def test_rejects_manifest_contract_mismatch(self) -> None:
        lock = fetch_test_artifacts.load_lock(self.write_lock(contract_version=3))

        with self.assertRaisesRegex(
            fetch_test_artifacts.LockError,
            "fixture alpha contract is 2; lock requires 3",
        ):
            fetch_test_artifacts.fetch(lock, ["alpha"], self.root / "checkout")

    def test_rejects_parent_traversal_in_fixture_path(self) -> None:
        lock_path = self.write_lock()
        value = json.loads(lock_path.read_text(encoding="utf-8"))
        value["fixtures"]["alpha"]["path"] = "../alpha"
        lock_path.write_text(json.dumps(value), encoding="utf-8")

        with self.assertRaisesRegex(
            fetch_test_artifacts.LockError, "normalized relative path"
        ):
            fetch_test_artifacts.load_lock(lock_path)


if __name__ == "__main__":
    unittest.main()
