# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest import mock

import archive_service
import prod_deployment
from product_archive import atomic_json, locked


class ArchiveServiceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.artifacts = self.root / "artifacts"
        self.archive = self.root / "archive"
        self.archive.mkdir()
        self.config = {
            "archive_enabled": True, "archive_root": str(self.archive), "artifact_root": str(self.artifacts),
            "source_root": str(Path(__file__).resolve().parents[1]), "data_root": str(self.root / "data"),
            "archive_total_gib": 10, "archive_cycles_gib": 7, "archive_buffer_gib": 1, "archive_reserve_gib": 1,
            "archive_notam_helper": "/usr/local/bin/aerobag-archive-notam",
        }
        self.generation = self.artifacts / "channel-generations/1"
        self.generation.mkdir(parents=True)
        (self.artifacts / "channel-current").symlink_to(self.generation)
        atomic_json(self.generation / "generation.json", {"production": "prod", "sunset": ["old"], "staging": "test"})
        atomic_json(self.generation / "live-feed-bindings.json", {"releases": {
            "prod": {"provider": "prod-instance"}, "old": {"provider": "prod-instance"},
            "test": {"provider": "test-instance"},
        }})
        self.publication = self.artifacts / "published/cycle/123"
        self.file = self.publication / "unpacked/nav-db/page"
        self.file.parent.mkdir(parents=True)
        self.file.write_bytes(b"compressed cycle page")
        (self.publication / "packaged").mkdir()
        atomic_json(self.generation / "production/packages/current_artifacts.json", [
            {"artifact_roots": {"unpacked": "cycle/123/unpacked", "packaged": "cycle/123/packaged"}},
        ])

    def test_shared_provider_is_captured_once_and_staging_is_excluded(self):
        generation, providers = archive_service.production_sources(self.artifacts)
        self.assertEqual(generation, self.generation)
        self.assertEqual(providers, ["prod-instance"])

    def test_cycle_pin_survives_publication_gc_and_skips_unchanged_cycle(self):
        pending = archive_service.pin_cycle(self.config)
        copy = pending / "published/cycle/123/unpacked/nav-db/page"
        self.assertEqual(copy.stat().st_ino, self.file.stat().st_ino)
        shutil.rmtree(self.publication)
        self.assertEqual(copy.read_bytes(), b"compressed cycle page")
        self.assertEqual(archive_service.pin_cycle(self.config), pending)
        metadata = json.loads((pending / "ready.json").read_text())
        atomic_json(self.artifacts / "state/archive-cycle-last.json", metadata)
        shutil.rmtree(pending)
        self.assertIsNone(archive_service.pin_cycle(self.config))

    def test_incomplete_cycle_pin_is_rebuilt_and_controller_lock_is_respected(self):
        pending = self.artifacts / "state/archive-cycle-pending"
        pending.mkdir(parents=True)
        (pending / "unfinished").write_text("interrupted")
        with locked(self.artifacts / "locks/release-reconciler.lock"):
            with self.assertRaises(BlockingIOError):
                archive_service.pin_cycle(self.config)
        archive_service.pin_cycle(self.config)
        self.assertFalse((pending / "unfinished").exists())
        self.assertTrue((pending / "ready.json").exists())

    def test_missing_mount_and_overlapping_roots_are_rejected(self):
        with self.assertRaisesRegex(OSError, "not mounted"):
            archive_service.require_mount(self.config)
        for root in (self.artifacts, self.artifacts / "archive", self.root):
            with self.assertRaisesRegex(ValueError, "disjoint"):
                archive_service.validate_config({**self.config, "archive_root": str(root)})

    def test_failed_borg_keeps_pinned_cycle_and_other_bucket_still_runs(self):
        with mock.patch.object(archive_service, "BorgStore") as borg:
            borg.return_value.commit_sealed.side_effect = RuntimeError("live repo unavailable")
            borg.return_value.commit.side_effect = RuntimeError("cycle repo unavailable")
            with self.assertRaisesRegex(RuntimeError, "live.*cycle"):
                archive_service.store_once(self.config, self.archive)
            borg.return_value.commit.assert_called_once()
        self.assertTrue((self.artifacts / "state/archive-cycle-pending/ready.json").exists())
        self.assertFalse((self.artifacts / "state/archive-cycle-last.json").exists())

    def test_status_has_valid_contract_and_latched_gaps(self):
        atomic_json(self.archive / "live-buffer/gaps/one.json", {"reason": "expired"})
        archive_service.status(self.config, "collect", RuntimeError("offline"))
        path = Path(self.config["data_root"]) / "health/archive/collect.json"
        first = json.loads(path.read_text())
        self.assertEqual(first["failure_count"], 1)
        self.assertEqual(first["capture_gap_count"], 1)
        self.assertEqual(first["telemetry_contract"], archive_service.telemetry_pin(self.config))
        archive_service.status(self.config, "collect", None)
        recovered = json.loads(path.read_text())
        self.assertEqual(recovered["failure_count"], 0)
        self.assertEqual(recovered["capture_gap_count"], 1)
        self.assertIsNotNone(recovered["last_success_at_utc"])

    def test_deployment_generates_independent_workers_and_requires_mount(self):
        for mode in ("collect", "store"):
            unit = prod_deployment.archive_unit(self.config, mode)
            self.assertIn(f"RequiresMountsFor={self.archive}", unit)
            self.assertIn(f"--mode {mode}", unit)
            self.assertIn("IOSchedulingClass=idle", unit)
            self.assertIn("ProtectSystem=strict", unit)
        with mock.patch.object(prod_deployment, "run_ssh") as ssh:
            prod_deployment.install_archive_tools(self.config, dry_run=True)
        command = ssh.call_args.args[1]
        self.assertIn("export CARGO_TARGET_DIR", command)
        self.assertIn("--locked --release", command)
        self.assertIn("-p notam-state --bin aerobag-archive-notam", command)


if __name__ == "__main__":
    unittest.main()
