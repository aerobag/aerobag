# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path

import numpy as np
from osgeo import gdal, osr

try:
    from chart_cutline_editor import (
        EditorCatalog,
        EditorState,
        RevisionConflict,
        file_revision,
        find_snap_candidate,
        navigable_inset_diagnostics,
        inset_georeference,
    )
except ImportError:
    from tools.chart_cutline_editor import (
        EditorCatalog,
        EditorState,
        RevisionConflict,
        file_revision,
        find_snap_candidate,
        navigable_inset_diagnostics,
        inset_georeference,
    )


class EditorStateTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)
        self.work_dir = self.root / "work"
        self.cutline_dir = self.root / "cutlines" / "TAC"
        self.cache_dir = self.root / "cache"
        self.work_dir.mkdir(parents=True)
        self.cutline_dir.mkdir(parents=True)
        self.source_path = self.work_dir / "Test TAC.tif"
        self.cutline_path = self.cutline_dir / "Test TAC.geojson"
        self._write_test_raster()
        self._write_test_cutline()
        self.state = EditorState(
            self.work_dir,
            self.cutline_dir,
            self.cache_dir,
            overview_width=60,
        )

    def tearDown(self) -> None:
        self.temp_dir.cleanup()

    def _write_test_raster(self) -> None:
        dataset = gdal.GetDriverByName("GTiff").Create(
            str(self.source_path),
            120,
            100,
            3,
            gdal.GDT_Byte,
        )
        dataset.SetGeoTransform((1000.0, 2.0, 0.0, 2000.0, 0.0, -2.0))
        srs = osr.SpatialReference()
        srs.ImportFromEPSG(3857)
        dataset.SetProjection(srs.ExportToWkt())
        for index, value in enumerate((240, 242, 244), start=1):
            dataset.GetRasterBand(index).Fill(value)
        dataset = None

    def _write_test_cutline(self) -> None:
        pixel_points = [(10.0, 12.0), (90.0, 12.0), (90.0, 80.0), (10.0, 80.0)]
        projected = [[1000.0 + x * 2.0, 2000.0 - y * 2.0] for x, y in pixel_points]
        projected.append(projected[0].copy())
        document = {
            "type": "FeatureCollection",
            "name": "Test TAC.tif",
            "crs": {
                "type": "name",
                "properties": {"name": "urn:ogc:def:crs:EPSG::3857"},
            },
            "features": [
                {
                    "type": "Feature",
                    "properties": {"location": "Test TAC.tif", "keep": "me"},
                    "geometry": {"type": "Polygon", "coordinates": [projected]},
                }
            ],
        }
        self.cutline_path.write_text(json.dumps(document), encoding="utf-8")

    def test_load_and_save_round_trip_in_source_pixels(self) -> None:
        payload = self.state.chart_payload("Test TAC")
        expected = [[10.0, 12.0], [90.0, 12.0], [90.0, 80.0], [10.0, 80.0]]
        assert_points_close(self, payload["points"], expected)

        edited = [[11.5, 13.25], [91.0, 12.0], [90.0, 81.0], [9.0, 80.0]]
        result = self.state.save_points(
            "Test TAC",
            edited,
            payload["revision"],
        )
        self.assertEqual(result["revision"], file_revision(self.cutline_path))
        assert_points_close(self, self.state.chart_payload("Test TAC")["points"], edited)
        document = json.loads(self.cutline_path.read_text(encoding="utf-8"))
        self.assertEqual(document["features"][0]["properties"]["keep"], "me")
        ring = document["features"][0]["geometry"]["coordinates"][0]
        self.assertEqual(ring[0], ring[-1])

    def test_save_rejects_stale_revision(self) -> None:
        payload = self.state.chart_payload("Test TAC")
        self.state.save_points("Test TAC", payload["points"], payload["revision"])
        with self.assertRaises(RevisionConflict):
            self.state.save_points("Test TAC", payload["points"], payload["revision"])

    def test_crs84_uses_longitude_latitude_axis_order(self) -> None:
        dataset = gdal.GetDriverByName("GTiff").Create(
            str(self.source_path),
            120,
            100,
            3,
            gdal.GDT_Byte,
        )
        dataset.SetGeoTransform((-90.0, 0.01, 0.0, 43.0, 0.0, -0.01))
        srs = osr.SpatialReference()
        srs.ImportFromEPSG(4326)
        dataset.SetProjection(srs.ExportToWkt())
        for index in range(1, 4):
            dataset.GetRasterBand(index).Fill(240)
        dataset = None

        pixel_points = [[10.0, 12.0], [90.0, 12.0], [90.0, 80.0], [10.0, 80.0]]
        geographic = [[-90.0 + x * 0.01, 43.0 - y * 0.01] for x, y in pixel_points]
        geographic.append(geographic[0].copy())
        document = json.loads(self.cutline_path.read_text(encoding="utf-8"))
        document["crs"]["properties"]["name"] = "urn:ogc:def:crs:OGC:1.3:CRS84"
        document["features"][0]["geometry"]["coordinates"] = [geographic]
        self.cutline_path.write_text(json.dumps(document), encoding="utf-8")

        state = EditorState(
            self.work_dir,
            self.cutline_dir,
            self.cache_dir,
            overview_width=60,
        )
        payload = state.chart_payload("Test TAC")
        assert_points_close(self, payload["points"], pixel_points)
        edited = [[11.0, 13.0], [91.0, 13.0], [91.0, 81.0], [11.0, 81.0]]
        state.save_points("Test TAC", edited, payload["revision"])
        assert_points_close(self, state.chart_payload("Test TAC")["points"], edited)

    def test_overview_and_crop_are_pngs(self) -> None:
        overview = self.state.overview_png("Test TAC")
        crop = self.state.crop_png("Test TAC", 5, 6, 64, 64)
        edge_sliver = self.state.crop_png("Test TAC", 119, 0, 1, 64)
        self.assertTrue(overview.startswith(b"\x89PNG\r\n\x1a\n"))
        self.assertTrue(crop.startswith(b"\x89PNG\r\n\x1a\n"))
        self.assertTrue(edge_sliver.startswith(b"\x89PNG\r\n\x1a\n"))

    def test_paletted_overview_expands_to_rgb_before_resampling(self) -> None:
        dataset = gdal.GetDriverByName("GTiff").Create(
            str(self.source_path),
            121,
            101,
            1,
            gdal.GDT_Byte,
        )
        dataset.SetGeoTransform((1000.0, 2.0, 0.0, 2000.0, 0.0, -2.0))
        srs = osr.SpatialReference()
        srs.ImportFromEPSG(3857)
        dataset.SetProjection(srs.ExportToWkt())
        color_table = gdal.ColorTable()
        color_table.SetColorEntry(0, (0, 0, 0, 255))
        color_table.SetColorEntry(1, (255, 255, 255, 255))
        band = dataset.GetRasterBand(1)
        band.SetRasterColorTable(color_table)
        band.WriteArray((np.indices((101, 121)).sum(axis=0) % 2).astype(np.uint8))
        dataset = None

        state = EditorState(
            self.work_dir,
            self.cutline_dir,
            self.cache_dir / "palette",
            overview_width=60,
        )
        overview = state.overview_png("Test TAC")
        vsi_path = "/vsimem/chart-cutline-editor-overview-test.png"
        try:
            gdal.FileFromMemBuffer(vsi_path, overview)
            rendered = gdal.Open(vsi_path)
            self.assertIsNotNone(rendered)
            assert rendered is not None
            pixels = rendered.ReadAsArray()
            rendered = None
            values = np.unique(pixels)
            self.assertTrue(
                np.any((values > 0) & (values < 255)),
                f"expected anti-aliased intermediate values, got {values}",
            )
        finally:
            gdal.Unlink(vsi_path)

    def test_large_inset_has_bounded_overview_without_relaxing_full_resolution_limit(self):
        source = gdal.GetDriverByName("GTiff").Create(
            str(self.source_path), 5300, 4700, 1, gdal.GDT_Byte,
            options=["TILED=YES", "COMPRESS=DEFLATE"],
        )
        colors = gdal.ColorTable()
        colors.SetColorEntry(0, (200, 100, 50, 255))
        source.GetRasterBand(1).SetColorTable(colors)
        source.GetRasterBand(1).Fill(0)
        source = None
        state = EditorState(self.work_dir, self.cutline_dir, self.cache_dir, overview_width=60)
        window = (133, 100, 5129, 4446)
        with self.assertRaisesRegex(RuntimeError, "crop area must not exceed"):
            state.crop_png("Test TAC", *window)
        overview = state.crop_overview_png("Test TAC", *window)
        self.assertEqual(overview, state.crop_overview_png("Test TAC", *window))
        vsi_path = "/vsimem/large-inset-overview-test.png"
        try:
            gdal.FileFromMemBuffer(vsi_path, overview)
            raster = gdal.Open(vsi_path)
            self.assertEqual((raster.RasterXSize, raster.RasterYSize), (60, 52))
            self.assertEqual(raster.GetRasterBand(1).ReadAsArray()[25, 30], 200)
            raster = None
        finally:
            gdal.Unlink(vsi_path)
        with self.assertRaisesRegex(RuntimeError, "outside source chart"):
            state.crop_overview_png("Test TAC", 0, 0, 6000, 4446)
        with self.assertRaisesRegex(RuntimeError, "must be positive"):
            state.crop_overview_png("Test TAC", 0, 0, 0, 1)

    def test_cropped_overview_preserves_pixel_axes_and_crop_origin(self):
        source = gdal.Open(str(self.source_path), gdal.GA_Update)
        source.SetGeoTransform((1000, 2, 0.7, 2000, 0.5, -2))
        band = source.GetRasterBand(1)
        band.Fill(0)
        band.WriteArray(np.full((20, 20), 240, dtype=np.uint8), 30, 20)
        source = None
        # A portrait crop also bounds its height, not just its width.
        overview = self.state.crop_overview_png("Test TAC", 30, 20, 40, 80)
        vsi_path = "/vsimem/cropped-inset-pixel-axes.png"
        try:
            gdal.FileFromMemBuffer(vsi_path, overview)
            raster = gdal.Open(vsi_path)
            self.assertEqual((raster.RasterXSize, raster.RasterYSize), (30, 60))
            pixels = raster.GetRasterBand(1).ReadAsArray()
            self.assertEqual(int(pixels[5, 5]), 240)
            self.assertEqual(int(pixels[5, 25]), 0)
            self.assertEqual(int(pixels[25, 5]), 0)
            raster = None
        finally:
            gdal.Unlink(vsi_path)

    def test_extract_layouts_round_trip_independently_with_revision_guards(self) -> None:
        initial = self.state.extract_payload("Test TAC", "legend")
        self.assertIsNone(initial["revision"])
        self.assertEqual(initial["regions"], [])
        regions = [
            {"x": 3, "y": 4, "width": 20, "height": 30},
            {"x": 40.4, "y": 10.6, "width": 50.2, "height": 60.1},
        ]
        saved = self.state.save_extract("Test TAC", "legend", regions, 1210, None)
        self.assertIsInstance(saved["revision"], str)
        loaded = self.state.extract_payload("Test TAC", "legend")
        self.assertEqual(
            loaded["regions"],
            [
                {"x": 3, "y": 4, "width": 20, "height": 30},
                {"x": 40, "y": 11, "width": 50, "height": 60},
            ],
        )
        self.assertEqual(loaded["max_output_width"], 1210)
        inset = self.state.extract_payload("Test TAC", "inset")
        self.assertIsNone(inset["revision"])
        self.assertEqual(inset["regions"], [])
        inset_saved = self.state.save_extract("Test TAC", "inset", regions[:1], 1210, None)
        self.assertNotEqual(saved["revision"], inset_saved["revision"])
        self.assertEqual(
            self.state.extract_payload("Test TAC", "inset")["regions"],
            [{"x": 3, "y": 4, "width": 20, "height": 30}],
        )
        self.assertEqual(len(self.state.extract_payload("Test TAC", "legend")["regions"]), 2)
        with self.assertRaises(RevisionConflict):
            self.state.save_extract("Test TAC", "legend", regions, 1210, None)

    def test_extract_editor_supports_unreferenced_source_with_parent_coverage(self) -> None:
        source_path = self.work_dir / "Reference Sheet.tif"
        dataset = gdal.GetDriverByName("GTiff").Create(
            str(source_path),
            80,
            60,
            1,
            gdal.GDT_Byte,
        )
        color_table = gdal.ColorTable()
        color_table.SetColorEntry(0, (255, 255, 255, 255))
        color_table.SetColorEntry(1, (255, 0, 0, 255))
        color_table.SetColorEntry(2, (0, 0, 255, 255))
        band = dataset.GetRasterBand(1)
        band.SetRasterColorTable(color_table)
        pixels = np.ones((60, 80), dtype=np.uint8)
        pixels[30:, :] = 2
        band.WriteArray(pixels)
        dataset = None
        layout_path = self.cutline_dir / "Reference Sheet.inset.json"
        layout_path.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "source": "Reference Sheet.tif",
                    "source_width": 80,
                    "source_height": 60,
                    "max_output_width": 1210,
                    "coverage_source": "Test TAC",
                    "regions": [],
                }
            ),
            encoding="utf-8",
        )

        state = EditorState(
            self.work_dir,
            self.cutline_dir,
            self.cache_dir / "extract-only",
            overview_width=60,
        )
        self.assertEqual([chart["name"] for chart in state.chart_list()], ["Test TAC"])
        self.assertEqual(
            [chart["name"] for chart in state.chart_list(include_extract_only=True)],
            ["Test TAC", "Reference Sheet"],
        )
        payload = state.chart_payload("Reference Sheet")
        self.assertFalse(payload["cutline_editable"])
        self.assertEqual(payload["points"], [])
        overview = state.overview_png("Reference Sheet")
        vsi_path = "/vsimem/chart-cutline-editor-unreferenced-overview.png"
        try:
            gdal.FileFromMemBuffer(vsi_path, overview)
            rendered = gdal.Open(vsi_path)
            self.assertIsNotNone(rendered)
            assert rendered is not None
            rgb = rendered.ReadAsArray()
            rendered = None
            self.assertGreater(rgb[0, 2, 2], rgb[2, 2, 2])
            self.assertGreater(rgb[2, -3, 2], rgb[0, -3, 2])
        finally:
            gdal.Unlink(vsi_path)
        extract = state.extract_payload("Reference Sheet", "inset")
        saved = state.save_extract(
            "Reference Sheet",
            "inset",
            [{"x": 1, "y": 2, "width": 30, "height": 40}],
            1210,
            extract["revision"],
        )
        self.assertIsInstance(saved["revision"], str)
        document = json.loads(layout_path.read_text(encoding="utf-8"))
        self.assertEqual(document["coverage_source"], "Test TAC")

    def test_catalog_routes_charts_and_overviews_by_family(self) -> None:
        catalog = EditorCatalog({"SEC": self.state, "TAC": self.state, "FLY": self.state})
        self.assertEqual(
            catalog.family_list(),
            [
                {"id": "SEC", "label": "Sectional", "chart_count": 1},
                {"id": "TAC", "label": "TAC", "chart_count": 1},
                {"id": "FLY", "label": "Flyway", "chart_count": 1},
            ],
        )
        payload = catalog.chart_payload("SEC", "Test TAC")
        self.assertEqual(payload["family"], "SEC")
        self.assertEqual(
            payload["overview_url"],
            "/api/overview?family=SEC&name=Test%20TAC",
        )

    def test_chart_list_reports_navigable_inset_candidate_progress(self) -> None:
        self.state.save_navigable_insets(
            "Test TAC",
            [{
                "id": "First",
                "enabled": False, "target_family": "TAC",
                "boundary": [[10, 10], [100, 10], [100, 80], [10, 80]],
                "control_points": [],
            }],
            None,
        )
        (self.cutline_dir / "navigable-inset-candidates.json").write_text(
            json.dumps({
                "schema_version": 1,
                "charts": [{"source": "Test TAC.tif", "insets": ["First", "Second"]}],
            }),
            encoding="utf-8",
        )
        state = EditorState(
            self.work_dir,
            self.cutline_dir,
            self.cache_dir,
            overview_width=60,
        )

        self.assertEqual(
            state.chart_list(include_extract_only=True),
            [{
                "name": "Test TAC",
                "width": 120,
                "height": 100,
                "navigable_inset_candidates": ["First", "Second"],
                "navigable_inset_defined_count": 1,
                "navigable_inset_enabled_count": 0,
            }],
        )

    def test_navigable_insets_round_trip_with_transform_diagnostics(self) -> None:
        initial = self.state.navigable_inset_payload("Test TAC")
        self.assertIsNone(initial["revision"])
        self.assertNotIn("target_family", initial)
        controls = [
            {"kind": "intersection", "pixel": [20, 20], "latitude": 42.0, "longitude": -88.0},
            {"kind": "intersection", "pixel": [80, 20], "latitude": 42.0, "longitude": -87.9},
            {"kind": "intersection", "pixel": [20, 70], "latitude": 41.9, "longitude": -88.0},
            {"kind": "intersection", "pixel": [80, 70], "latitude": 41.9, "longitude": -87.9},
        ]
        saved = self.state.save_navigable_insets(
            "Test TAC",
            [{
                "id": "Test inset",
                "enabled": True, "target_family": "TAC",
                "boundary": [[10, 10], [100, 10], [100, 80], [10, 80]],
                "control_points": controls,
            }],
            None,
        )
        self.assertIsInstance(saved["revision"], str)
        self.assertTrue(saved["regions"][0]["diagnostics"]["ready"])
        loaded = self.state.navigable_inset_payload("Test TAC")
        self.assertEqual(loaded["regions"][0]["id"], "Test inset")
        self.assertTrue(loaded["regions"][0]["enabled"])
        document = json.loads(
            (self.cutline_dir / "Test TAC.navigable-insets.json").read_text(encoding="utf-8")
        )
        self.assertNotIn("diagnostics", document["insets"][0])
        self.assertNotIn("x", document["insets"][0])
        self.assertEqual(
            document["insets"][0]["boundary"],
            [[10.0, 10.0], [100.0, 10.0], [100.0, 80.0], [10.0, 80.0]],
        )
        with self.assertRaises(RevisionConflict):
            self.state.save_navigable_insets("Test TAC", [], None)

    def test_navigable_inset_boundary_must_stay_on_source_raster(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "boundary is outside the source raster"):
            self.state.save_navigable_insets(
                "Test TAC",
                [{
                    "id": "Outside",
                    "enabled": False, "target_family": "TAC",
                    "boundary": [[-1, 10], [100, 10], [100, 80], [10, 80]],
                    "control_points": [],
                }],
                None,
            )

    def test_georeference_controls_may_be_outside_cutline_but_not_source_raster(self):
        controls = [
            {"kind": "intersection", "pixel": [20, 20], "latitude": 42.0, "longitude": -88.0},
            {"kind": "intersection", "pixel": [80, 20], "latitude": 42.0, "longitude": -87.9},
            {"kind": "intersection", "pixel": [20, 70], "latitude": 41.9, "longitude": -88.0},
            {"kind": "intersection", "pixel": [80, 70], "latitude": 41.9, "longitude": -87.9},
        ]
        boundary = [[30, 30], [70, 30], [70, 60], [30, 60]]
        region = {"id": "Margin controls", "enabled": True, "target_family": "TAC",
                  "boundary": boundary, "control_points": controls}
        saved = self.state.save_navigable_insets("Test TAC", [region], None)
        loaded = self.state.navigable_inset_payload("Test TAC")
        self.assertEqual(loaded["regions"][0]["boundary"], boundary)
        self.assertEqual(loaded["regions"][0]["control_points"], controls)
        self.assertTrue(loaded["regions"][0]["diagnostics"]["ready"])
        controls[0]["pixel"][0] = -1
        with self.assertRaisesRegex(RuntimeError, "outside the source raster"):
            self.state.save_navigable_insets("Test TAC", [region], saved["revision"])

    def test_enabled_navigable_inset_rejects_incomplete_control_points(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "Need at least 8 coordinate constraints"):
            self.state.save_navigable_insets(
                "Test TAC",
                [{
                    "id": "Incomplete",
                    "enabled": True, "target_family": "TAC",
                    "boundary": [[10, 10], [100, 10], [100, 80], [10, 80]],
                    "control_points": [
                        {"kind": "intersection", "pixel": [20, 20], "latitude": 42.0, "longitude": -88.0},
                    ],
                }],
                None,
            )

    def test_disabled_navigable_inset_preserves_unfinished_control_point(self) -> None:
        saved = self.state.save_navigable_insets(
            "Test TAC",
            [
                {
                    "id": "Draft",
                    "enabled": False, "target_family": "TAC",
                    "boundary": [[10, 20], [100, 20], [100, 90], [10, 90]],
                    "control_points": [
                        {"kind": "intersection", "pixel": [30, 40], "latitude": None, "longitude": None},
                    ],
                }
            ],
            None,
        )
        self.assertEqual(saved["regions"][0]["control_points"][0]["latitude"], None)
        self.assertIn("Need at least 8", saved["regions"][0]["diagnostics"]["summary"])

    def test_partial_control_types_survive_save_reload(self):
        controls = [
            {"kind": "latitude", "pixel": [20, 20], "latitude": 58.25, "longitude": None},
            {"kind": "longitude", "pixel": [80, 70], "latitude": None, "longitude": -134.5},
            {"kind": "latitude", "pixel": [40, 40], "latitude": None, "longitude": None},
        ]
        region = {"id": "Ticks", "enabled": False, "target_family": "TAC",
                  "boundary": [[10, 10], [100, 10], [100, 80]], "control_points": controls}
        saved = self.state.save_navigable_insets("Test TAC", [region], None)
        self.assertEqual(self.state.navigable_inset_payload("Test TAC")["regions"][0]["control_points"], controls)
        controls[0]["longitude"] = -134.5
        with self.assertRaisesRegex(RuntimeError, "must not supply longitude"):
            self.state.save_navigable_insets("Test TAC", [region], saved["revision"])
        controls[0]["longitude"] = None
        region["enabled"] = True
        with self.assertRaisesRegex(RuntimeError, "needs latitude"):
            self.state.save_navigable_insets("Test TAC", [region], saved["revision"])

    def test_coordinate_entry(self):
        subprocess.run(["node", str(Path(__file__).with_name("test_coordinate_input.cjs"))], check=True)

    def test_output_layer_is_per_inset_and_required(self):
        regions = [
            {"id": name, "enabled": False, "target_family": target,
             "boundary": [[10, 10], [100, 10], [100, 80]], "control_points": []}
            for name, target in [("Juneau", "TAC"), ("Traffic Area", "FLY"), ("Glacier", "TAC")]
        ]
        saved = self.state.save_navigable_insets("Test TAC", regions, None)
        self.assertEqual(self.state.navigable_inset_payload("Test TAC")["regions"][1]["target_family"], "FLY")
        document = json.loads((self.cutline_dir / "Test TAC.navigable-insets.json").read_text())
        self.assertNotIn("target_family", document)
        regions[1]["target_family"] = "TAC"
        saved = self.state.save_navigable_insets("Test TAC", regions, saved["revision"])
        self.assertEqual(self.state.navigable_inset_payload("Test TAC")["regions"][1]["target_family"], "TAC")
        for target in ["SEC", None]:
            regions[1]["target_family"] = target
            with self.assertRaisesRegex(RuntimeError, "target_family must be TAC or FLY"):
                self.state.save_navigable_insets("Test TAC", regions, saved["revision"])
        del regions[1]["target_family"]
        with self.assertRaisesRegex(RuntimeError, "target_family must be TAC or FLY"):
            self.state.save_navigable_insets("Test TAC", regions, saved["revision"])


    def test_navigable_diagnostics_reject_collinear_controls(self) -> None:
        result = navigable_inset_diagnostics(self.source_path, [
            {"kind": "intersection", "pixel": [10, 10], "latitude": 42.0, "longitude": -88.0},
            {"kind": "intersection", "pixel": [20, 20], "latitude": 42.1, "longitude": -87.9},
            {"kind": "intersection", "pixel": [30, 30], "latitude": 42.2, "longitude": -87.8},
            {"kind": "intersection", "pixel": [40, 40], "latitude": 42.3, "longitude": -87.7},
        ])
        self.assertFalse(result["ready"])
        self.assertEqual(result["summary"], "Control points are collinear")


class ProjectedInsetTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / "Projected SEC.tif"
        self.srs = osr.SpatialReference()
        self.srs.ImportFromProj4(
            "+proj=lcc +lat_0=54.1666666666667 +lon_0=-168.5 "
            "+lat_1=54.6666666666667 +lat_2=49.3333333333333 +datum=NAD83 +units=m"
        )
        geographic = osr.SpatialReference()
        geographic.ImportFromEPSG(4326)
        geographic.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
        self.project = osr.CoordinateTransformation(geographic, self.srs)
        cx, cy, _ = self.project.TransformPoint(-166.5, 54)
        self.truth = (cx - 45000, 60, 2, cy + 45000, 1, -60)
        self.controls = []
        inverse = gdal.InvGeoTransform(self.truth)
        for lat in (53.75, 54, 54.25):
            for lon in (-167, -166.5, -166):
                point = self.project.TransformPoint(lon, lat)
                self.controls.append({
                    "kind": "intersection", "pixel": list(gdal.ApplyGeoTransform(inverse, *point[:2])),
                    "latitude": lat, "longitude": lon,
                })
        source = gdal.GetDriverByName("GTiff").Create(str(self.source), 1600, 1600, 1)
        source.SetProjection(self.srs.ExportToWkt())
        # Parent placement is deliberately unrelated to inset placement.
        source.SetGeoTransform((0, 10, 0, 0, 0, -10))
        colors = gdal.ColorTable()
        colors.SetColorEntry(0, (200, 100, 50, 255))
        source.GetRasterBand(1).SetColorTable(colors)
        source.GetRasterBand(1).Fill(0)
        source = None
        self.inset = {"control_points": self.controls,
                      "boundary": [[50, 50], [1550, 50], [50, 1550]]}

    def test_curved_graticule_predicts_unsampled_points_and_build_uses_same_fit(self):
        diagnostics = navigable_inset_diagnostics(self.source, self.controls)
        self.assertTrue(diagnostics["ready"])
        self.assertLess(diagnostics["max_error_px"], 0.001)
        self.assertEqual(diagnostics["projection"], "Lambert_Conformal_Conic_2SP")

        # Prove a flat lat/lon fit cannot satisfy this fixture.
        pixels = np.array([point["pixel"] for point in self.controls])
        lonlat = np.array([[point["longitude"], point["latitude"]] for point in self.controls])
        flat = inset_georeference.affine_fit(pixels, lonlat)
        inverse_flat = gdal.InvGeoTransform(flat)
        flat_errors = [np.linalg.norm(np.array(gdal.ApplyGeoTransform(inverse_flat, *ll)) - pixel)
                       for ll, pixel in zip(lonlat, pixels)]
        self.assertGreater(max(flat_errors), 1.0)

        output = self.root / "test.vrt"
        report = inset_georeference.build_inset(self.source, self.inset, output)
        self.assertEqual(report, diagnostics)
        fitted = gdal.Open(str(self.root / "test-source.vrt"))
        np.testing.assert_allclose(fitted.GetGeoTransform()[1:3], self.truth[1:3], atol=1e-7)
        transformer = gdal.Transformer(fitted, None, ["DST_SRS=EPSG:4326"])
        inverse_truth = gdal.InvGeoTransform(self.truth)
        for lon, lat in [(-166.8, 54.1), (-166.2, 53.9)]:
            # Independent coordinates not in the control set, through the build's VRT.
            xy = self.project.TransformPoint(lon, lat)
            pixel = gdal.ApplyGeoTransform(inverse_truth, *xy[:2])
            ok, actual = transformer.TransformPoint(False, pixel[0] - 50, pixel[1] - 50)
            self.assertTrue(ok)
            np.testing.assert_allclose(actual[:2], [lon, lat], atol=1e-8, rtol=0)

        warped = gdal.Open(str(output))
        self.assertEqual(warped.GetSpatialRef().GetAuthorityCode(None), "3857")
        projection = osr.CoordinateTransformation(self.srs, warped.GetSpatialRef())
        inverse_warp = gdal.InvGeoTransform(warped.GetGeoTransform())
        # Test the reprojected triangle, including pixels close to its diagonal edge.
        for x, y, expected in [(200, 200, 200), (1400, 1400, 51),
                               (790, 800, 200), (810, 800, 51)]:
            xy = gdal.ApplyGeoTransform(self.truth, x, y)
            mercator = projection.TransformPoint(*xy)
            px, py = gdal.ApplyGeoTransform(inverse_warp, *mercator[:2])
            value = warped.GetRasterBand(1).ReadAsArray(int(px), int(py), 1, 1)[0, 0]
            self.assertEqual(value, expected, (x, y))

    def test_bad_coordinate_remains_a_visible_large_residual(self):
        self.controls[4]["longitude"] += 0.5
        report = navigable_inset_diagnostics(self.source, self.controls)
        self.assertTrue(report["ready"])
        self.assertGreater(report["max_error_px"], 400)

    def perimeter_controls(self):
        geographic = osr.SpatialReference()
        geographic.ImportFromEPSG(4326)
        geographic.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
        unproject = osr.CoordinateTransformation(self.srs, geographic)
        controls = []
        for edge in (50, 1550):
            for along in (250, 600, 1000, 1350):
                for kind, pixel in [("latitude", [edge, along]), ("longitude", [along, edge])]:
                    lon, lat, _ = unproject.TransformPoint(*gdal.ApplyGeoTransform(self.truth, *pixel))
                    controls.append({"kind": kind, "pixel": pixel,
                                     "latitude": lat if kind == "latitude" else None,
                                     "longitude": lon if kind == "longitude" else None})
        return controls

    def test_perimeter_ticks_predict_interior_curvature_in_production_vrt(self):
        self.inset["control_points"] = self.perimeter_controls()
        output = self.root / "ticks.vrt"
        report = inset_georeference.build_inset(self.source, self.inset, output)
        self.assertEqual(report["constraint_count"], 16)
        self.assertLess(report["max_error_px"], 0.001)
        fitted = gdal.Open(str(self.root / "ticks-source.vrt"))
        transformer = gdal.Transformer(fitted, None, ["DST_SRS=EPSG:4326"])
        inverse_truth = gdal.InvGeoTransform(self.truth)
        # No full intersections entered; these interior coordinates are independent checks.
        for lon in np.linspace(-166.9, -166.1, 5):
            for lat in np.linspace(53.8, 54.2, 5):
                xy = self.project.TransformPoint(float(lon), float(lat))
                pixel = gdal.ApplyGeoTransform(inverse_truth, *xy[:2])
                ok, actual = transformer.TransformPoint(False, pixel[0] - 50, pixel[1] - 50)
                self.assertTrue(ok)
                np.testing.assert_allclose(actual[:2], [lon, lat], atol=1e-8, rtol=0)

    def test_mixed_controls_and_invalid_constraints(self):
        controls = self.perimeter_controls() + self.controls[:2]
        report = navigable_inset_diagnostics(self.source, controls)
        self.assertTrue(report["ready"], report)
        self.assertLess(report["max_error_px"], 0.001)
        controls[0]["latitude"] += .1
        report = navigable_inset_diagnostics(self.source, controls)
        self.assertTrue(report["ready"], report)
        self.assertGreater(report["point_errors_px"][0], 100)
        controls[0]["longitude"] = -166
        report = navigable_inset_diagnostics(self.source, controls)
        self.assertFalse(report["ready"])
        self.assertIn("must not supply longitude", report["summary"])

    def test_ticks_need_both_coordinates_and_spatial_spread(self):
        controls = self.perimeter_controls()
        report = navigable_inset_diagnostics(self.source, [p for p in controls if p["kind"] == "latitude"])
        self.assertFalse(report["ready"])
        self.assertIn("longitude observations", report["summary"])
        for point in controls:
            point["pixel"][0] = 50 if point["kind"] == "latitude" else 1550
        report = navigable_inset_diagnostics(self.source, controls)
        self.assertFalse(report["ready"])
        self.assertIn("spatial spread", report["summary"])

    def test_missing_projection_fails_explicitly(self):
        source = gdal.Open(str(self.source), gdal.GA_Update)
        source.SetProjection("")
        source = None
        report = navigable_inset_diagnostics(self.source, self.controls)
        self.assertFalse(report["ready"])
        self.assertIn("projected coordinate system", report["summary"])
        with self.assertRaisesRegex(ValueError, "projected coordinate system"):
            inset_georeference.build_inset(self.source, self.inset, self.root / "bad.vrt")

    def test_noisy_juneau_marks_converge_without_demanding_zero_residual(self):
        # Captured clicks: the least-squares optimum retains real placement error.
        # A fixed 10-micrometre coefficient tolerance stalled on a withheld point.
        srs = osr.SpatialReference()
        srs.ImportFromProj4("+proj=lcc +lat_0=58 +lon_0=-135.45 +lat_1=62.6666666666667 "
                           "+lat_2=57.3333333333333 +datum=NAD83 +units=m")
        source_path = self.root / "Noisy Juneau.tif"
        source = gdal.GetDriverByName("GTiff").Create(str(source_path), 4000, 5500, 1)
        source.SetProjection(srs.ExportToWkt())
        source = None
        controls = [
            {"kind": "intersection", "pixel": [x, y], "latitude": lat, "longitude": lon}
            for x, y, lat, lon in [
                (2298.5, 4973.5, 58.333333333, -134.75),
                (2294, 4535.5, 58.416666667, -134.75),
                (2990, 4965, 58.333333333, -134.5),
                (2983.5, 4526.5, 58.416666667, -134.5),
                (2303.5, 5411.5, 58.25, -134.75),
                (2996.5, 5403, 58.25, -134.5),
                (3689.25, 5392.5, 58.25, -134.25),
                (3674, 4516, 58.416666667, -134.25),
                (3681.5, 4954.25, 58.333333333, -134.25),
            ]
        ]
        report = navigable_inset_diagnostics(source_path, controls)
        self.assertTrue(report["ready"], report)
        self.assertAlmostEqual(report["max_error_px"], .743, delta=.002)


class SnapTest(unittest.TestCase):
    def test_finds_white_margin_corner(self) -> None:
        image = np.full((256, 256, 3), 255, dtype=np.uint8)
        image[92:, 78:, :] = (198, 214, 224)
        image[90:94, 78:, :] = 22
        image[92:, 76:80, :] = 22
        result = find_snap_candidate(image, (69.0, 103.0), 80)
        self.assertIsNotNone(result)
        assert result is not None
        self.assertAlmostEqual(result[0], 78.0, delta=3.0)
        self.assertAlmostEqual(result[1], 92.0, delta=3.0)
        self.assertGreater(result[2], 0.1)



def assert_points_close(
    test: unittest.TestCase,
    actual: object,
    expected: list[list[float]],
) -> None:
    test.assertIsInstance(actual, list)
    assert isinstance(actual, list)
    test.assertEqual(len(actual), len(expected))
    for actual_point, expected_point in zip(actual, expected):
        test.assertAlmostEqual(actual_point[0], expected_point[0], places=5)
        test.assertAlmostEqual(actual_point[1], expected_point[1], places=5)


if __name__ == "__main__":
    unittest.main()
