# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

import product_archive
from product_archive import BorgStore, DailyCapture, atomic_json, read_events, utc


class Source:
    """An immutable-version source that can advance during a baseline copy."""

    identity = "release/v3"

    def __init__(self):
        self.values = [1]
        self.on_baseline = lambda: None

    def heads(self):
        return {"metars": {"version": str(len(self.values))}}

    def baseline(self, product, head, target):
        value = self.values[int(head["version"]) - 1]
        (target / "value.json").write_text(json.dumps(value))
        self.on_baseline()
        return {"kind": "baseline", "head": head, "payload": "value.json"}

    def updates(self, product, previous, head):
        for index in range(int(previous["version"]), int(head["version"])):
            yield {"version": str(index + 1)}

    def update(self, product, previous, head, target):
        delta = self.values[int(head["version"]) - 1] - self.values[int(previous["version"]) - 1]
        (target / "value.json").write_text(json.dumps(delta))
        return {"kind": "delta", "head": head, "payload": "value.json"}


def replay(batch):
    value = None
    versions = []
    for event in read_events(batch):
        payload = json.loads((batch / event["directory"] / event["payload"]).read_text())
        value = payload if event["kind"] == "baseline" else value + payload
        versions.append(event["head"]["version"])
    return value, versions


class DayBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = Source()
        self.capture = DailyCapture(self.root, max_bytes=10_000_000)

    def test_update_during_rollover_baseline_is_not_lost(self):
        first = self.capture.collect(self.source, "2026-10-09T23:59:59Z")
        self.source.values.append(2)
        self.source.on_baseline = lambda: self.source.values.append(3)
        second = self.capture.collect(self.source, "2026-10-10T00:00:00Z")
        self.assertEqual(replay(first), (1, ["1"]))
        self.assertEqual(replay(second), (3, ["1", "2", "3"]))
        self.assertTrue((first / "sealed.json").is_file())
        self.assertFalse((second / "sealed.json").exists())

    def test_new_day_has_no_dependency_on_previous_day(self):
        import shutil
        first = self.capture.collect(self.source, "2026-10-09T23:59:59Z")
        self.source.values.append(9)
        self.capture.collect(self.source, "2026-10-09T23:59:59Z")
        second = self.capture.collect(self.source, "2026-10-10T00:00:00Z")
        shutil.rmtree(first)
        self.assertEqual(replay(second), (9, ["2"]))
        self.source.values.append(10)
        self.capture.collect(self.source, "2026-10-10T00:00:01Z")
        self.assertEqual(replay(second), (10, ["2", "3"]))

    def test_failed_baseline_does_not_swap_or_seal(self):
        first = self.capture.collect(self.source, "2026-10-09T23:59:59Z")
        def fail():
            raise OSError("disk unavailable")
        self.source.on_baseline = fail
        with self.assertRaises(OSError):
            self.capture.collect(self.source, "2026-10-10T00:00:00Z")
        self.assertEqual(self.capture.active(self.source.identity), first)
        self.assertFalse((first / "sealed.json").exists())
        self.source.on_baseline = lambda: None
        second = self.capture.collect(self.source, "2026-10-10T00:00:01Z")
        self.assertEqual(replay(second), (1, ["1"]))

    def test_crash_after_pointer_swap_recovers_old_seal(self):
        first = self.capture.collect(self.source, "2026-10-09T23:59:59Z")
        def crash(point):
            if point == "pointer_committed":
                raise RuntimeError("power loss")
        self.capture.checkpoint = crash
        with self.assertRaises(RuntimeError):
            self.capture.collect(self.source, "2026-10-10T00:00:00Z")
        self.source.values.append(4)
        restarted = DailyCapture(self.root, max_bytes=10_000_000)
        second = restarted.collect(self.source, "2026-10-10T00:00:01Z")
        self.assertTrue((first / "sealed.json").exists())
        self.assertEqual(replay(first), (1, ["1"]))
        self.assertEqual(replay(second), (4, ["1", "2"]))

    def test_restart_does_not_duplicate_updates(self):
        batch = self.capture.collect(self.source, "2026-10-09T12:00:00Z")
        self.source.values.extend([2, 3, 4])
        self.capture.collect(self.source, "2026-10-09T12:00:01Z")
        DailyCapture(self.root, max_bytes=10_000_000).collect(self.source, "2026-10-09T12:00:02Z")
        self.assertEqual(replay(batch), (4, ["1", "2", "3", "4"]))

    def test_clock_rollback_does_not_reopen_old_day(self):
        self.capture.collect(self.source, "2026-10-10T00:00:00Z")
        with self.assertRaisesRegex(ValueError, "backward"):
            self.capture.collect(self.source, "2026-10-09T23:59:59Z")

    def test_event_directory_is_durable_before_index_commit(self):
        with mock.patch.object(product_archive, "fsync_dir", wraps=product_archive.fsync_dir) as sync:
            def check(point):
                if point == "payload_durable":
                    directories = [Path(call.args[0]).name for call in sync.call_args_list]
                    self.assertIn("events", directories)
            self.capture.checkpoint = check
            self.capture.collect(self.source, "2026-10-09T12:00:00Z")

    def test_failure_after_atomic_replace_never_deletes_active_batch(self):
        first = self.capture.collect(self.source, "2026-10-09T23:59:59Z")
        def interrupted(path, value):
            atomic_json(path, value)
            if path == self.capture.pointer(self.source.identity):
                raise OSError("directory fsync interrupted after rename")
        with mock.patch.object(product_archive, "atomic_json", side_effect=interrupted):
            with self.assertRaises(OSError):
                self.capture.collect(self.source, "2026-10-10T00:00:00Z")
        second = self.capture.collect(self.source, "2026-10-10T00:00:01Z")
        self.assertNotEqual(first, second)
        self.assertEqual(replay(second), (1, ["1"]))
        self.assertTrue((first / "sealed.json").exists())

    def test_update_commit_crashes_are_exactly_once_on_restart(self):
        for point in ("payload_durable", "event_committed"):
            with self.subTest(point=point):
                root = self.root / point
                capture = DailyCapture(root, max_bytes=10_000_000)
                source = Source()
                batch = capture.collect(source, "2026-10-09T12:00:00Z")
                source.values.append(5)
                def crash(at):
                    if at == point:
                        raise RuntimeError(point)
                capture.checkpoint = crash
                with self.assertRaises(RuntimeError):
                    capture.collect(source, "2026-10-09T12:00:01Z")
                capture = DailyCapture(root, max_bytes=10_000_000)
                capture.collect(source, "2026-10-09T12:00:02Z")
                self.assertEqual(replay(batch), (5, ["1", "2"]))
                self.assertEqual(len(list((batch / "events").iterdir())), 2)

    def test_unpublished_baselines_and_orphan_payloads_are_recovered(self):
        batch = self.capture.collect(self.source, "2026-10-09T12:00:00Z")
        (batch / "events/orphan").mkdir()
        abandoned = self.root / "batches/unpublished"
        abandoned.mkdir()
        atomic_json(abandoned / "batch.json", {"source": self.source.identity})
        self.capture.collect(self.source, "2026-10-09T12:00:01Z")
        self.assertFalse(abandoned.exists())
        self.assertFalse((batch / "events/orphan").exists())
        self.assertEqual(replay(batch), (1, ["1"]))

    def test_actual_process_exit_at_rollover_commit_boundaries(self):
        for point in ("payload_durable", "event_committed", "baselines_durable", "pointer_committed"):
            with self.subTest(point=point):
                root = self.root / point
                capture = DailyCapture(root, max_bytes=10_000_000)
                source = Source()
                old = capture.collect(source, "2026-10-09T23:59:59Z")
                child = os.fork()
                if child == 0:
                    capture.checkpoint = lambda at: os._exit(67) if at == point else None
                    capture.collect(source, "2026-10-10T00:00:00Z")
                    os._exit(0)
                _, status = os.waitpid(child, 0)
                self.assertEqual(os.waitstatus_to_exitcode(status), 67)
                source.values.append(3)
                new = DailyCapture(root, max_bytes=10_000_000).collect(source, "2026-10-10T00:00:01Z")
                self.assertEqual(replay(new), (3, ["1", "2"]))
                self.assertEqual(replay(old), (1, ["1"]))
                self.assertEqual(len(list((root / "batches").iterdir())), 2)

    def test_long_outage_crossing_midnight_latches_gap(self):
        self.source.history_seconds = 30
        self.capture.collect(self.source, "2026-10-09T12:00:00Z")
        self.capture.collect(self.source, "2026-10-10T12:00:00Z")
        self.assertEqual(len(list((self.root / "gaps").glob("*.json"))), 1)
        self.capture.collect(self.source, "2026-10-10T12:00:01Z")
        self.assertEqual(len(list((self.root / "gaps").glob("*.json"))), 1)

    def test_quota_failure_keeps_old_pointer_and_data(self):
        batch = self.capture.collect(self.source, "2026-10-09T23:59:59Z")
        self.capture.max_bytes = 1
        with self.assertRaisesRegex(OSError, "quota"):
            self.capture.collect(self.source, "2026-10-10T00:00:00Z")
        self.assertEqual(self.capture.active(self.source.identity), batch)
        self.assertEqual(replay(batch), (1, ["1"]))
        self.assertFalse((batch / "sealed.json").exists())

    def test_late_observed_yesterday_update_lands_before_seal(self):
        class TimedSource(Source):
            def updates(self, product, previous, head):
                times = {"2": "2026-10-09T23:59:59Z", "3": "2026-10-10T00:00:00Z"}
                for update in super().updates(product, previous, head):
                    yield {**update, "mtime_ns": int(utc(times[update["version"]]).timestamp() * 1e9)}
        source = TimedSource()
        old = self.capture.collect(source, "2026-10-09T23:59:58Z")
        source.values.extend([2, 3])
        new = self.capture.collect(source, "2026-10-10T00:00:02Z")
        self.assertEqual(replay(old), (2, ["1", "2"]))
        self.assertEqual(replay(new), (3, ["2", "3"]))

    def test_retirement_recovers_incomplete_rotation_before_sealing(self):
        old = self.capture.collect(self.source, "2026-10-09T23:59:59Z")
        def crash(point):
            if point == "pointer_committed":
                raise RuntimeError("crash")
        self.capture.checkpoint = crash
        with self.assertRaises(RuntimeError):
            self.capture.collect(self.source, "2026-10-10T00:00:00Z")
        new = self.capture.active(self.source.identity)
        self.capture.retire(set(), "2026-10-10T00:00:01Z")
        self.assertIsNone(self.capture.active(self.source.identity))
        self.assertTrue((old / "sealed.json").exists())
        self.assertTrue((new / "sealed.json").exists())


