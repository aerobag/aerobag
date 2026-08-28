#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import json
import lzma
import tempfile
import unittest
import zipfile
from pathlib import Path

import build_release_journey_fixture as fixture
import materialize_release_journey_fixture as materializer


class ReleaseJourneyFixtureTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def make_zip(self, name: str, members: dict[str, bytes]) -> Path:
        path = self.root / name
        with zipfile.ZipFile(path, "w") as archive:
            for member, payload in members.items():
                archive.writestr(member, payload)
        return path

    def test_slippy_tms_tile_matches_seattle_chart_coordinates(self) -> None:
        x, y = fixture.slippy_tms_tile(*fixture.MAP_CENTER, 11)
        self.assertTrue(320 <= x <= 330)
        self.assertTrue(1310 <= y <= 1340)
        self.assertTrue(fixture.tile_is_in_window(f"tiles/0/11/{x}/{y}.webp"))
        self.assertFalse(fixture.tile_is_in_window(f"tiles/0/11/{x + 100}/{y}.webp"))

    def test_tile_window_includes_contextual_tac_inset_location(self) -> None:
        x, y = fixture.slippy_tms_tile(33.9425, -118.4081, 11)
        self.assertTrue(fixture.tile_is_in_window(f"tiles/0/11/{x}/{y}.webp"))

    def test_tile_window_covers_tall_android_viewport(self) -> None:
        x, y = fixture.slippy_tms_tile(*fixture.MAP_CENTER, 10)
        self.assertTrue(fixture.tile_is_in_window(f"tiles/0/10/{x}/{y + 3}.webp"))
        self.assertFalse(fixture.tile_is_in_window(f"tiles/0/10/{x}/{y + 4}.webp"))

    def test_package_selection_includes_contextual_tac_inset_region(self) -> None:
        packages = [
            {"family_id": family, "region_id": region}
            for family, region in (
                ("nav-db", None),
                ("world-basemap", None),
                ("sec", "nw"),
                ("sec", "sw"),
                ("tac", "nw"),
                ("tac", "sw"),
                ("enr-l", "nw"),
                ("enr-h", "nw"),
                ("terrain", "nw"),
                ("shaded-relief", "nw"),
                ("tpp", "nw"),
                ("csup", "nw"),
            )
        ]
        selected = fixture.selected_packages({"packages": packages})
        self.assertIn(
            {"family_id": "tac", "region_id": "sw"},
            selected,
        )
        self.assertIn(
            {"family_id": "sec", "region_id": "sw"},
            selected,
        )

    def test_compacts_tile_package_and_rewrites_path_manifest(self) -> None:
        x, y = fixture.slippy_tms_tile(*fixture.MAP_CENTER, 11)
        source = self.make_zip("tiles.zip", {
            "PACKAGE.manifest": (
                f"2608\ntiles/0/11/{x}/{y}.webp\n"
                f"tiles/0/11/{x + 100}/{y}.webp\n"
            ).encode(),
            f"tiles/0/11/{x}/{y}.webp": b"near",
            f"tiles/0/11/{x + 100}/{y}.webp": b"far",
            "chart-references/ref.json": b"{}",
        })
        destination = self.root / "compact.zip"
        self.assertEqual(1, fixture.compact_tile_package(source, destination))
        with zipfile.ZipFile(destination) as archive:
            self.assertIn(f"tiles/0/11/{x}/{y}.webp", archive.namelist())
            self.assertNotIn(f"tiles/0/11/{x + 100}/{y}.webp", archive.namelist())
            self.assertIn("chart-references/ref.json", archive.namelist())
            self.assertEqual(
                f"2608\ntiles/0/11/{x}/{y}.webp\n",
                archive.read("PACKAGE.manifest").decode(),
            )

    def test_compacted_package_preserves_nav_db_resource_path(self) -> None:
        x, y = fixture.slippy_tms_tile(*fixture.MAP_CENTER, 11)
        source = self.make_zip("sec_nw_SEC1_fixture.zip", {
            "PACKAGE.manifest": f"2608\ntiles/0/11/{x}/{y}.webp\n".encode(),
            f"tiles/0/11/{x}/{y}.webp": b"near",
        })
        output = self.root / "packaged"
        output.mkdir()
        package, _ = fixture.compact_package(
            self.root,
            output,
            {
                "id": "NW_SEC_SEC1_fixture",
                "family_id": "sec",
                "region_id": "nw",
                "relative_path": source.name,
            },
        )
        self.assertEqual(source.name, package["relative_path"])
        self.assertTrue((output / source.name).is_file())

    def test_compacts_asset_package_by_named_airport(self) -> None:
        manifest = {
            "schema_version": 2,
            "assets": [
                {"id": "a", "airport_id": "KSEA", "asset_path": "plates/KSEA/a.png", "thumbnail_path": "thumbnails/plates/KSEA/a.png"},
                {"id": "b", "airport_id": "KPAE", "asset_path": "plates/KPAE/b.png", "thumbnail_path": "thumbnails/plates/KPAE/b.png"},
            ],
        }
        source = self.make_zip("assets.zip", {
            "package-assets.json": json.dumps(manifest).encode(),
            "PACKAGE.manifest": b"2608\nplates/KSEA/a.png\nthumbnails/plates/KSEA/a.png\nplates/KPAE/b.png\nthumbnails/plates/KPAE/b.png\n",
            "plates/KSEA/a.png": b"a",
            "thumbnails/plates/KSEA/a.png": b"ta",
            "plates/KPAE/b.png": b"b",
            "thumbnails/plates/KPAE/b.png": b"tb",
        })
        destination = self.root / "compact-assets.zip"
        self.assertEqual(1, fixture.compact_asset_package(source, destination, {"KSEA"}))
        with zipfile.ZipFile(destination) as archive:
            compact = json.loads(archive.read("package-assets.json"))
            self.assertEqual(["KSEA"], [value["airport_id"] for value in compact["assets"]])
            self.assertNotIn("plates/KPAE/b.png", archive.namelist())

    def test_asset_compaction_fails_when_capability_disappears(self) -> None:
        source = self.make_zip("assets.zip", {
            "package-assets.json": b'{"schema_version":2,"assets":[]}',
            "PACKAGE.manifest": b"2608\n",
        })
        with self.assertRaisesRegex(fixture.BuildError, "missing airports"):
            fixture.compact_asset_package(source, self.root / "compact.zip", {"KSEA"})

    def test_compacts_schema_three_live_feed_with_bounded_history(self) -> None:
        source = self.root / "live-source"
        for version in ("old", "middle", "new"):
            (source / "versions" / "metars").mkdir(parents=True, exist_ok=True)
            (source / "states" / "metars").mkdir(parents=True, exist_ok=True)
            (source / "states" / "metars" / f"{version}.json.xz").write_bytes(version.encode())
            manifest = {
                "schema_version": 3,
                "product": "metars",
                "version": version,
                "state": {"kind": "json_xz", "url": f"states/metars/{version}.json.xz"},
            }
            if version == "new":
                delta = source / "deltas" / "metars" / "middle__new.json.xz"
                delta.parent.mkdir(parents=True, exist_ok=True)
                delta.write_bytes(b"delta")
                manifest["recent_deltas"] = [{
                    "kind": "ordered_delta_xz",
                    "url": "deltas/metars/middle__new.json.xz",
                }]
            fixture.write_json(source / "versions" / "metars" / f"{version}.json", manifest)
        fixture.write_json(source / "current.json", {
            "schema_version": 3,
            "generated_at_utc": "2026-08-20T00:00:00Z",
            "products": {
                "metars": {
                    "current": "new",
                    "version_manifest_url": "versions/metars/new.json",
                    "state_url": "states/metars/new.json.xz",
                    "collected_at_utc": "2026-08-20T00:00:00Z",
                    "history": [
                        {"version": "old", "version_manifest_url": "versions/metars/old.json"},
                        {"version": "middle", "version_manifest_url": "versions/metars/middle.json"},
                    ],
                },
            },
        })
        output = self.root / "live-output"
        diagnostics = fixture.compact_live_feed_fixture(source, output, stale=True)
        compact = fixture.read_json(output / "current.json")
        self.assertEqual(3, compact["schema_version"])
        self.assertEqual("2020-01-01T00:00:00Z", compact["products"]["metars"]["collected_at_utc"])
        self.assertEqual(2, diagnostics["version_count"])
        self.assertFalse((output / "states" / "metars" / "old.json.xz").exists())
        self.assertTrue((output / "states" / "metars" / "new.json.xz").is_file())
        self.assertTrue((output / "deltas" / "metars" / "middle__new.json.xz").is_file())

    def test_materializer_expands_packaged_resources(self) -> None:
        source = self.root / "source"
        packaged = source / "published" / "release-e2e-v1" / "packaged"
        packaged.mkdir(parents=True)
        package = packaged / "sample.zip"
        with zipfile.ZipFile(package, "w") as archive:
            archive.writestr("tiles/1/2/3.webp", b"tile")
        fixture.write_json(source / "published" / "current_artifacts.json", [{
            "artifact_roots": {
                "packaged": "release-e2e-v1/packaged/",
                "unpacked": "release-e2e-v1/unpacked/",
            },
        }])
        fixture.write_json(source / "fixture.json", {
            "schema_version": 1,
            "publication_root": "published",
        })
        fixture.write_json(source / "live-feeds" / "fresh" / "current.json", {
            "schema_version": 3,
            "generated_at_utc": "2026-08-20T04:13:35.500Z",
            "products": {
                "metars": {"collected_at_utc": "2026-08-20T04:13:35.500Z"},
            },
        })
        output = self.root / "materialized"
        materializer.materialize(source, output)
        self.assertEqual(
            b"tile",
            (output / "published" / "release-e2e-v1" / "unpacked" / "sample" / "tiles" / "1" / "2" / "3.webp").read_bytes(),
        )
        materialized_live_feed = fixture.read_json(
            output / "live-feeds" / "fresh" / "current.json"
        )
        self.assertEqual(
            "2026-08-20T04:13:35.500Z",
            materialized_live_feed["generated_at_utc"],
        )
        self.assertEqual(
            "2026-08-20T04:13:35.500Z",
            materialized_live_feed["products"]["metars"]["collected_at_utc"],
        )

    def test_live_feed_variants_include_real_fresh_stale_and_missing_states(self) -> None:
        source = self.root / "live-source"
        products = {"tfrs": {}, "pireps": {}}
        for product in products:
            version = f"{product}-v1"
            state = source / "states" / product / f"{version}.json.xz"
            state.parent.mkdir(parents=True, exist_ok=True)
            payload = {
                "schema_version": 1,
                "version_label": version,
            }
            if product == "pireps":
                payload.update({
                    "generated_at_utc": "2026-08-20T00:00:00Z",
                    "observed_at_utc": "2026-08-20T00:00:00Z",
                    "pirep_count": 0,
                    "pireps_by_id": {},
                })
            state.write_bytes(lzma.compress(fixture.canonical_json_bytes(payload)))
            manifest = source / "versions" / product / f"{version}.json"
            fixture.write_json(manifest, {
                "schema_version": 3,
                "product": product,
                "version": version,
                "state": {"kind": "json_xz", "url": f"states/{product}/{version}.json.xz"},
            })
            products[product] = {
                "current": version,
                "version_manifest_url": f"versions/{product}/{version}.json",
                "state_url": f"states/{product}/{version}.json.xz",
                "collected_at_utc": "2026-08-20T00:00:00Z",
                "history": [],
            }
        fixture.write_json(source / "current.json", {
            "schema_version": 3,
            "generated_at_utc": "2026-08-20T00:00:00Z",
            "products": products,
        })
        diagnostics = fixture.write_auxiliary_fixtures(self.root / "fixture", source)
        mixed = fixture.read_json(self.root / "fixture/live-feeds/mixed/current.json")
        self.assertEqual("2020-01-01T00:00:00Z", mixed["products"]["tfrs"]["collected_at_utc"])
        self.assertNotIn("pireps", mixed["products"])
        self.assertTrue(any(value.get("mixed") for value in diagnostics))
        fresh_state = lzma.decompress(next(
            (self.root / "fixture/live-feeds/fresh/states/pireps").glob("*.json.xz")
        ).read_bytes())
        self.assertIn(fixture.FIXTURE_PIREP_ID, json.loads(fresh_state)["pireps_by_id"])

    def test_live_feed_reference_epoch_comes_from_source_generation(self) -> None:
        source = self.root / "live-source"
        fixture.write_json(source / "current.json", {
            "schema_version": 3,
            "generated_at_utc": "2026-08-20T04:13:35.500Z",
            "products": {},
        })
        self.assertEqual(
            1_787_199_215_500,
            fixture.live_feed_reference_epoch_ms(source),
        )

    def test_replay_fixture_uses_real_trace_shape_with_track_gap(self) -> None:
        fixture.write_replay_fixture(self.root / "fixture")
        replay = fixture.read_json(self.root / "fixture/replay/track-gap.json")
        self.assertIsInstance(replay["trace"], list)
        self.assertGreaterEqual(len(replay["trace"]), 6)
        self.assertEqual([None, None], [replay["trace"][2][5], replay["trace"][3][5]])
        missing_track_seconds = replay["trace"][4][0] - replay["trace"][2][0]
        self.assertGreaterEqual(missing_track_seconds / 0.25, 10.0)


if __name__ == "__main__":
    unittest.main()
