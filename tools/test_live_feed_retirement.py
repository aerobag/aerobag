# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import live_feed_launch as launch
import live_feed_retirement as retirement
import release_reconciler as releases


class InstanceRetirementTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.env = self.root / "environments"
        self.env.mkdir()
        self.observed = releases.ObservedState()
        self.now = datetime(2026, 9, 13, tzinfo=timezone.utc)
        self.states = {}
        self.commands = []
        self.events = []
        self.pins = set()
        self.active = set()
        self.candidates = set()
        self.provider = self.add_instance("prod", "a")
        self.active.add(self.provider.instance_id)

    def add_instance(self, tag: str, digest: str, status: str = "running") -> releases.LiveFeedInstance:
        key = f"{tag}-{digest * 16}"
        launch_root = launch.instance_root(self.root, key)
        paths = [self.root / prefix / key for prefix in (
            "live-feeds/instances", "scratch/live-feeds/instances", "state/live-feeds/instances",
        )] + [launch_root]
        for path in paths:
            path.mkdir(parents=True)
            (path / "keep").write_text("instance state")
        manifest = launch_root / "packages/product_artifacts.json"
        manifest.parent.mkdir()
        manifest.write_text("{}")
        (manifest.parent / "current_artifacts.json").write_text("[]")
        (self.env / f"{key}.env").write_text("environment")
        instance = releases.LiveFeedInstance(
            instance_id=key, release_tag=tag, launch_digest=digest * 64,
            endpoint=f"http://127.0.0.1:{8100 + len(self.states)}",
            unit=f"aerobag-live-feeds-release@{key}.service", manifest=str(manifest),
            roots=[str(path) for path in paths], gc_paths=[launch.launch_gc_path(self.root, key)],
            status=status, evidence={"process_instance_id": key},
        )
        self.observed.live_feed_instances[key] = instance
        self.observed.releases.setdefault(tag, releases.ObservedRelease(
            tag=tag, tag_object="a" * 40, commit="b" * 40,
        ))
        self.states[instance.unit] = "active" if status == "running" else "inactive"
        self.pins.add(instance.gc_paths[0])
        self.persist()
        return instance

    def persist(self) -> None:
        releases.write_observed_state(self.root / "observed.json", self.observed)

    def run_service(self, command: list[str], **kwargs) -> subprocess.CompletedProcess:
        self.assertEqual(kwargs["timeout"], 30)
        self.assertTrue(kwargs["check"])
        self.commands.append(command)
        if command[:2] == ["systemctl", "show"]:
            return subprocess.CompletedProcess(command, 0, stdout=self.states[command[2]] + "\n")
        self.assertEqual(command[:3], ["systemctl", "disable", "--now"])
        self.states[command[-1]] = "inactive"
        self.events.append("disable")
        return subprocess.CompletedProcess(command, 0)

    def release_pins(self, ids: set[str]) -> None:
        self.events.append("release-pins")
        for key in ids:
            self.assertTrue(self.observed.gc_pending)
            self.pins.difference_update(self.observed.live_feed_instances[key].gc_paths)

    def maintain(self, at: datetime | None = None, **kwargs) -> set[str]:
        return retirement.retire_instances(
            self.root, self.observed, active_provider_ids=self.active,
            candidate_instance_ids=self.candidates, now=at or self.now,
            save=self.persist, release_gc_roots=self.release_pins, env_root=self.env,
            run=kwargs.pop("run", self.run_service), **kwargs,
        )

    def test_unused_candidate_stops_promptly_and_preserves_state_forever_for_opt_out(self) -> None:
        sunset = self.add_instance("sunset", "b")
        self.candidates.add(sunset.instance_id)
        self.maintain()
        deadline = sunset.draining_until_utc
        self.assertEqual(sunset.status, "stopped")
        self.assertEqual(self.states[sunset.unit], "inactive")
        self.assertIsNone(sunset.evidence)
        self.assertEqual(deadline, (self.now + releases.RELEASE_DRAIN_GRACE).isoformat().replace("+00:00", "Z"))
        self.assertEqual(self.commands, [
            ["systemctl", "show", sunset.unit, "--property=ActiveState", "--value"],
            ["systemctl", "disable", "--now", sunset.unit],
            ["systemctl", "show", sunset.unit, "--property=ActiveState", "--value"],
        ])
        self.maintain(self.now + timedelta(minutes=59))
        self.assertEqual(sunset.draining_until_utc, deadline)
        self.assertEqual(self.states[sunset.unit], "inactive")
        self.maintain(self.now + timedelta(hours=1))
        self.assertEqual(sunset.status, "stopped")
        self.assertEqual(sunset.draining_until_utc, deadline)
        self.assertTrue(all(Path(path).exists() for path in sunset.roots))
        self.assertTrue((self.env / f"{sunset.instance_id}.env").exists())
        self.assertTrue(set(sunset.gc_paths) <= self.pins)
        self.assertEqual(self.provider.status, "running")
        self.assertFalse(any(self.provider.unit in command for command in self.commands))
        stops = self.events.count("disable")
        self.maintain(self.now + timedelta(days=1))
        self.assertEqual(self.events.count("disable"), stops)
        self.assertEqual(sunset.draining_until_utc, deadline)
        self.assertTrue(all(Path(path).exists() for path in sunset.roots))
        self.assertTrue(set(sunset.gc_paths) <= self.pins)
        self.assertNotIn("release-pins", self.events)
        self.assertFalse(self.observed.gc_pending)

    def test_unavailable_instance_stops_before_existing_file_grace_without_extending_it(self) -> None:
        old = self.add_instance("sunset", "b", "unavailable")
        self.states[old.unit] = "active"
        deadline = (self.now + timedelta(minutes=37)).isoformat()
        old.draining_until_utc = deadline
        self.persist()
        self.assertEqual(self.maintain(), set())
        self.assertEqual(old.status, "stopped")
        self.assertEqual(self.states[old.unit], "inactive")
        self.assertEqual(old.draining_until_utc, deadline)
        self.assertTrue(all(Path(path).exists() for path in old.roots))
        self.assertTrue((self.env / f"{old.instance_id}.env").exists())
        self.assertTrue(set(old.gc_paths) <= self.pins)
        self.assertIsNone(old.evidence)
        self.assertEqual(self.maintain(self.now + timedelta(minutes=36)), set())
        self.assertEqual(old.draining_until_utc, deadline)
        self.assertNotIn("release-pins", self.events)
        self.assertEqual(self.maintain(self.now + timedelta(minutes=37)), {old.instance_id})
        self.assertEqual(self.events.count("disable"), 1)

    def test_stopped_instance_restarted_during_grace_is_stopped_without_sliding_deadline(self) -> None:
        old = self.add_instance("sunset", "b", "stopped")
        self.candidates.add(old.instance_id)
        old.draining_until_utc = (self.now + timedelta(minutes=20)).isoformat()
        deadline = old.draining_until_utc
        self.states[old.unit] = "active"
        self.assertEqual(self.maintain(), set())
        self.assertEqual(self.states[old.unit], "inactive")
        self.assertEqual(old.status, "stopped")
        self.assertEqual(old.draining_until_utc, deadline)
        self.assertTrue(set(old.gc_paths) <= self.pins)

    def test_same_tag_replacement_reclaims_only_old_instance_after_root_release(self) -> None:
        old = self.add_instance("prod", "b")
        clients = self.root / "release-builds/prod-build"
        clients.mkdir(parents=True)
        (clients / "web").write_text("client")
        self.observed.releases["prod"].release_root = str(clients)
        self.maintain()
        deadline = old.draining_until_utc
        self.assertEqual(old.status, "stopped")
        self.assertEqual(self.states[old.unit], "inactive")
        self.assertEqual(self.maintain(self.now + timedelta(minutes=59)), set())
        self.assertEqual(old.draining_until_utc, deadline)
        self.assertTrue(all(Path(path).exists() for path in old.roots))
        self.assertTrue(set(old.gc_paths) <= self.pins)
        self.assertNotIn("release-pins", self.events)
        remove = retirement.retirement.remove_owned_path

        def check_removal(root: Path, path: Path) -> None:
            self.assertFalse(set(old.gc_paths) & self.pins)
            self.assertEqual(old.status, "removed")
            self.events.append("remove")
            remove(root, path)

        with mock.patch.object(retirement.retirement, "remove_owned_path", side_effect=check_removal):
            removed = self.maintain(self.now + timedelta(hours=1))
        self.assertEqual(removed, {old.instance_id})
        self.assertLess(self.events.index("release-pins"), self.events.index("remove"))
        self.assertFalse(any(Path(path).exists() for path in old.roots))
        self.assertTrue((clients / "web").exists())
        self.assertTrue(all(Path(path).exists() for path in self.provider.roots))
        self.assertEqual(self.provider.status, "running")
        restored = releases.load_observed_state(self.root / "observed.json")
        self.assertEqual(restored.live_feed_instances[old.instance_id].status, "removed")

    def test_active_shared_provider_is_protected_even_with_expired_drain(self) -> None:
        self.provider.draining_until_utc = self.now.isoformat()
        self.maintain(self.now + timedelta(days=2))
        self.assertIsNone(self.provider.draining_until_utc)
        self.assertEqual(self.commands, [])
        self.assertEqual(self.provider.status, "running")
        self.assertTrue(set(self.provider.gc_paths) <= self.pins)

    def test_pending_obsolete_instance_preserves_files_and_pins_until_grace_expires(self) -> None:
        old = self.add_instance("prod", "b", "pending")
        self.observed.releases["prod"].live_feed_instance = old.instance_id
        self.assertEqual(self.maintain(), set())
        self.assertEqual(old.status, "stopped")
        self.assertIsNotNone(old.draining_until_utc)
        self.assertTrue(all(Path(path).exists() for path in old.roots))
        self.assertTrue(set(old.gc_paths) <= self.pins)
        self.assertEqual(self.maintain(self.now + timedelta(hours=1)), {old.instance_id})
        self.assertEqual(old.status, "removed")
        self.assertFalse(any(Path(path).exists() for path in old.roots))
        restored = releases.load_observed_state(self.root / "observed.json")
        self.assertEqual(restored.releases["prod"].live_feed_instance, old.instance_id)

    def test_obsolete_stopped_candidate_releases_resources_when_no_longer_needed(self) -> None:
        old = self.add_instance("sunset", "b", "stopped")
        self.candidates.add(old.instance_id)
        self.maintain()
        deadline = old.draining_until_utc
        self.assertTrue(set(old.gc_paths) <= self.pins)
        self.candidates.clear()
        self.assertEqual(self.maintain(self.now + timedelta(minutes=30)), set())
        self.assertEqual(old.draining_until_utc, deadline)
        self.assertEqual(self.maintain(self.now + timedelta(hours=1)), {old.instance_id})
        self.assertEqual(self.provider.status, "running")

    def test_stopped_candidate_becoming_obsolete_after_grace_is_removed_without_another_grace(self) -> None:
        old = self.add_instance("sunset", "b", "stopped")
        self.candidates.add(old.instance_id)
        old.draining_until_utc = self.now.isoformat()
        self.assertEqual(self.maintain(self.now + timedelta(days=1)), set())
        self.candidates.clear()
        self.assertEqual(self.maintain(self.now + timedelta(days=1)), {old.instance_id})

    def test_removed_candidate_does_not_block_repreparation_on_reintroduction(self) -> None:
        old = self.add_instance("prod", "b", "pending")
        old.draining_until_utc = self.now.isoformat()
        self.maintain()
        self.candidates.add(old.instance_id)
        calls = len(self.commands)
        self.assertEqual(self.maintain(), set())
        self.assertEqual(len(self.commands), calls)
        self.assertEqual(old.status, "removed")
        self.assertFalse(Path(old.manifest).exists())

    def test_removed_instance_cannot_be_protected_as_an_active_provider(self) -> None:
        old = self.add_instance("prod", "b", "pending")
        old.draining_until_utc = self.now.isoformat()
        self.maintain()
        self.active.add(old.instance_id)
        with self.assertRaisesRegex(releases.ReleaseConfigError, "removed.*still required"):
            self.maintain()

    def test_unrecorded_running_instance_gets_a_recoverable_non_sliding_drain(self) -> None:
        old = self.add_instance("prod", "b", "pending")
        self.states[old.unit] = "active"
        self.maintain()
        self.assertEqual(old.status, "stopped")
        self.assertEqual(self.states[old.unit], "inactive")
        self.assertIsNotNone(old.draining_until_utc)
        deadline = old.draining_until_utc
        self.observed = releases.load_observed_state(self.root / "observed.json")
        self.maintain(self.now + timedelta(minutes=30))
        self.assertEqual(self.observed.live_feed_instances[old.instance_id].draining_until_utc, deadline)
        self.assertEqual(self.events.count("disable"), 1)
        self.maintain(self.now + timedelta(hours=1))
        self.assertEqual(self.observed.live_feed_instances[old.instance_id].status, "removed")

    def test_stop_failure_keeps_files_and_gc_pins(self) -> None:
        old = self.add_instance("sunset", "b")
        old.draining_until_utc = (self.now + timedelta(hours=1)).isoformat()

        def fail(command: list[str], **kwargs):
            if command[:2] == ["systemctl", "disable"]:
                raise subprocess.TimeoutExpired(command, 30)
            return self.run_service(command, **kwargs)

        with self.assertRaises(subprocess.TimeoutExpired):
            self.maintain(run=fail)
        self.assertTrue(all(Path(path).exists() for path in old.roots))
        self.assertTrue(set(old.gc_paths) <= self.pins)
        self.assertEqual(old.status, "running")

    def test_inspection_failure_does_not_authorize_removal(self) -> None:
        old = self.add_instance("sunset", "b", "pending")
        with self.assertRaises(subprocess.CalledProcessError):
            self.maintain(run=mock.Mock(side_effect=subprocess.CalledProcessError(1, ["systemctl"])))
        self.assertTrue(all(Path(path).exists() for path in old.roots))
        self.assertTrue(set(old.gc_paths) <= self.pins)

    def test_failed_gc_root_write_never_deletes_instance_files(self) -> None:
        old = self.add_instance("prod", "b", "pending")
        old.draining_until_utc = self.now.isoformat()
        self.release_pins = mock.Mock(side_effect=OSError("root registry unavailable"))
        with self.assertRaisesRegex(OSError, "registry unavailable"):
            self.maintain()
        self.assertEqual(old.status, "stopped")
        self.assertTrue(all(Path(path).exists() for path in old.roots))
        self.assertTrue(set(old.gc_paths) <= self.pins)

    def test_partial_removal_retries_without_repinning_deleted_launch_inputs(self) -> None:
        old = self.add_instance("prod", "b", "pending")
        old.draining_until_utc = self.now.isoformat()
        remove = retirement.retirement.remove_owned_path

        def fail(root: Path, path: Path) -> None:
            if path == self.env / f"{old.instance_id}.env":
                raise OSError("interrupted")
            remove(root, path)

        with mock.patch.object(retirement.retirement, "remove_owned_path", side_effect=fail):
            with self.assertRaisesRegex(OSError, "interrupted"):
                self.maintain()
        self.observed = releases.load_observed_state(self.root / "observed.json")
        self.assertEqual(self.observed.live_feed_instances[old.instance_id].status, "removed")
        self.assertFalse(Path(old.manifest).exists())
        self.candidates.add(old.instance_id)
        self.maintain()
        self.assertFalse((self.env / f"{old.instance_id}.env").exists())
        self.assertFalse(set(old.gc_paths) & self.pins)
        calls = len(self.commands)
        self.maintain()
        self.assertEqual(len(self.commands), calls)

    def test_corrupt_instance_metadata_cannot_stop_or_delete_another_owner(self) -> None:
        old = self.add_instance("sunset", "b", "pending")
        for field, value in [
            ("instance_id", "../../prod"), ("unit", "aerobag-live-feeds.service"),
            ("roots", [str(self.root)]), ("manifest", str(self.root / "production.json")),
            ("gc_paths", self.provider.gc_paths), ("launch_digest", "invalid"),
        ]:
            with self.subTest(field=field):
                original = getattr(old, field)
                setattr(old, field, value)
                with self.assertRaises(releases.ReleaseConfigError):
                    self.maintain()
                self.assertEqual(self.commands, [])
                setattr(old, field, original)

    def test_symlinked_environment_cannot_be_followed_for_cleanup(self) -> None:
        old = self.add_instance("sunset", "b", "pending")
        environment = self.env / f"{old.instance_id}.env"
        environment.unlink()
        environment.symlink_to(self.root / "observed.json")
        with self.assertRaisesRegex(releases.ReleaseConfigError, "symlink"):
            self.maintain()
        self.assertEqual(self.commands, [])
        self.assertTrue((self.root / "observed.json").exists())

    def test_symlinked_instance_root_cannot_delete_shared_data(self) -> None:
        old = self.add_instance("sunset", "b", "pending")
        path = Path(old.roots[0])
        outside = self.root / "shared-data"
        path.rename(outside)
        path.symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(releases.ReleaseConfigError, "symlink"):
            self.maintain()
        self.assertTrue((outside / "keep").exists())
        self.assertEqual(self.commands, [])

    def test_successful_stop_command_must_actually_stop_the_instance(self) -> None:
        old = self.add_instance("sunset", "b")
        old.draining_until_utc = (self.now + timedelta(hours=1)).isoformat()

        def still_running(command: list[str], **kwargs):
            if command[:2] == ["systemctl", "disable"]:
                return subprocess.CompletedProcess(command, 0)
            return self.run_service(command, **kwargs)

        with self.assertRaisesRegex(RuntimeError, "did not stop"):
            self.maintain(run=still_running)
        self.assertTrue(all(Path(path).exists() for path in old.roots))
        self.assertTrue(set(old.gc_paths) <= self.pins)

    def test_unknown_protected_instance_fails_before_any_service_change(self) -> None:
        self.active.add("missing-provider")
        with self.assertRaisesRegex(releases.ReleaseConfigError, "missing protected"):
            self.maintain()
        self.assertEqual(self.commands, [])

    def test_removed_but_running_instance_does_not_lose_its_remaining_files(self) -> None:
        old = self.add_instance("sunset", "b", "removed")
        self.states[old.unit] = "active"
        with self.assertRaisesRegex(RuntimeError, "unexpectedly active"):
            self.maintain()
        self.assertTrue(all(Path(path).exists() for path in old.roots))
        self.assertEqual(old.status, "removed")


if __name__ == "__main__":
    unittest.main()
