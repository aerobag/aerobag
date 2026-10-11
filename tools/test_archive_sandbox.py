# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Real systemd/Borg tests, confined to fresh fixtures and uniquely named units."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import uuid
from unittest import mock

import prod_deployment
from product_archive import BorgStore, DailyCapture, atomic_json
from test_product_archive import Source, replay


ROOT = Path(__file__).resolve().parents[1]


class ArchiveSandboxTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        missing = []
        if os.geteuid() != 0:
            missing.append("root")
        for command in ("borg", "systemctl", "journalctl", "mount", "umount"):
            if not shutil.which(command):
                missing.append(command)
        if not Path("/run/systemd/system").is_dir():
            missing.append("running systemd")
        if missing:
            message = "archive sandbox tests require " + ", ".join(missing)
            if os.environ.get("AEROBAG_REQUIRE_ARCHIVE_SANDBOX_TEST") == "1":
                raise RuntimeError(message)
            raise unittest.SkipTest(message)

    def setUp(self):
        # Outside /tmp and /var/tmp: PrivateTmp deliberately hides their host view.
        self.temp = tempfile.TemporaryDirectory(prefix="aerobag-archive-test-", dir="/run")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.artifacts = self.root / "artifacts"
        self.archive = self.root / "archive"
        self.data = self.root / "data"
        for path in (self.archive, self.artifacts / "state", self.artifacts / "locks", self.data / "health"):
            path.mkdir(parents=True)
        # A separate filesystem like production. Size is a ceiling, not a
        # reservation; these tiny fixtures allocate only a few MiB.
        subprocess.run(["mount", "-t", "tmpfs", "-o", "size=2G,nodev,nosuid", "tmpfs", str(self.archive)],
                       check=True, timeout=10)
        self.addCleanup(subprocess.run, ["umount", str(self.archive)], check=True, timeout=10)
        self.config = {
            "archive_root": str(self.archive), "artifact_root": str(self.artifacts),
            "data_root": str(self.data), "source_root": str(ROOT),
            "archive_total_gib": 10, "archive_cycles_gib": 7,
            "archive_buffer_gib": 1, "archive_reserve_gib": 1,
        }
        self.config_path = self.root / "config.json"
        atomic_json(self.config_path, self.config)
        generation = self.artifacts / "channel-generations/1"
        atomic_json(generation / "generation.json", {"production": "prod", "sunset": []})
        atomic_json(generation / "live-feed-bindings.json", {"releases": {"prod": {"provider": "provider"}}})
        (self.artifacts / "channel-current").symlink_to(generation)
        self.publication = self.artifacts / "published/cycle/123"
        self.page = self.publication / "unpacked/nav-db/page"
        self.page.parent.mkdir(parents=True)
        self.page.write_bytes(b"immutable compressed cycle page")
        atomic_json(generation / "production/packages/current_artifacts.json", [
            {"artifact_roots": {"unpacked": "cycle/123/unpacked"}},
        ])
        self.environment = {
            "BORG_CACHE_DIR": str(self.archive / "borg-cache"),
            "BORG_SECURITY_DIR": str(self.archive / "borg-security"),
            "BORG_PASSPHRASE": "",
        }
        environment = mock.patch.dict(os.environ, self.environment)
        environment.start()
        self.addCleanup(environment.stop)

    def run_unit(self, mode, probe=None):
        name = "aerobag-archive-test-" + uuid.uuid4().hex + ".service"
        unit_path = Path("/run/systemd/system") / name
        unit = prod_deployment.archive_unit(self.config, mode)
        original = f"ExecStart=/usr/bin/python3 {ROOT}/tools/archive_service.py --mode {mode}"
        self.assertIn(original, unit)
        command = original + f" --config {self.config_path}"
        if probe is not None:
            script = self.root / "probe.py"
            script.write_text(
                "import errno, json, os, shutil, sys, tempfile\nfrom pathlib import Path\n"
                f"sys.path.insert(0, {str(ROOT / 'tools')!r})\n"
                "import archive_service\n"
                f"config = json.loads(Path({str(self.config_path)!r}).read_text())\n"
                f"root = Path({str(self.root)!r})\n" + probe,
            )
            command = f"ExecStart=/usr/bin/python3 {script}"
        # Keep every sandbox directive from the generated unit. Only replace
        # the fixture invocation and bound the test's service lifetime.
        unit = unit.replace(original, command).replace("Restart=on-failure", "Restart=no")
        unit = unit.replace("TimeoutStartSec=4h", "TimeoutStartSec=45s")
        unit = unit.replace("[Service]\n", "[Service]\nTimeoutStopSec=10s\n")
        if mode == "collect":
            unit = unit.replace("[Service]\n", "[Service]\nRuntimeMaxSec=45s\n")
        with unit_path.open("x") as stream:
            stream.write(unit)
        try:
            subprocess.run(["systemctl", "daemon-reload"], check=True, timeout=20)
            try:
                result = subprocess.run(["systemctl", "start", "--wait", name],
                                        text=True, capture_output=True, timeout=60)
            except subprocess.TimeoutExpired:
                result = None
            journal = subprocess.run(["journalctl", "-u", name, "--no-pager"],
                                     text=True, capture_output=True, check=True, timeout=10)
            self.assertIsNotNone(result, "service watchdog expired\n" + journal.stdout + "\n" + unit)
            self.assertEqual(result.returncode, 0, result.stderr + "\n" + journal.stdout + "\n" + unit)
        finally:
            subprocess.run(["systemctl", "stop", name], check=True, timeout=15)
            subprocess.run(["systemctl", "reset-failed", name], capture_output=True, timeout=10)
            unit_path.unlink()
            subprocess.run(["systemctl", "daemon-reload"], check=True, timeout=20)

    def test_store_commits_restores_and_skips_unchanged_publications(self):
        capture = DailyCapture(self.archive / "live-buffer", max_bytes=10_000_000)
        batch = capture.collect(Source(), "2026-10-09T12:00:00Z")
        capture.retire(set(), "2026-10-09T12:01:00Z")
        pending = self.artifacts / "state/archive-cycle-pending"
        pending.mkdir()
        (pending / "unfinished").write_text("left behind by the failed production pin")

        self.run_unit("store")
        status = json.loads((self.data / "health/archive/store.json").read_text())
        self.assertIsNone(status["error"])
        self.assertIsNotNone(status["last_success_at_utc"])
        self.assertFalse(batch.exists())
        self.assertFalse(pending.exists())
        self.assertEqual(self.page.read_bytes(), b"immutable compressed cycle page")
        counts = {}
        for bucket in ("live-feeds", "cycles"):
            store = BorgStore(self.archive / f"{bucket}.borg", max_bytes=10**9, reserve_bytes=0)
            names = [row["name"] for row in store.archives()]
            self.assertEqual(len(names), 1)
            counts[bucket] = names
            restored = self.root / f"restored-{bucket}"
            restored.mkdir()
            store.command("extract", f"{store.repository}::{names[0]}", cwd=restored)
            if bucket == "live-feeds":
                self.assertEqual(replay(restored), (1, ["1"]))
            else:
                self.assertEqual((restored / "published/cycle/123/unpacked/nav-db/page").read_bytes(),
                                 self.page.read_bytes())
        self.run_unit("store")
        for bucket, names in counts.items():
            store = BorgStore(self.archive / f"{bucket}.borg", max_bytes=10**9, reserve_bytes=0)
            self.assertEqual([row["name"] for row in store.archives()], names)

    def test_store_hardlink_pin_survives_publication_gc(self):
        self.run_unit("store", """
pending = archive_service.pin_cycle(config)
publication = root / 'artifacts/published/cycle/123'
source = publication / 'unpacked/nav-db/page'
copy = pending / 'published/cycle/123/unpacked/nav-db/page'
assert source.stat().st_ino == copy.stat().st_ino
assert source.stat().st_dev == copy.stat().st_dev
shutil.rmtree(publication)  # Only this test's synthetic publication.
assert copy.read_bytes() == b'immutable compressed cycle page'
try:
    (root / 'outside-allowed-paths').write_text('forbidden')
except OSError as error:
    assert error.errno == errno.EROFS, error
else:
    raise AssertionError('store lost ProtectSystem=strict')
"""
        )

    def test_collector_has_private_tmp_and_read_only_artifacts(self):
        self.run_unit("collect", """
with tempfile.TemporaryDirectory() as temp:
    (Path(temp) / 'borg-style-scratch').write_text('writable')
for relative in ('artifacts/published/cycle/123/unpacked/nav-db/page',
                 'artifacts/state/forbidden', 'artifacts/locks/forbidden',
                 'outside-allowed-paths'):
    try:
        (root / relative).write_text('forbidden')
    except OSError as error:
        assert error.errno == errno.EROFS, error
    else:
        raise AssertionError('collector could write ' + relative)
(root / 'archive/allowed').write_text('buffer')
(root / 'data/health/allowed').write_text('status')
"""
        )


if __name__ == "__main__":
    unittest.main()
