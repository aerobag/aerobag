from __future__ import annotations

import json
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
    )
except ImportError:
    from tools.chart_cutline_editor import (
        EditorCatalog,
        EditorState,
        RevisionConflict,
        file_revision,
        find_snap_candidate,
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
