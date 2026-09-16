# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import fcntl
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest
from unittest import mock

import preprocessor_tool_cache as cache


class ToolCacheTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.repo = self.root / "repo"
        self.repo.mkdir()
        self.git("init", "--quiet")
        (self.repo / "source.txt").write_text("original source")
        self.git("add", ".")
        self.git("-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "fixture")
        self.commit = self.git("rev-parse", "HEAD")
        self.binary = self.root / "built-binary"
        self.binary.write_bytes(b"exact revision executable")
        self.binary.chmod(0o755)
        compiler_patch = mock.patch.object(cache, "compile_binary", return_value=self.binary)
        self.compile = compiler_patch.start()
        self.addCleanup(compiler_patch.stop)
        self.real_identity = cache.build_identity
        identity_patch = mock.patch.object(cache, "build_identity", side_effect=lambda repo, commit, release, env, **kwargs:
                          {"schema_version": 1, "commit": commit, "profile": "release" if release else "debug",
                           "flags": env.get("RUSTFLAGS")})
        identity_patch.start()
        self.addCleanup(identity_patch.stop)
        self.store = cache.ToolCache(self.root / "cache", self.repo, self.root / "target", os.environ)

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.repo, text=True, stderr=subprocess.PIPE).strip()

    def test_hit_skips_cargo_and_keeps_legacy_checkout_at_original_path(self):
        with self.store:
            first = self.store.ensure(self.commit, release=True)
            second = self.store.ensure(self.commit, release=True)
            self.assertFalse(first.hit)
            self.assertTrue(second.hit)
            self.assertEqual(first.cwd, second.cwd)
            self.assertEqual((first.cwd / "source.txt").read_text(), "original source")
            self.assertEqual(self.compile.call_count, 1)

    def test_corrupt_binary_and_changed_checkout_are_rebuilt(self):
        with self.store:
            first = self.store.ensure(self.commit, release=True)
            first.binary.write_bytes(b"corrupt")
            self.assertFalse(self.store.ensure(self.commit, release=True).hit)
            (first.cwd / "source.txt").write_text("wrong release")
            self.assertFalse(self.store.ensure(self.commit, release=True).hit)
            self.assertEqual(self.compile.call_count, 3)

    def test_profile_and_build_flags_change_identity(self):
        with self.store:
            release = self.store.ensure(self.commit, release=True)
            debug = self.store.ensure(self.commit, release=False)
            self.store.env["RUSTFLAGS"] = "-C opt-level=1"
            changed = self.store.ensure(self.commit, release=True)
            self.assertEqual(len({release.key, debug.key, changed.key}), 3)

    def test_failure_does_not_leave_reusable_partial_install(self):
        self.compile.side_effect = RuntimeError("interrupted compilation")
        with self.store:
            with self.assertRaisesRegex(RuntimeError, "interrupted"):
                self.store.ensure(self.commit, release=True)
            self.assertEqual(list((self.store.root / "entries").iterdir()), [])

    def test_gc_preserves_active_commit_and_reclaims_retired_tool_and_checkout(self):
        with self.store:
            tool = self.store.ensure(self.commit, release=True)
            future = time.time() + 2 * cache.GRACE_SECONDS
            self.assertEqual(self.store.gc({self.commit}, now=future), [])
            self.assertEqual(self.store.gc(set(), now=future), [tool.binary.parent])
            self.assertFalse(tool.binary.exists())
            self.assertNotIn(str(tool.cwd), self.git("worktree", "list", "--porcelain"))

    def test_gc_preserves_controller_execution_lease(self):
        with self.store:
            tool = self.store.ensure(self.commit, release=True)
            with (tool.binary.parent / "use.lock").open("a") as lease:
                fcntl.flock(lease, fcntl.LOCK_SH)
                self.assertEqual(self.store.gc(set(), now=time.time() + 2 * cache.GRACE_SECONDS), [])
            self.assertTrue(tool.binary.exists())

    def test_concurrent_build_and_gc_cannot_enter_cache(self):
        with self.store:
            other = cache.ToolCache(self.store.root, self.repo, self.root / "target", os.environ)
            with self.assertRaisesRegex(RuntimeError, "in use"):
                with other:
                    self.fail("concurrent cache lease")
            cache.collect_retired_tools(self.store.root, self.repo, set(), now=time.time())

    def test_unknown_directories_and_symlinks_are_not_gc_targets(self):
        with self.store:
            unknown = self.store.root / "entries" / ("a" * 64)
            unknown.mkdir()
            sentinel = unknown / "user-data"
            sentinel.write_text("preserve")
            (self.store.root / "entries" / ("b" * 64)).symlink_to(self.repo)
            self.assertEqual(self.store.gc(set(), now=time.time() + 2 * cache.GRACE_SECONDS), [])
            self.assertEqual(sentinel.read_text(), "preserve")

    def test_symlinked_entries_namespace_is_rejected(self):
        self.store.root.mkdir()
        (self.store.root / "entries").symlink_to(self.repo)
        with self.assertRaisesRegex(ValueError, "entries must not be a symlink"):
            with self.store:
                self.fail("symlinked namespace accepted")
        self.assertTrue((self.repo / "source.txt").exists())

    def test_identity_resolves_historical_toolchain_and_actual_build_ancestors(self):
        toolchain = self.repo / cache.PREPROCESSOR / "rust-toolchain.toml"
        toolchain.parent.mkdir(parents=True)
        toolchain.write_text('[toolchain]\nchannel = "historical"\n')
        self.git("add", ".")
        self.git("-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "toolchain")
        commit = self.git("rev-parse", "HEAD")
        toolchain.write_text('[toolchain]\nchannel = "different-current-checkout"\n')
        configuration = self.store.root / ".cargo/config.toml"
        configuration.parent.mkdir(parents=True)
        configuration.write_text('[build]\nrustflags = ["-C", "opt-level=1"]\n')
        original_output = cache.output
        def result(command, cwd, env=None):
            if command[0] in {"rustc", "cargo"}:
                self.assertIn('channel = "historical"', (cwd / "rust-toolchain.toml").read_text())
                return "test compiler version"
            return original_output(command, cwd, env)
        with self.store, mock.patch.object(cache, "output", side_effect=result):
            identity = self.real_identity(self.repo, commit, True, {}, build_parent=self.store.root / "entries")
            self.assertEqual(identity["configuration"][str(configuration)], cache.digest(configuration))
            self.assertEqual(identity["rustc"], "test compiler version")
            self.assertFalse(list((self.store.root / "entries").iterdir()))

    def test_bundle_hit_checks_resources_and_does_not_retain_checkout(self):
        resource_crate = self.repo / cache.RESOURCE_CRATE
        resource_crate.parent.mkdir(parents=True)
        resource_crate.write_text("bundle capable")
        self.git("add", ".")
        self.git("-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "bundle support")
        commit = self.git("rev-parse", "HEAD")
        resources = self.root / "build-resources"
        resources.mkdir()
        (resources / "script.py").write_text("print('release-specific')")
        cache.atomic_json(resources / "manifest.json", {"schema_version": 1, "files": {"script.py": cache.digest(resources / "script.py")}})
        original_output = cache.output
        def result(command, cwd, env=None):
            if command[-1] == "tool-bundle-info":
                return json.dumps({"schema_version": 1, "resource_root": str(resources), "resource_sha256": cache.digest(resources / "manifest.json")})
            return original_output(command, cwd, env)
        with mock.patch.object(cache, "output", side_effect=result), self.store:
            first = self.store.ensure(commit, release=True)
            self.assertFalse((first.binary.parent / "checkout").exists())
            self.assertTrue(self.store.ensure(commit, release=True).hit)
            (first.cwd / "script.py").write_text("corrupt")
            self.assertFalse(self.store.ensure(commit, release=True).hit)
            self.assertEqual(self.compile.call_count, 2)


if __name__ == "__main__":
    unittest.main()
