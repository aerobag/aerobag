# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import numpy as np
from osgeo import gdal, osr

spec = importlib.util.spec_from_file_location(
    "chart_quality", Path(__file__).resolve().parents[1]
    / "product/preprocessor/preprocessor-charts/chart_quality.py",
)
quality = importlib.util.module_from_spec(spec)
spec.loader.exec_module(quality)


class ChartFixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.sources = self.root / "sources"
        self.sources.mkdir()
        self.metadata = self.root / "metadata"
        (self.metadata / "SEC").mkdir(parents=True)
        self.output = self.root / "state/chart-quality/SEC"
        self.cutline = self.metadata / "SEC/Test SEC.geojson"
        self.cutline.write_text(json.dumps({
            "type": "FeatureCollection", "crs": {"type": "name", "properties": {"name": "EPSG:3857"}},
            "features": [{"type": "Feature", "properties": {}, "geometry": {"type": "Polygon", "coordinates": [
                [[100, 700], [900, 700], [900, 100], [100, 100], [100, 700]],
            ]}}],
        }))
        self.inset_file = self.metadata / "SEC/Test SEC.navigable-insets.json"
        srs = osr.SpatialReference()
        srs.ImportFromEPSG(3857)
        self.inset_file.write_text(json.dumps({"schema_version": 2, "source": "Test SEC.tif",
            "source_width": 1000, "source_height": 800, "insets": [{"id": "City", "target_family": "TAC", "enabled": True,
            "projection_wkt": srs.ExportToWkt(),
            "boundary": [[200, 200], [800, 200], [800, 650], [200, 650]],
            "control_points": [{"kind": "intersection", "pixel": [x, y], "latitude": 43, "longitude": -88}
                               for x, y in [(300, 300), (700, 300), (300, 600), (700, 600)]]}]}))
        y, x = np.indices((800, 1000))
        self.original = np.stack([np.full(x.shape, c, dtype=np.uint8) for c in [225, 232, 207]])
        self.original[:, (x % 100 < 4) | (y % 100 < 4)] = np.array([80, 90, 100])[:, None]
        self.original[:, ((x + y // 3) % 83 < 5)] = np.array([130, 80, 100])[:, None]
        self.original[:, (x - 450)**2 + (y - 450)**2 < 90**2] = np.array([150, 185, 230])[:, None]
        self.write_source(self.original)

    def write_source(self, pixels):
        path = self.sources / "Test SEC.tif"
        dataset = gdal.GetDriverByName("GTiff").Create(str(path), pixels.shape[2], pixels.shape[1], 3, gdal.GDT_Byte)
        dataset.SetGeoTransform((0, 1, 0, 800, 0, -1))
        srs = osr.SpatialReference()
        srs.ImportFromEPSG(3857)
        dataset.SetProjection(srs.ExportToWkt())
        for i, band in enumerate(pixels, 1):
            dataset.GetRasterBand(i).WriteArray(band)
        dataset = None

    def check(self, cycle="2610"):
        return quality.run_check(self.sources, self.metadata, "SEC", self.output, "synthetic-source", cycle)

    def approve_all(self):
        report = self.check()
        self.assertEqual(report["status"], "warning", report)
        for region in report["regions"]:
            candidate = self.output / "reports" / report["report_id"] / region["id"] / "candidate.json"
            quality.approve(candidate, self.metadata, "test operator")
        return report

    def inset(self, report):
        return next(r for r in report["regions"] if r["kind"] == "inset")


class ChartVisualReferencesTest(ChartFixture):
    def test_missing_references_are_unreviewed_not_automatically_approved(self):
        report = self.check()
        self.assertEqual(report["unreviewed_count"], 2)
        self.assertEqual(report["critical_count"], 0, report)
        self.assertFalse((self.metadata / "SEC/visual-references").exists())

    def test_approved_round_trip_is_exact_even_across_cycles(self):
        self.approve_all()
        approved = {p: p.read_bytes() for p in (self.metadata / "SEC/visual-references").glob("*.json")}
        report = self.check("2611")
        self.assertEqual(report["status"], "ok", report)
        self.assertTrue(all(value == 0 for r in report["regions"] for v in r["scores"].values() for value in v.values()))
        self.assertEqual(approved, {p: p.read_bytes() for p in approved})

    def test_small_label_change_does_not_trip_drift_alarm(self):
        self.approve_all()
        changed = self.original.copy()
        changed[:, 420:430, 610:630] = 20
        self.write_source(changed)
        self.assertEqual(self.check()["status"], "ok")

    def test_translation_and_scaling_block_manual_georeferences(self):
        self.approve_all()
        changed = np.zeros_like(self.original)
        changed[:, :, 30:] = self.original[:, :, :-30]
        self.write_source(changed)
        translated = self.check()
        self.assertEqual(self.inset(translated)["status"], "critical", translated)
        y = np.clip(((np.arange(800) - 400) / 1.10 + 400).astype(int), 0, 799)
        x = np.clip(((np.arange(1000) - 500) / 1.10 + 500).astype(int), 0, 999)
        self.write_source(self.original[:, y[:, None], x[None, :]])
        scaled = self.check()
        self.assertEqual(self.inset(scaled)["status"], "critical", scaled)

    def test_control_patch_catches_local_movement_hidden_in_overview(self):
        self.approve_all()
        changed = self.original.copy()
        changed[:, 245:355, 245:355] = np.roll(changed[:, 245:355, 245:355], 25, axis=2)
        self.write_source(changed)
        report = self.check()
        inset = self.inset(report)
        self.assertEqual(inset["status"], "critical", report)
        self.assertLess(inset["scores"]["overview"]["interior"], quality.POLICY["interior"]["warning"])
        self.assertGreater(inset["scores"]["control-1"]["control"], quality.POLICY["control"]["critical"])

    def test_boundary_leak_is_scored_separately_from_interior(self):
        self.approve_all()
        changed = self.original.copy()
        changed[:, 92:104, 100:900] = 0
        self.write_source(changed)
        report = self.check()
        cutline = next(r for r in report["regions"] if r["kind"] == "cutline")
        self.assertEqual(cutline["status"], "warning", report)
        self.assertLess(cutline["scores"]["overview"]["interior"], quality.POLICY["interior"]["warning"])

    def test_stale_candidate_cannot_be_approved(self):
        report = self.check()
        region = self.inset(report)
        candidate = self.output / "reports" / report["report_id"] / region["id"] / "candidate.json"
        layout = json.loads(self.inset_file.read_text())
        layout["insets"][0]["control_points"][0]["latitude"] = 44
        self.inset_file.write_text(json.dumps(layout))
        with self.assertRaisesRegex(ValueError, "stale"):
            quality.approve(candidate, self.metadata, "operator")

    def test_geometry_edit_needs_review_even_when_pixels_match(self):
        self.approve_all()
        layout = json.loads(self.inset_file.read_text())
        layout["insets"][0]["control_points"][0]["latitude"] = 44
        self.inset_file.write_text(json.dumps(layout))
        self.assertEqual(self.inset(self.check())["status"], "critical")

    def test_inset_projection_change_invalidates_approval_and_stale_candidates(self):
        report = self.approve_all()
        candidate = self.output / "reports" / report["report_id"] / self.inset(report)["id"] / "candidate.json"
        layout = json.loads(self.inset_file.read_text())
        srs = osr.SpatialReference()
        srs.ImportFromEPSG(26916)
        layout['insets'][0]['projection_wkt'] = srs.ExportToWkt()
        self.inset_file.write_text(json.dumps(layout))
        current = self.check()
        self.assertEqual(self.inset(current)['status'], 'critical', current)
        self.assertIn('Cutline/georeference changed', self.inset(current)['message'])
        self.assertTrue(all(value == 0 for view in self.inset(current)['scores'].values() for value in view.values()))
        with self.assertRaisesRegex(ValueError, 'stale'):
            quality.approve(candidate, self.metadata, 'operator')

    def test_source_projection_change_alarms_even_when_inset_projection_and_pixels_are_pinned(self):
        self.approve_all()
        original = self.inset_file.read_bytes()
        source = gdal.Open(str(self.sources / 'Test SEC.tif'), gdal.GA_Update)
        srs = osr.SpatialReference()
        srs.ImportFromEPSG(26916)
        source.SetProjection(srs.ExportToWkt())
        source = None
        current = self.check()
        self.assertEqual(self.inset(current)['status'], 'critical', current)
        self.assertIn('Source projection changed', self.inset(current)['message'])
        self.assertEqual(self.inset_file.read_bytes(), original)

    def test_corrupt_reference_is_not_treated_as_missing(self):
        report = self.approve_all()
        path = quality.reference_path(self.metadata, "SEC", self.inset(report)["id"])
        value = json.loads(path.read_text())
        value["views"][0]["pixels"] = "corrupt"
        path.write_text(json.dumps(value))
        result = self.check()
        self.assertEqual(self.inset(result)["status"], "critical")
        self.assertIn("hash mismatch", self.inset(result)["message"])

    def test_missing_source_and_interrupted_checks_do_not_reuse_green(self):
        self.approve_all()
        (self.sources / "Test SEC.tif").unlink()
        self.assertEqual(self.check()["critical_count"], 2)
        with patch.object(quality, "regions", side_effect=KeyboardInterrupt):
            with self.assertRaises(KeyboardInterrupt):
                self.check()
        self.assertEqual(json.loads((self.output / "current.json").read_text())["status"], "checking")

    def test_reports_are_bounded_but_references_are_not_garbage_collected(self):
        self.approve_all()
        for i in range(4):
            report = self.check(str(2610 + i))
        self.assertEqual(len(list((self.output / "reports").iterdir())), 3)
        self.assertTrue((self.output / "reports" / report["report_id"] / "index.html").is_file())
        self.assertEqual(len(list((self.metadata / "SEC/visual-references").glob("*.json"))), 2)

    def test_missing_native_dependency_replaces_previous_success_with_failure(self):
        self.approve_all()
        self.assertEqual(self.check()["status"], "ok")
        with patch.object(quality, "initialize_rendering", side_effect=ImportError("No GDAL")):
            report = self.check()
        self.assertEqual(report["status"], "critical")
        self.assertEqual(report["critical_count"], 1)
        self.assertEqual(json.loads((self.output / "current.json").read_text())["status"], "critical")
        self.assertIn("No GDAL", report["error"])

    def test_destinations_check_the_same_reference_without_a_sectional_rebuild(self):
        approved = self.approve_all()
        expected_id = self.inset(approved)["id"]
        (self.metadata / "TAC").mkdir()
        report = quality.run_check(self.sources, self.metadata, "TAC", self.output.parent / "TAC", "tac-build", "2611")
        self.assertEqual(report["status"], "ok", report)
        self.assertEqual([r["id"] for r in report["regions"]], [expected_id])
        self.assertFalse((self.metadata / "TAC/visual-references").exists())
        self.write_source(np.roll(self.original, 30, axis=2))
        report = quality.run_check(self.sources, self.metadata, "TAC", self.output.parent / "TAC", "tac-build", "2611")
        self.assertEqual(report["status"], "critical", report)
        (self.metadata / "FLY").mkdir()
        self.assertEqual(quality.regions(self.metadata, "FLY"), [])

    def test_faa_main_map_placement_change_is_not_a_pixel_match(self):
        self.approve_all()
        dataset = gdal.Open(str(self.sources / "Test SEC.tif"), gdal.GA_Update)
        dataset.SetGeoTransform((10, 1, 0, 800, 0, -1))
        dataset = None
        report = self.check()
        main = next(r for r in report["regions"] if r["kind"] == "cutline")
        self.assertEqual(main["status"], "warning")
        self.assertIn("pixel-to-map placement", main["message"])
        self.assertEqual(self.inset(report)["status"], "ok", "Inset uses the CRS, not the parent's affine placement")

    def test_source_archive_overlay_matches_builder_order_without_case_ambiguity(self):
        primary = self.root / "primary"
        primary.mkdir()
        (primary / "Test SEC.tif").write_bytes(b"older overlapping archive member")
        self.assertEqual(quality.source_path([primary, self.sources], "test sec.TIF"), self.sources / "Test SEC.tif")
        (primary / "TEST SEC.TIF").write_bytes(b"ambiguous case-only duplicate")
        with self.assertRaisesRegex(ValueError, "found 2"):
            quality.source_path([primary, self.sources], "test sec.TIF")

    def test_candidate_and_comparison_read_each_source_window_once(self):
        self.approve_all()
        with patch.object(quality, "sample", wraps=quality.sample) as reader:
            self.assertEqual(self.check()["status"], "ok")
        self.assertEqual(reader.call_count, 6, "two overviews and four control patches, not duplicate decodes")


if __name__ == "__main__":
    unittest.main()
