#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import build_multi_version_publication as publication


class MultiVersionPublicationWorktreeTests(unittest.TestCase):
    def test_build_product_arguments_are_not_misclassified_as_refs(self) -> None:
        args = publication.parse_args(
            [
                "--primary-ref",
                "main",
                "--release",
                "main",
                "legacy-nav15",
                "--",
                "--profile",
                "production",
            ]
        )

        self.assertEqual(args.refs, ["main", "legacy-nav15"])
        self.assertEqual(args.build_args, ["--profile", "production"])

    def test_isolated_build_requires_and_records_a_results_destination(self) -> None:
        with self.assertRaises(SystemExit):
            publication.parse_args(
                ["--primary-ref", "release-a", "--no-activate", "release-a"]
            )
        args = publication.parse_args(
            [
                "--primary-ref",
                "release-a",
                "--no-activate",
                "--results-output",
                "/tmp/releases.json",
                "release-a",
            ]
        )
        self.assertTrue(args.no_activate)
        self.assertEqual(args.results_output, Path("/tmp/releases.json"))

    def test_build_results_pin_each_ref_to_commit_and_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "results.json"
            publication.write_build_results(
                output,
                [
                    publication.BuiltRevision(
                        ref="2026-08-22.1",
                        sha="a" * 40,
                        worktree=Path("/worktree"),
                        binary=Path("/binary"),
                        manifest=Path("/published/product_artifacts.json"),
                    )
                ],
            )
            document = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(document["releases"][0]["commit"], "a" * 40)
            self.assertEqual(
                document["releases"][0]["product_artifacts"],
                "/published/product_artifacts.json",
            )

    def test_primary_is_first_and_compatibility_order_is_input_independent(self) -> None:
        forward = publication.primary_first_refs(
            "main", ["legacy-nav15", "main", "legacy-nav16"]
        )
        reverse = publication.primary_first_refs(
            "main", ["legacy-nav16", "main", "legacy-nav15"]
        )

        self.assertEqual(forward, ["main", "legacy-nav15", "legacy-nav16"])
        self.assertEqual(reverse, forward)

    def test_primary_must_be_one_unique_publication_ref(self) -> None:
        with self.assertRaisesRegex(ValueError, "not present"):
            publication.primary_first_refs("main", ["legacy"])
        with self.assertRaisesRegex(ValueError, "duplicates"):
            publication.primary_first_refs("main", ["main", "main"])

    def test_merge_and_gc_commands_use_preserved_primary_binary(self) -> None:
        primary_binary = Path("/run/binaries/main/preprocessor-cli")
        build_root = Path("/artifacts")
        manifests = [Path("/manifests/main.json"), Path("/manifests/legacy.json")]

        merge = publication.merge_command(
            primary_binary, build_root, manifests, "2026-08-17T00:00:00Z"
        )
        gc = publication.gc_command(primary_binary, build_root)

        self.assertEqual(merge[0], str(primary_binary))
        self.assertEqual(gc[0], str(primary_binary))
        self.assertEqual(
            merge[-4:],
            [
                "--manifest",
                str(manifests[0]),
                "--manifest",
                str(manifests[1]),
            ],
        )

    def test_build_ref_runs_the_preserved_revision_binary(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            worktree = root / "worktree"
            preprocessor_dir = worktree / publication.PREPROCESSOR_DIR
            preprocessor_dir.mkdir(parents=True)
            target = root / "target"
            source_binary = target / "debug" / "preprocessor-cli"
            source_binary.parent.mkdir(parents=True)
            source_binary.write_bytes(b"primary executable")
            source_binary.chmod(0o755)
            preserved_binary = root / "preserved" / "preprocessor-cli"
            manifest = root / "product_artifacts.json"
            manifest.write_text("{}", encoding="utf-8")
            completed = [
                subprocess.CompletedProcess(["cargo"], 0, ""),
                subprocess.CompletedProcess(
                    [str(preserved_binary)],
                    0,
                    f"product_artifacts {manifest}\n",
                ),
            ]

            with mock.patch.object(publication, "create_worktree"), mock.patch.object(
                publication, "run", side_effect=completed
            ) as run:
                built = publication.build_ref(
                    repo_root=root,
                    ref="main",
                    sha="a" * 40,
                    worktree=worktree,
                    env={"CARGO_TARGET_DIR": str(target)},
                    build_root=root / "artifacts",
                    publish_label="main-aaaaaaaaaaaa",
                    publish_timestamp="20260817T000000Z",
                    release=False,
                    build_args=[],
                    preserved_binary=preserved_binary,
                )

            self.assertEqual(built.binary, preserved_binary)
            self.assertEqual(preserved_binary.read_bytes(), b"primary executable")
            self.assertEqual(run.call_args_list[1].args[0][0], str(preserved_binary))

    def test_publication_log_records_parseable_task_lifecycle_and_rotates(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "publication" / "master.log"
            path.parent.mkdir(parents=True)
            path.write_text("old log\n", encoding="utf-8")

            log = publication.PublicationLog(path)
            try:
                log.log(
                    "begin pid=1 build_root=/tmp/build publish_label=main "
                    "scheduler=multi_version_publication"
                )
                with log.task("merge-current-artifacts", manifests=2):
                    pass
                log.log(
                    "complete PASS current_artifacts=/tmp/build/published/current_artifacts.json"
                )
            finally:
                log.close()

            lines = path.read_text(encoding="utf-8").splitlines()
            self.assertIn(" begin pid=1 ", lines[0])
            self.assertIn(
                " task event=start id=merge-current-artifacts "
                "source=publication-coordinator manifests=2",
                lines[1],
            )
            self.assertIn(
                " task event=complete id=merge-current-artifacts "
                "source=publication-coordinator status=PASS manifests=2",
                lines[2],
            )
            self.assertIn(" complete PASS current_artifacts=", lines[3])
            rotated = list(path.parent.glob("master-*.log"))
            self.assertEqual(len(rotated), 1)
            self.assertEqual(rotated[0].read_text(encoding="utf-8"), "old log\n")

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