@unittest.skipUnless(shutil.which("borg"), "Borg integration requires borgbackup")
class BorgRestoreTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        environment = mock.patch.dict(os.environ, {
            "BORG_CACHE_DIR": str(self.root / "cache"),
            "BORG_SECURITY_DIR": str(self.root / "security"),
            "BORG_PASSPHRASE": "",
        })
        environment.start()
        self.addCleanup(environment.stop)
        self.store = BorgStore(self.root / "repo", max_bytes=100_000_000, reserve_bytes=0)

    def test_real_restore_after_rotation_and_deletion_of_previous_day(self):
        capture = DailyCapture(self.root / "buffer", max_bytes=10_000_000)
        source = Source()
        old = capture.collect(source, "2026-10-09T23:59:59Z")
        source.values.append(4)
        new = capture.collect(source, "2026-10-10T00:00:00Z")
        self.store.commit_sealed(capture)
        self.assertFalse(old.exists())
        self.assertTrue(new.exists())
        self.store.command("delete", f"{self.store.repository}::live-{old.name}")
        capture.retire(set(), "2026-10-10T23:59:59Z")
        self.store.commit_sealed(capture)
        self.assertFalse(new.exists())
        output = self.root / "restored"
        output.mkdir()
        self.store.command("extract", f"{self.store.repository}::live-{new.name}", cwd=output)
        self.assertEqual(replay(output), (4, ["1", "2"]))

    def test_failed_borg_then_retry_verifies_existing_archive_before_deleting_buffer(self):
        capture = DailyCapture(self.root / "buffer", max_bytes=10_000_000)
        source = Source()
        batch = capture.collect(source, "2026-10-09T12:00:00Z")
        capture.retire(set(), "2026-10-09T12:01:00Z")
        original = self.store.command
        def failing(*args, **kwargs):
            if args[0] == "extract":
                raise subprocess.CalledProcessError(2, "borg extract --dry-run")
            return original(*args, **kwargs)
        with mock.patch.object(self.store, "command", side_effect=failing):
            with self.assertRaises(subprocess.CalledProcessError):
                self.store.commit_sealed(capture)
        self.assertTrue(batch.exists())
        self.assertEqual(len(self.store.archives()), 1)
        self.store.commit_sealed(capture)
        self.assertFalse(batch.exists())
        self.assertEqual(len(self.store.archives()), 1)

    def test_next_rotation_during_borg_verification_keeps_new_events(self):
        capture = DailyCapture(self.root / "buffer", max_bytes=10_000_000)
        source = Source()
        first = capture.collect(source, "2026-10-09T23:59:59Z")
        second = capture.collect(source, "2026-10-10T00:00:00Z")
        original = self.store.command
        rotated = []
        def verify_while_collecting(*args, **kwargs):
            if args[0] == "extract" and not rotated:
                source.values.append(7)
                rotated.append(capture.collect(source, "2026-10-11T00:00:00Z"))
            return original(*args, **kwargs)
        with mock.patch.object(self.store, "command", side_effect=verify_while_collecting):
            self.store.commit_sealed(capture)
        self.assertFalse(first.exists())
        self.assertFalse(second.exists())
        self.assertEqual(replay(rotated[0]), (7, ["1", "2"]))
        restored = self.root / "restored"
        restored.mkdir()
        self.store.command("extract", f"{self.store.repository}::live-{first.name}", cwd=restored)
        self.assertEqual(replay(restored), (1, ["1"]))

    def test_create_warning_leaving_partial_named_archive_is_rebuilt_on_retry(self):
        capture = DailyCapture(self.root / "buffer", max_bytes=10_000_000)
        source = Source()
        batch = capture.collect(source, "2026-10-09T12:00:00Z")
        capture.retire(set(), "2026-10-09T12:01:00Z")
        original = self.store.command
        def incomplete(*args, **kwargs):
            if args[0] == "create":
                original(*args[:-1], "index.sqlite", **kwargs)
                raise subprocess.CalledProcessError(1, "borg create: skipped an unreadable file")
            return original(*args, **kwargs)
        with mock.patch.object(self.store, "command", side_effect=incomplete):
            with self.assertRaises(subprocess.CalledProcessError):
                self.store.commit_sealed(capture)
        self.assertTrue(batch.exists())
        self.store.commit_sealed(capture)
        restored = self.root / "restored"
        restored.mkdir()
        self.store.command("extract", f"{self.store.repository}::live-{batch.name}", cwd=restored)
        self.assertEqual(replay(restored), (1, ["1"]))

    def test_rolling_budget_drops_old_units_without_breaking_shared_chunks(self):
        tree = self.root / "tree"
        tree.mkdir()
        shared = os.urandom(300_000)
        (tree / "shared").write_bytes(shared)
        (tree / "changed").write_bytes(os.urandom(300_000))
        self.store.commit("first", tree)
        (tree / "changed").write_bytes(os.urandom(300_000))
        self.store.commit("second", tree)
        size = product_archive.disk_bytes(self.store.repository)
        self.store.max_bytes = size - 100_000
        self.store.prune()
        self.assertEqual([a["name"] for a in self.store.archives()], ["second"])
        restored = self.root / "restored"
        restored.mkdir()
        self.store.command("extract", f"{self.store.repository}::second", cwd=restored)
        self.assertEqual((restored / "shared").read_bytes(), shared)
        self.assertEqual((restored / "changed").read_bytes(), (tree / "changed").read_bytes())


if __name__ == "__main__":
    unittest.main()
