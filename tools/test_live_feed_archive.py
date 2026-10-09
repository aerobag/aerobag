# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

import hashlib
import json
import lzma
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import zipfile

from live_feed_archive import LiveSource, copy_payload, member, verify_directory
from product_archive import DailyCapture, atomic_json, read_events


class PublishedFixture:
    def __init__(self, root, product="metars"):
        self.root, self.product = root, product
        self.current = {"schema_version": 3, "products": {}}

    def payload(self, relative, value):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        raw = lzma.compress(json.dumps(value).encode())
        path.write_bytes(raw)
        return {"url": relative, "bytes": len(raw), "blob_sha256": hashlib.sha256(raw).hexdigest(),
                "kind": "json_xz", "state_sha256": str(value)}

    def publish(self, version, *, parent=None, timestamp=None):
        state = self.payload(f"states/{self.product}/{version}.json.xz", {"version": version})
        delta = None
        if parent:
            delta = self.payload(f"deltas/{self.product}/{parent}-{version}.json.xz", {"from": parent, "to": version})
            delta.update(from_version=parent, to_version=version)
        value = {"schema_version": 3, "product": self.product, "version": version,
                 "previous": parent, "state": state, "delta_from_previous": delta}
        relative = f"versions/{self.product}/{version}.json"
        atomic_json(self.root / relative, value)
        if timestamp is not None:
            os.utime(self.root / relative, ns=(timestamp, timestamp))
        self.current["products"][self.product] = {"current": version, "version_manifest_url": relative}
        atomic_json(self.root / "current.json", self.current)
        return value


class LiveFeedArchiveTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.publication = self.root / "publication"
        self.fixture = PublishedFixture(self.publication)
        self.source = LiveSource(self.publication, "test/v3", "notam-test-helper")
        self.capture = DailyCapture(self.root / "buffer", max_bytes=20_000_000)

    def test_coalesced_current_captures_every_delta_without_decoding(self):
        self.fixture.publish("v1")
        batch = self.capture.collect(self.source, "2026-10-09T12:00:00Z")
        self.fixture.publish("v2", parent="v1")
        self.fixture.publish("v3", parent="v2")
        with mock.patch("live_feed_archive.lzma.decompress", side_effect=AssertionError("must copy cooked bytes")):
            self.capture.collect(self.source, "2026-10-09T12:00:01Z")
        events = read_events(batch)
        self.assertEqual([e["head"]["version"] for e in events], ["v1", "v2", "v3"])
        for event in events:
            self.assertEqual(len(event["payloads"]), 1)
            ref = event["payloads"][0]
            self.assertEqual((batch / event["directory"] / ref["url"]).read_bytes(),
                             (self.publication / ref["url"]).read_bytes())
        self.assertEqual([e["kind"] for e in events], ["baseline", "delta", "delta"])

    def test_full_frames_with_equal_timestamps_are_not_lost_or_repeated(self):
        self.fixture.product = "nexrad"
        self.fixture.publish("v1", timestamp=1_000_000_000)
        batch = self.capture.collect(self.source, "2026-10-09T12:00:00Z")
        self.fixture.publish("v2", timestamp=1_000_000_000)
        self.fixture.publish("v3", timestamp=1_000_000_000)
        self.capture.collect(self.source, "2026-10-09T12:00:01Z")
        self.fixture.publish("v4", timestamp=1_000_000_000)
        self.capture.collect(self.source, "2026-10-09T12:00:02Z")
        self.capture.collect(self.source, "2026-10-09T12:00:03Z")
        self.assertEqual([e["head"]["version"] for e in read_events(batch)], ["v1", "v2", "v3", "v4"])

    def test_expired_chain_resumes_full_state_and_latches_gap(self):
        self.fixture.publish("v1")
        batch = self.capture.collect(self.source, "2026-10-09T12:00:00Z")
        self.fixture.publish("v3", parent="v2")
        self.capture.collect(self.source, "2026-10-09T12:00:01Z")
        last = read_events(batch)[-1]
        self.assertEqual(last["kind"], "baseline")
        self.assertIn("expired delta chain", last["gap"])
        self.assertEqual(last["head"]["version"], "v3")
        self.assertEqual(len(list((self.capture.root / "gaps").glob("*.json"))), 1)

    def test_payload_hash_failure_does_not_advance_cursor(self):
        self.fixture.publish("v1")
        batch = self.capture.collect(self.source, "2026-10-09T12:00:00Z")
        next_version = self.fixture.publish("v2", parent="v1")
        path = self.publication / next_version["delta_from_previous"]["url"]
        original = path.read_bytes()
        path.write_bytes(b"corrupt")
        with self.assertRaisesRegex(ValueError, "identity mismatch"):
            self.capture.collect(self.source, "2026-10-09T12:00:01Z")
        self.assertEqual(len(read_events(batch)), 1)
        path.write_bytes(original)
        self.capture.collect(self.source, "2026-10-09T12:00:02Z")
        self.assertEqual(len(read_events(batch)), 2)

    def test_same_state_checkpoint_compaction_is_not_a_capture_gap(self):
        self.fixture.product = "notams"
        self.fixture.publish("v1")
        old = self.capture.collect(self.source, "2026-10-09T23:59:59Z")
        old_path = self.publication / "versions/notams/v1.json"
        new_relative = "versions/notams/v1.checkpoint.json"
        old_path.rename(self.publication / new_relative)
        self.fixture.current["products"]["notams"]["version_manifest_url"] = new_relative
        atomic_json(self.publication / "current.json", self.fixture.current)
        new = self.capture.collect(self.source, "2026-10-10T00:00:00Z")
        self.assertNotEqual(old, new)
        self.assertEqual(read_events(new)[0]["head"]["version"], "v1")
        self.assertNotIn("gap", read_events(new)[0])

    def test_directory_copies_all_pages_and_tiles_and_rejects_symlinks(self):
        descriptor = self.publication / "states/nexrad/v1/manifest.json"
        descriptor.parent.mkdir(parents=True)
        descriptor.write_bytes(b'{"all_levels":true}')
        for name in ("tiles/res0/1/1.png", "tiles/res2/2/3.png"):
            path = descriptor.parent / name
            path.parent.mkdir(parents=True)
            path.write_bytes(b"unchanged compressed PNG")
        ref = {"url": str(descriptor.relative_to(self.publication)), "bytes": descriptor.stat().st_size,
               "blob_sha256": hashlib.sha256(descriptor.read_bytes()).hexdigest()}
        output = self.root / "copy"
        copy_payload(self.publication, ref, output)
        self.assertEqual(len(list(output.rglob("*.png"))), 2)
        (descriptor.parent / "escape").symlink_to("/etc/passwd")
        with self.assertRaisesRegex(ValueError, "symlink"):
            copy_payload(self.publication, ref, output)

    def test_member_cannot_escape_archive_or_source(self):
        for name in ("/etc/passwd", "../secret", "x/../../secret"):
            with self.assertRaises(ValueError):
                member(self.root, name)

    def test_already_pruned_directory_member_cannot_be_archived_as_complete(self):
        self.fixture.product = "obstacles"
        value = self.fixture.publish("v1")
        directory = self.publication / "states/obstacles/v1"
        directory.mkdir()
        (directory / "manifest.json").write_text('{"schema_version":1}')
        (directory / "tile.png").write_bytes(b"PNG bytes")
        package = self.publication / "whole.zip"
        with zipfile.ZipFile(package, "w") as archive:
            for path in directory.iterdir():
                archive.write(path, path.name)
        def ref(path):
            raw = path.read_bytes()
            return {"url": str(path.relative_to(self.publication)), "bytes": len(raw),
                    "blob_sha256": hashlib.sha256(raw).hexdigest()}
        value.update(state=ref(directory / "manifest.json"), install_state=ref(package))
        atomic_json(self.publication / "versions/obstacles/v1.json", value)
        (directory / "tile.png").unlink()
        with self.assertRaisesRegex(ValueError, "directory inventory"):
            self.capture.collect(self.source, "2026-10-09T12:00:00Z")
        self.assertIsNone(self.capture.active(self.source.identity))
        (directory / "tile.png").write_bytes(b"PNG bytes")
        (directory / "stats.json").write_text('{"diagnostic_only":true}')
        batch = self.capture.collect(self.source, "2026-10-09T12:00:01Z")
        event = read_events(batch)[0]
        self.assertEqual(len(list((batch / event["directory"]).rglob("tile.png"))), 1)

    def test_notam_baseline_materializes_checkpoint_suffix_once_per_day(self):
        self.fixture.product = "notams"
        value = self.fixture.publish("current")
        state = self.fixture.payload("states/notams/base.json.xz", {"state_id": "base"})
        state["kind"] = "notam_checkpoint_xz"
        delta = self.fixture.payload("deltas/notams/base-current.json.xz", {"from_state_id": "base", "to_state_id": "current"})
        delta.update(from_version="base", to_version="current")
        value.update(state=state, recent_deltas=[delta])
        atomic_json(self.publication / "versions/notams/current.json", value)
        calls = []
        def helper(command, **kwargs):
            calls.append(json.loads(kwargs["input"]))
            return mock.Mock(stdout='{"state_id":"current","records":[]}')
        with mock.patch("live_feed_archive.subprocess.run", side_effect=helper):
            old = self.capture.collect(self.source, "2026-10-09T23:59:59Z")
            new = self.capture.collect(self.source, "2026-10-10T00:00:00Z")
            self.capture.collect(self.source, "2026-10-10T00:00:01Z")
        self.assertEqual(len(calls), 2)
        self.assertEqual(calls[0]["target_state_id"], "current")
        self.assertEqual(calls[0]["deltas"][0]["from_state_id"], "base")
        self.assertNotEqual(old, new)
        event = read_events(new)[0]
        ref = event["payloads"][0]
        self.assertEqual(json.loads(lzma.decompress((new / event["directory"] / ref["url"]).read_bytes()))["state_id"], "current")

    def test_nexrad_inventory_includes_coarse_levels_absent_from_offline_profiles(self):
        directory = self.publication / "frame"
        directory.mkdir(parents=True)
        atomic_json(directory / "manifest.json", {
            "levels": [{"res": res, "tile_cols": 1, "tile_rows": 1} for res in range(4)],
            "tile_path_template": "tiles/res{res}/{x}/{y}.png",
        })
        for res in range(4):
            path = directory / f"tiles/res{res}/0/0.png"
            path.parent.mkdir(parents=True)
            path.write_bytes(b"PNG" + bytes([res]))
        package = self.publication / "offline.zip"
        with zipfile.ZipFile(package, "w") as zip_file:
            for name in ("manifest.json", "tiles/res0/0/0.png", "tiles/res1/0/0.png"):
                zip_file.write(directory / name, name)
        value = {"product": "nexrad", "state": {"url": "frame/manifest.json"}, "install_profiles": {
            "offline": {"url": "offline.zip", "bytes": package.stat().st_size,
                        "blob_sha256": hashlib.sha256(package.read_bytes()).hexdigest()},
        }}
        verify_directory(self.publication, value, self.publication)
        (directory / "tiles/res3/0/0.png").unlink()
        with self.assertRaisesRegex(ValueError, "directory inventory"):
            verify_directory(self.publication, value, self.publication)


if __name__ == "__main__":
    unittest.main()
