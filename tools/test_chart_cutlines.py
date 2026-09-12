# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Hermetic regressions for editing -> saved CRS -> actual GDAL warp geometry."""

import copy
import json
import math
import subprocess
from pathlib import Path
import tempfile
import unittest
import xml.etree.ElementTree as ET

import numpy as np
from osgeo import gdal, ogr

from tools.chart_cutline_editor import EditorState, cutlines


class CutlineProjectionTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source_path = self.root / "Curved.tif"
        self.cutline_dir = self.root / "SEC"
        self.cutline_dir.mkdir()
        self.path = self.cutline_dir / "Curved.geojson"
        self.srs = cutlines.spatial_reference(
            "+proj=lcc +lat_1=33 +lat_2=45 +lat_0=39 +lon_0=-96 +datum=WGS84 +units=m +no_defs"
        )
        self.gt = (-2400000, 2000, 0, 1600000, 0, -2000)
        self.source = gdal.GetDriverByName("GTiff").Create(str(self.source_path), 2400, 1600, 1)
        self.source.SetGeoTransform(self.gt)
        self.source.SetProjection(self.srs.ExportToWkt())
        self.source.GetRasterBand(1).Fill(123)
        self.source.FlushCache()
        self.addCleanup(lambda: setattr(self, "source", None))
        self.points = [(100., 100.), (2200., 100.), (2200., 1450.), (100., 1450.)]
        self.document = {"type": "FeatureCollection", "crs": {"type": "name", "properties": {"name": "EPSG:3857"}},
                         "features": [{"type": "Feature", "properties": {"operator_note": "keep me"},
                                       "geometry": {"type": "Polygon", "coordinates": [[]]}}]}

    def write(self, document):
        self.path.write_text(json.dumps(document))

    def dense_oracle(self, points, per_edge=2000):
        # Independent dense sampling, not the adaptive subdivision under test.
        project = cutlines.coordinate_map(self.srs, cutlines.spatial_reference("EPSG:3857"))
        coordinates = []
        for a, b in zip(points, points[1:] + [points[0]]):
            for i in range(per_edge):
                p = (a[0] + (b[0] - a[0]) * i / per_edge, a[1] + (b[1] - a[1]) * i / per_edge)
                coordinates.append(project(gdal.ApplyGeoTransform(self.gt, *p)))
        coordinates.append(coordinates[0])
        return coordinates

    def dense_input(self):
        document = copy.deepcopy(self.document)
        document["features"][0]["geometry"]["coordinates"] = [self.dense_oracle(self.points)]
        self.write(document)

    def warp(self, path, name):
        warp = gdal.Warp(str(self.root / (name + ".vrt")), self.source, format="VRT",
                         dstSRS="EPSG:3857", resampleAlg="cubic", dstNodata=51,
                         cutlineDSName=str(path), cropToCutline=True)
        geometry = ogr.CreateGeometryFromWkt(ET.fromstring(warp.GetMetadata("xml:VRT")[0]).find(".//Cutline").text)
        return warp, geometry

    def assert_boundary_matches(self, geometry, points):
        boundary = geometry.Boundary()
        for a, b in zip(points, points[1:] + [points[0]]):
            for i in range(101):
                point = ogr.Geometry(ogr.wkbPoint)
                point.AddPoint_2D(a[0] + (b[0] - a[0]) * i / 100, a[1] + (b[1] - a[1]) * i / 100)
                self.assertLess(point.Distance(boundary), 0.05)

    def test_edit_save_prepare_and_warp_preserve_visible_edges_and_entire_crop(self):
        self.dense_input()
        before = self.path.read_bytes()
        editor = EditorState(self.root, self.cutline_dir, self.root / "cache", 600)
        payload = editor.chart_payload("Curved")
        self.assertEqual(len(payload["points"]), 4)
        self.assertEqual(before, self.path.read_bytes(), "opening must not rewrite metadata or approvals")
        np.testing.assert_allclose(payload["points"], self.points, atol=0.001)
        self.points[1] = (2185., 120.)
        editor.save_points("Curved", [list(p) for p in self.points], payload["revision"])
        saved, srs, _ = cutlines.read(self.path)
        self.assertTrue(srs.IsSame(self.srs))
        self.assertEqual(saved["features"][0]["properties"], {"operator_note": "keep me"})
        self.assertEqual(len(saved["features"][0]["geometry"]["coordinates"][0]), 5)
        np.testing.assert_allclose(editor.chart_payload("Curved")["points"], self.points, atol=1e-6)
        self.assert_boundary_matches(cutlines.pixel_geometry(self.path, self.source), self.points)

        prepared = self.root / "derived.geojson"
        cutlines.prepare(self.path, self.source, prepared)
        warp, actual = self.warp(prepared, "derived")
        self.assert_boundary_matches(actual, self.points)
        gt = warp.GetGeoTransform()
        bounds = (gt[0], gt[3] + gt[5] * warp.RasterYSize,
                  gt[0] + gt[1] * warp.RasterXSize, gt[3])
        oracle = self.dense_oracle(self.points)
        expected = (min(x for x, _ in oracle), min(y for _, y in oracle),
                    max(x for x, _ in oracle), max(y for _, y in oracle))
        np.testing.assert_allclose(bounds, expected, atol=100)

        # Reading raster data exercises the clipping mask, not only VRT metadata.
        pixels = warp.ReadAsArray()
        self.assertGreater(np.count_nonzero(pixels == 123), pixels.size // 2)
        self.assertGreater(np.count_nonzero(pixels == 51), 0)
        coverage = cutlines.coverage(self.path)
        self.assertAlmostEqual(coverage["lon_min"], math.degrees(expected[0] / 6378137), places=5)
        self.assertAlmostEqual(coverage["lon_max"], math.degrees(expected[2] / 6378137), places=5)

    def test_four_corners_in_mercator_reproduces_the_old_visible_shape_trap(self):
        self.document["features"][0]["geometry"]["coordinates"] = [self.dense_oracle(self.points, per_edge=1)]
        self.write(self.document)
        _, actual = self.warp(self.path, "wrong")
        point = ogr.Geometry(ogr.wkbPoint)
        point.AddPoint_2D(1150, 100)
        self.assertGreater(point.Distance(actual.Boundary()), 100)

    def test_registered_faa_detail_masks_printed_copy_without_masking_detail(self):
        self.write(cutlines.native_document(self.document, self.source, self.points))
        detail_path = self.root / 'Detail.tif'
        detail = gdal.GetDriverByName('GTiff').Create(str(detail_path), 200, 200, 1)
        detail.SetProjection(self.srs.ExportToWkt())
        detail.SetGeoTransform((100000, 100, 0, 200000, 0, -100))
        detail.GetRasterBand(1).Fill(211)
        detail.FlushCache()
        definition = cutlines.native_document(self.document, detail, [(0, 0), (200, 0), (200, 200), (0, 200)])
        definition['source_sheet'] = dict(schema_version=1, source='Curved.tif',
            source_width=2400, source_height=1600, detail_width=200, detail_height=200,
            pixel_transform=[500, 1, 0, 600, 0, 1])
        child = self.cutline_dir / 'Detail.geojson'
        child.write_text(json.dumps(definition))
        prepared = self.root / 'parent.geojson'
        cutlines.prepare(self.path, self.source, prepared)
        warp, geometry = self.warp(prepared, 'parent')
        for x, y, expected in [(600, 700, 51), (400, 700, 123)]:
            world = cutlines.coordinate_map(self.srs, cutlines.spatial_reference('EPSG:3857'))(
                gdal.ApplyGeoTransform(self.gt, x, y))
            px, py = cutlines.source_pixel_map(warp)(world)
            self.assertEqual(int(warp.ReadAsArray(int(px), int(py), 1, 1)[0, 0]), expected)
        cutlines.prepare(child, detail, self.root / 'detail.geojson')
        output = gdal.Warp(str(self.root / 'detail.vrt'), detail, format='VRT', dstSRS='EPSG:3857',
                           cutlineDSName=str(self.root / 'detail.geojson'), cropToCutline=True, dstNodata=51)
        self.assertGreater(np.count_nonzero(output.ReadAsArray() == 211), output.RasterXSize * output.RasterYSize * .9)
        self.assertEqual(int(self.source.ReadAsArray(600, 700, 1, 1)[0, 0]), 123)
        _, _, authored = cutlines.read(self.path)
        self.assertEqual(authored.GetGeometryRef(0).GetGeometryCount(), 1, 'authored outline remains editable without holes')

    def test_sparse_stored_curve_keeps_four_handles_and_preview_matches_saved_warp(self):
        self.document["features"][0]["geometry"]["coordinates"] = [self.dense_oracle(self.points, per_edge=1)]
        self.write(self.document)
        editor = EditorState(self.root, self.cutline_dir, self.root / "cache", 600)
        payload = editor.chart_payload("Curved")
        self.assertEqual(len(payload["points"]), 4)
        self.assertGreater(len(payload["outline"]), 40)
        points = [list(p) for p in payload["points"]]
        points[0][0] += 5
        preview = editor.preview_points("Curved", points, payload["revision"])
        editor.save_points("Curved", points, payload["revision"])
        self.assertTrue(cutlines.read(self.path)[1].IsSame(cutlines.spatial_reference("EPSG:3857")))
        _, warped = self.warp(self.path, "edited-curve")
        for x, y in preview["outline"]:
            point = ogr.Geometry(ogr.wkbPoint); point.AddPoint_2D(x, y)
            self.assertLess(point.Distance(warped.Boundary()), 0.025)
        with self.assertRaisesRegex(RuntimeError, "changed"):
            editor.preview_points("Curved", points, payload["revision"])

    def test_preview_requests_coalesce_and_ignore_old_chart_responses(self):
        subprocess.run(["node", str(Path(__file__).with_name("test_cutline_preview.cjs"))], check=True)

    def test_native_corners_alone_do_not_define_warp_crop_extrema(self):
        # ENR_H03's rotated source-plane geometry, without any FAA raster fixture.
        # A corner-only crop underestimates the extremum of its curved top edge.
        self.srs = cutlines.spatial_reference(
            "+proj=lcc +lat_1=45 +lat_2=33 +lat_0=39 +lon_0=-95 +datum=NAD83 +units=m +no_defs"
        )
        self.gt = (-2943495.28, 91.6761117, -13.0080829, 889253.689, -13.0200458, -91.6831876)
        self.source = gdal.GetDriverByName("GTiff").Create(str(self.root / "rotated.tif"), 24000, 8000, 1,
                                                           options=["TILED=YES", "SPARSE_OK=YES"])
        self.source.SetProjection(self.srs.ExportToWkt())
        self.source.SetGeoTransform(self.gt)
        self.points = [(2207.47460372572, 101.096888456947), (23899.7651863517, 101.03351319844),
                       (23898.6742257691, 7798.61896002224), (2206.38364314562, 7798.68233528053)]
        self.write(cutlines.native_document(self.document, self.source, self.points))
        naive, _ = self.warp(self.path, "corner-only")
        oracle = self.dense_oracle(self.points)
        expected_top = max(y for _, y in oracle)
        self.assertGreater(expected_top - naive.GetGeoTransform()[3], 40)
        prepared = self.root / "derived.geojson"
        cutlines.prepare(self.path, self.source, prepared)
        correct, _ = self.warp(prepared, "complete")
        self.assertLess(abs(correct.GetGeoTransform()[3] - expected_top), 3)

    def test_future_source_placement_does_not_move_the_saved_geographic_boundary(self):
        self.write(cutlines.native_document(self.document, self.source, self.points))
        old = cutlines.pixel_geometry(self.path, self.source)
        self.source.SetGeoTransform((self.gt[0] + 10000, *self.gt[1:]))
        new = cutlines.pixel_geometry(self.path, self.source)
        self.assertAlmostEqual(new.GetEnvelope()[0], old.GetEnvelope()[0] - 5)

    def test_dateline_chart_renders_both_sides_without_a_world_sized_crop(self):
        self.srs = cutlines.spatial_reference(
            "+proj=lcc +lat_1=0 +lat_2=46 +lat_0=23 +lon_0=180 +datum=WGS84 +units=m +no_defs")
        self.gt = (-1200000, 1000, 0, 800000, 0, -1000)
        self.source.SetGeoTransform(self.gt)
        self.source.SetProjection(self.srs.ExportToWkt())
        self.source.FlushCache()
        self.points = [(100, 100), (2300, 100), (2300, 1500), (100, 1500)]
        self.write(cutlines.native_document(self.document, self.source, self.points))
        prepared = cutlines.prepare_parts(self.path, self.source, self.root / 'seam.geojson')
        self.assertEqual(len(prepared), 2)
        signs = set()
        for i, part in enumerate(prepared):
            warp = gdal.Warp(str(self.root / f'seam-{i}.vrt'), self.source, format='VRT',
                             dstSRS='EPSG:3857', resampleAlg='cubic', dstNodata=51,
                             cutlineDSName=part['cutline'], outputBounds=part['bounds'])
            self.assertLess(warp.RasterXSize, 2000)
            image = warp.ReadAsArray()
            self.assertGreater(np.count_nonzero(image == 123), image.size // 2)
            signs.add(math.copysign(1, warp.GetGeoTransform()[0]))
        self.assertEqual(signs, {-1, 1})
        for ring in cutlines.geographic_exteriors(self.cutline_dir):
            self.assertLess(max(p[0] for p in ring) - min(p[0] for p in ring), 20)

    def test_holes_and_multipolygons_survive_preparation_but_not_unsupported_editing(self):
        document = cutlines.native_document(self.document, self.source, self.points)
        coords = document["features"][0]["geometry"]["coordinates"]
        hole = [(800, 600), (1000, 600), (1000, 800), (800, 800), (800, 600)]
        coords.append([gdal.ApplyGeoTransform(self.gt, *p) for p in hole])
        extra = [gdal.ApplyGeoTransform(self.gt, *p) for p in [(5, 5), (30, 5), (30, 30), (5, 30), (5, 5)]]
        document["features"][0]["geometry"] = {"type": "MultiPolygon", "coordinates": [coords, [extra]]}
        self.write(document)
        prepared = self.root / "derived.geojson"
        cutlines.prepare(self.path, self.source, prepared)
        _, _, geometry = cutlines.read(prepared)
        self.assertEqual(geometry.GetGeometryCount(), 2)
        self.assertEqual(geometry.GetGeometryRef(0).GetGeometryCount(), 2)
        with self.assertRaisesRegex(ValueError, "one polygon without holes"):
            cutlines.editable_points(self.path, self.source)

    def test_missing_crs_and_invalid_topology_are_rejected_not_guessed_or_repaired(self):
        self.dense_input()
        document = json.loads(self.path.read_text())
        del document["crs"]
        self.write(document)
        with self.assertRaisesRegex(ValueError, "explicit"):
            cutlines.read(self.path)
        with self.assertRaisesRegex(ValueError, "topology"):
            cutlines.native_document(self.document, self.source, [(0, 0), (10, 10), (0, 10), (10, 0)])

    def test_corner_preserving_simplification_is_bounded_and_idempotent(self):
        original = [(0., 0.), (20., 0.), (20., 20.), (12., 20.),
                    (12., 8.), (8., 8.), (8., 20.), (0., 20.)]
        dense = [cutlines.interpolate(a, b, i / 200) for a, b in zip(original, original[1:] + [original[0]])
                 for i in range(200)]
        simplified = cutlines.simplified_ring(dense)
        self.assertEqual(simplified, original)
        self.assertEqual(cutlines.simplified_ring(simplified), original)
        for p in dense:
            self.assertLessEqual(min(cutlines.segment_distance(p, a, b)
                                     for a, b in zip(simplified, simplified[1:] + [simplified[0]])), 0.2)
        # A small notch is protected even though its removal is under the error tolerance.
        tiny = [(0, 0), (10, 0), (10, 10), (5.1, 10), (5, 9.9), (4.9, 10), (0, 10)]
        self.assertEqual(cutlines.simplified_ring(tiny), tiny)

    def test_mercator_dateline_branch_is_preserved_without_longitude_wrapping(self):
        document = copy.deepcopy(self.document)
        left = 20037508
        document["features"][0]["geometry"]["coordinates"] = [[
            [left, 1000], [left + 10000, 1000], [left + 10000, 0], [left, 0], [left, 1000],
        ]]
        self.write(document)
        self.source.SetProjection(cutlines.spatial_reference("EPSG:3857").ExportToWkt())
        self.source.SetGeoTransform((left, 10, 0, 1000, 0, -10))
        prepared = self.root / "derived.geojson"
        cutlines.prepare(self.path, self.source, prepared)
        self.assertGreater(cutlines.coverage(prepared)["lon_max"], 180)
        self.assertEqual(cutlines.pixel_geometry(prepared, self.source).GetEnvelope(), (0, 1000, 0, 100))


if __name__ == "__main__":
    unittest.main()
