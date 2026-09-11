#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import reconcile_prod_releases as controller
import release_reconciler as releases
import release_retirement as retirement


class RetirementTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.now = datetime(2026, 9, 11, tzinfo=timezone.utc)
        self.instance = controller.Controller.__new__(controller.Controller)
        self.instance.artifact_root = self.root
        self.instance.args = SimpleNamespace(
            observed=self.root / "state/releases-observed.json",
            controller_preprocessor=self.root / "controller/preprocessor-cli",
            force_production_tag=None,
        )
        self.instance.observed = releases.ObservedState()
        self.instance.desired = releases.DesiredReleases(releases.ReleaseBinding("prod"), None, ())
        self.states: dict[str, str] = {}
        self.env_root = self.root / "service-environments"
        self.env_root.mkdir()
        self.commands: list[list[str]] = []
        for tag in ("prod", "old", "older", "stage", "sunset"):
            record = releases.ObservedRelease(
                tag=tag, tag_object="a" * 40, commit="b" * 40,
                build_status="passed", qualification_status="passed",
                live_feed_status="stopped", live_feed_endpoint="http://127.0.0.1:8101",
                release_root=str(self.root / f"release-builds/{tag}-{'b' * 12}"),
            )
            self.instance.observed.releases[tag] = record
            for path in retirement.release_paths(self.root, record):
                if path.suffix == ".json":
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text("{}")
                else:
                    path.mkdir(parents=True, exist_ok=True)
                    (path / "owned").write_bytes(b"release-owned content")
            self.states[tag] = "inactive"
            (self.env_root / f"{tag}.env").write_text("release environment")
        self.instance.save()
        self.sequence = 0
        self.mock_run = mock.patch.object(controller, "_run", side_effect=self.run_command).start()
        self.mock_show = mock.patch.object(controller.subprocess, "run", side_effect=self.show_service).start()
        self.addCleanup(mock.patch.stopall)

    def run_command(self, command: list[str], **kwargs) -> None:
        self.commands.append(command)
        if command[:3] == ["systemctl", "disable", "--now"]:
            tag = command[-1].split("@", 1)[1].removesuffix(".service")
            self.states[tag] = "inactive"

    def show_service(self, command: list[str], **kwargs) -> subprocess.CompletedProcess:
        self.assertEqual(command[:2], ["systemctl", "show"])
        self.assertEqual(kwargs["timeout"], 30)
        tag = command[2].split("@", 1)[1].removesuffix(".service")
        return subprocess.CompletedProcess(command, 0, stdout=self.states[tag] + "\n")

    def generation(self, tag: str, *, sunset: tuple[str, ...] = (), stage: str | None = None) -> Path:
        self.sequence += 1
        generation = self.root / f"channel-generations/{self.sequence:08d}"

        def manifest(release_tag: str) -> releases.ChannelManifest:
            return releases.ChannelManifest(
                release_tag=release_tag, source_path=self.root / "unused.json",
                document={"contracts": {"nav-db": "NAV25"}}, publication_roots=(),
            )

        releases.materialize_channel_generation(
            generation, self.root / "published",
            production_manifests=[manifest(name) for name in (tag, *sunset)],
            staging_manifests=[] if stage is None else [manifest(stage)],
        )
        (generation / "generation.json").write_text(json.dumps({
            "generation": self.sequence, "production": tag, "staging": stage, "sunset": list(sunset),
        }))
        return generation

    def activate(self, tag: str, *, at: datetime | None = None) -> Path:
        generation = self.generation(tag)
        releases.activate_channel_generation(self.root, generation, now=at or self.now)
        self.instance.observed.production = tag
        self.instance.observed.generation = self.sequence
        self.instance.desired = releases.DesiredReleases(releases.ReleaseBinding(tag), None, ())
        return generation

    def registry(self) -> list[str]:
        return json.loads((self.root / releases.RELEASE_GC_ROOTS).read_text())["current_artifacts_paths"]

    def maintain(self, at: datetime) -> None:
        self.instance.maintain_retirement(now=at, env_root=self.env_root)

    def test_deadline_reclaims_old_roots_files_and_port_without_another_activation(self) -> None:
        old = self.activate("old")
        current = self.activate("prod", at=self.now + timedelta(minutes=5))
        record = self.instance.observed.releases["old"]
        record.live_feed_status = "running"
        record.draining_until_utc = (self.now + timedelta(minutes=65)).isoformat()
        self.states["old"] = "active"
        paths = retirement.release_paths(self.root, record)
        self.maintain(self.now + timedelta(minutes=64))
        self.assertEqual(self.states["old"], "active")
        self.assertTrue(all(path.exists() for path in paths))
        self.assertTrue(any(old.name in path for path in self.registry()))
        self.maintain(self.now + timedelta(minutes=65))
        self.assertEqual(self.states["old"], "inactive")
        self.assertFalse(any(path.exists() for path in paths))
        self.assertFalse((self.env_root / "old.env").exists())
        self.assertTrue((self.env_root / "prod.env").exists())
        self.assertFalse(old.exists())
        self.assertTrue(current.exists())
        self.assertTrue(all(current.name in path for path in self.registry()))
        self.assertIsNone(record.live_feed_endpoint)
        self.assertIsNone(record.release_root)
        self.assertIsNone(record.product_manifest)
        self.assertEqual(record.build_status, "pending")
        self.assertEqual(record.commit, "b" * 40)
        self.assertTrue(self.instance.observed.gc_pending)
        self.instance.run_pending_gc()
        self.assertFalse(self.instance.observed.gc_pending)
        self.assertEqual(self.commands[-1][1:], ["gc", "--build-root", str(self.root), "--execute"])
        calls = len(self.commands)
        self.maintain(self.now + timedelta(days=5))
        self.assertEqual(len(self.commands), calls)
        self.instance.desired = releases.DesiredReleases(releases.ReleaseBinding("old"), None, ())
        plan = releases.plan_reconciliation(self.instance.desired, self.instance.observed)
        self.assertEqual(plan.actions[0], releases.ReconcileAction("build_release", "old"))

    def test_all_rapidly_replaced_generations_keep_their_own_non_sliding_grace(self) -> None:
        first = self.activate("older")
        second = self.activate("old", at=self.now + timedelta(minutes=10))
        third = self.activate("prod", at=self.now + timedelta(minutes=20))
        self.assertEqual({Path(path).parts[1] for path in self.registry()}, {first.name, second.name, third.name})
        self.maintain(self.now + timedelta(minutes=70))
        self.assertFalse(first.exists())
        self.assertTrue(second.exists())
        self.assertTrue(third.exists())
        self.maintain(self.now + timedelta(minutes=80))
        self.assertFalse(second.exists())
        self.assertTrue(third.exists())

    def test_legacy_roots_get_one_grace_and_unrooted_historical_generations_are_swept(self) -> None:
        abandoned = self.generation("older")
        previous = self.activate("old")
        current = self.activate("prod")
        (previous / releases.GENERATION_RETIREMENT_FILE).unlink()
        (current / releases.GENERATION_RETIREMENT_FILE).unlink()
        self.maintain(self.now)
        self.assertFalse(abandoned.exists())
        marker = previous / releases.GENERATION_RETIREMENT_FILE
        before = marker.read_bytes()
        self.maintain(self.now + timedelta(minutes=59))
        self.assertEqual(marker.read_bytes(), before)
        self.maintain(self.now + timedelta(hours=1))
        self.assertFalse(previous.exists())
        self.assertTrue(current.exists())

    def test_protects_desired_staging_sunset_and_actual_serving_generation(self) -> None:
        generation = self.generation("prod", sunset=("sunset",), stage="stage")
        releases.activate_channel_generation(self.root, generation, now=self.now)
        self.instance.observed.production = "prod"
        # Even a desired-state edit does not authorize deletion of still-served files.
        self.instance.desired = releases.DesiredReleases(releases.ReleaseBinding("old"), None, ())
        self.maintain(self.now + timedelta(days=20))
        for tag in ("prod", "old", "sunset", "stage"):
            self.assertTrue(all(path.exists() for path in retirement.release_paths(
                self.root, self.instance.observed.releases[tag],
            )))
        self.assertFalse(any("@stage." in " ".join(command) for command in self.commands))

    def test_historical_stopped_releases_are_removed_without_harming_shared_inodes(self) -> None:
        self.activate("prod")
        shared = self.root / "shared-blob"
        shared.write_bytes(b"shared data")
        old = Path(self.instance.observed.releases["old"].release_root)
        os.link(shared, old / "hardlink")
        (old / "symlink").symlink_to(shared)
        self.maintain(self.now)
        self.assertFalse(old.exists())
        self.assertEqual(shared.read_bytes(), b"shared data")
        self.assertEqual(shared.stat().st_nlink, 1)

    def test_actual_running_service_is_not_deleted_even_when_observed_as_stopped(self) -> None:
        self.activate("prod")
        self.states["old"] = "active"
        old = Path(self.instance.observed.releases["old"].release_root)
        self.maintain(self.now)
        record = self.instance.observed.releases["old"]
        first_deadline = record.draining_until_utc
        self.assertTrue(old.exists())
        self.assertIsNotNone(first_deadline)
        self.maintain(self.now + timedelta(minutes=59))
        self.assertEqual(record.draining_until_utc, first_deadline)
        self.maintain(self.now + timedelta(hours=1))
        self.assertFalse(old.exists())

    def test_recovered_service_drain_keeps_its_old_publication_rooted(self) -> None:
        previous = self.activate("old")
        self.activate("prod")
        self.states["old"] = "active"
        self.maintain(self.now + timedelta(hours=2))
        self.assertTrue(previous.exists())
        self.assertTrue(any(previous.name in path for path in self.registry()))
        self.maintain(self.now + timedelta(hours=3))
        self.assertFalse(previous.exists())
        self.assertEqual(self.states["old"], "inactive")

    def test_explicit_service_deadline_outlives_generation_lease(self) -> None:
        previous = self.activate("old")
        self.activate("prod")
        record = self.instance.observed.releases["old"]
        record.draining_until_utc = (self.now + timedelta(hours=3)).isoformat()
        self.states["old"] = "active"
        self.maintain(self.now + timedelta(hours=2))
        self.assertTrue(previous.exists())
        self.assertEqual(self.states["old"], "active")
        self.maintain(self.now + timedelta(hours=3))
        self.assertFalse(previous.exists())
        self.assertEqual(self.states["old"], "inactive")

    def test_failed_service_stop_keeps_files_and_gc_roots(self) -> None:
        previous = self.activate("old")
        self.activate("prod")
        record = self.instance.observed.releases["old"]
        record.draining_until_utc = self.now.isoformat()
        self.states["old"] = "active"
        roots = self.registry()
        self.mock_run.side_effect = RuntimeError("systemd failed")
        with self.assertRaisesRegex(RuntimeError, "systemd failed"):
            self.maintain(self.now + timedelta(hours=2))
        self.assertTrue(previous.exists())
        self.assertTrue(Path(record.release_root).exists())
        self.assertEqual(self.registry(), roots)

    def test_service_inspection_failure_does_not_authorize_cleanup(self) -> None:
        self.activate("prod")
        self.mock_show.side_effect = subprocess.CalledProcessError(1, ["systemctl"])
        with self.assertRaises(subprocess.CalledProcessError):
            self.maintain(self.now)
        self.assertTrue(Path(self.instance.observed.releases["old"].release_root).exists())

    def test_partial_removal_is_retried_after_restart(self) -> None:
        self.activate("prod")
        record = self.instance.observed.releases["old"]
        original = retirement.remove_owned_path

        def fail_once(root: Path, path: Path) -> None:
            if path == self.root / "state/live-feeds/releases/old":
                raise OSError("interrupted removal")
            original(root, path)

        with mock.patch.object(retirement, "remove_owned_path", side_effect=fail_once):
            with self.assertRaisesRegex(OSError, "interrupted removal"):
                self.maintain(self.now)
        self.instance.observed = releases.load_observed_state(self.instance.args.observed)
        self.assertTrue(self.instance.observed.gc_pending)
        self.maintain(self.now)
        self.assertIsNone(self.instance.observed.releases["old"].release_root)
        self.assertFalse(any(path.exists() for path in retirement.release_paths(self.root, record)))

    def test_unsafe_observed_release_root_cannot_delete_external_files(self) -> None:
        self.activate("prod")
        outside = self.root / "not-release-owned"
        outside.mkdir()
        (outside / "keep").write_text("keep")
        self.instance.observed.releases["old"].release_root = str(outside)
        with self.assertRaisesRegex(releases.ReleaseConfigError, "unexpected release_root"):
            self.maintain(self.now)
        self.assertTrue((outside / "keep").exists())

    def test_symlinked_cleanup_namespace_is_refused(self) -> None:
        self.activate("prod")
        original = self.root / "live-feeds/releases"
        moved = self.root / "do-not-follow"
        original.rename(moved)
        original.symlink_to(moved, target_is_directory=True)
        with self.assertRaisesRegex(releases.ReleaseConfigError, "symlink"):
            self.maintain(self.now)
        self.assertTrue((moved / "old/owned").exists())

    def test_missing_active_manifest_fails_closed(self) -> None:
        current = self.activate("prod")
        (current / "production/packages/current_artifacts.json").unlink()
        with self.assertRaisesRegex(releases.ReleaseConfigError, "missing"):
            self.maintain(self.now)
        self.assertTrue(Path(self.instance.observed.releases["old"].release_root).exists())

    def test_without_active_generation_nothing_is_deleted(self) -> None:
        self.maintain(self.now)
        self.assertEqual(self.commands, [])
        self.mock_show.assert_not_called()

    def test_legacy_package_directory_symlink_is_read_but_not_deleted(self) -> None:
        published = self.root / "published"
        published.mkdir()
        (published / "current_artifacts.json").write_text("[]")
        (published / "keep").write_text("shared data")
        legacy = self.root / "channel-generations/legacy-bootstrap"
        (legacy / "production").mkdir(parents=True)
        (legacy / "production/packages").symlink_to(published, target_is_directory=True)
        (legacy / "gc-root-manifests.json").write_text(json.dumps({
            "schema_version": 1,
            "current_artifacts_paths": ["production/packages/current_artifacts.json"],
        }))
        releases.activate_channel_generation(self.root, legacy, now=self.now)
        current = self.activate("prod")
        self.maintain(self.now + timedelta(hours=2))
        self.assertFalse(legacy.exists())
        self.assertTrue(current.exists())
        self.assertEqual((published / "keep").read_text(), "shared data")

    def test_main_finishes_retirement_and_gc_before_a_failing_product_refresh(self) -> None:
        previous = self.activate("old")
        self.activate("prod")
        self.instance.observed.releases["prod"].live_feed_status = "running"
        args = SimpleNamespace(
            artifact_root=self.root, check_deployments_only=False,
            plan=False, refresh_products=True, force_production_tag=None,
        )
        maintain = self.instance.maintain_retirement
        with (
            mock.patch.object(controller, "parse_args", return_value=args),
            mock.patch.object(controller, "Controller", return_value=self.instance),
            mock.patch.object(self.instance, "maintain_retirement", side_effect=lambda: maintain(
                now=self.now + timedelta(hours=2), env_root=self.env_root,
            )),
            mock.patch.object(self.instance, "refresh_products", side_effect=RuntimeError("refresh failed")),
        ):
            with self.assertRaisesRegex(RuntimeError, "refresh failed"):
                controller.main()
        self.assertFalse(previous.exists())
        self.assertIsNone(self.instance.observed.releases["old"].release_root)
        self.assertFalse(self.instance.observed.gc_pending)

    def test_plan_only_entrypoint_does_not_migrate_retention_or_remove_files(self) -> None:
        previous = self.activate("old")
        self.activate("prod")
        marker = previous / releases.GENERATION_RETIREMENT_FILE
        marker.unlink()
        args = SimpleNamespace(
            artifact_root=self.root, check_deployments_only=False,
            plan=True, refresh_products=True, force_production_tag=None,
        )
        with (
            mock.patch.object(controller, "parse_args", return_value=args),
            mock.patch.object(controller, "Controller", return_value=self.instance),
        ):
            self.assertEqual(controller.main(), 0)
        self.assertFalse(marker.exists())
        self.assertTrue(previous.exists())
        self.mock_show.assert_not_called()

    def test_failed_pointer_switch_protects_both_generations_then_expires_candidate(self) -> None:
        original = self.activate("prod")
        candidate = self.generation("old")
        replace = os.replace

        def fail_pointer(source, target):
            if Path(target).name == "channel-current":
                raise OSError("pointer switch failed")
            return replace(source, target)

        with mock.patch.object(releases.os, "replace", side_effect=fail_pointer):
            with self.assertRaisesRegex(OSError, "pointer switch failed"):
                releases.activate_channel_generation(self.root, candidate, now=self.now)
        self.assertEqual(releases.current_generation(self.root), original)
        self.maintain(self.now + timedelta(minutes=59))
        self.assertTrue(candidate.exists())
        self.maintain(self.now + timedelta(hours=1))
        self.assertFalse(candidate.exists())
        self.assertTrue(original.exists())


if __name__ == "__main__":
    unittest.main()
