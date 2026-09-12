# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import base64
import gzip
import http.client
import json
from pathlib import Path
import shutil
import subprocess
import threading
import xml.etree.ElementTree as ET
from unittest.mock import patch

import numpy as np

from tools import chart_visual_review as review
from tools.test_chart_visual_references import ChartFixture, quality


def test_prefetch_scheduler_contract():
    subprocess.run(["node", str(Path(__file__).with_name("test-chart-review-prefetch.mjs"))], check=True, timeout=10)


class ChartVisualReviewTests(ChartFixture):
    def test_faa_details_are_reviewed_on_parent_without_losing_operator_annotations(self):
        import shutil
        self.check()
        self.store = self.open_store()
        parent = self.store.snapshot()['sheets'][0]
        self.store.sheet_decision({**parent, 'verdict':'incomplete', 'note':'Missing detail map'})
        detail = self.metadata / 'SEC/Detail.geojson'
        document = json.loads(self.cutline.read_text())
        document['source_sheet'] = {'schema_version':1, 'source':'Test SEC.tif',
            'source_width':1000, 'source_height':800, 'detail_width':1000, 'detail_height':800,
            'pixel_transform':[0,1,0,0,0,1], 'note':'Use FAA georeference; review cutline.'}
        detail.write_text(json.dumps(document))
        shutil.copy(self.sources/'Test SEC.tif', self.sources/'Detail.tif')
        state = self.store.snapshot()
        self.assertEqual(len(state['sheets']), 1, 'detail belongs to printed parent, not a second independent sheet')
        parent = state['sheets'][0]
        self.assertEqual(parent['decision']['note'], 'Missing detail map')
        self.assertEqual(parent['decision']['verdict'], 'incomplete')
        self.assertEqual(parent['status'], 'stale')
        region = next(r for r in parent['regions'] if r['kind']=='detail')
        self.assertIn('no manual georef', region['name'])
        rendered = self.store.inventory.render(parent)
        self.assertIn(region['id'], [r['id'] for r in rendered['regions']])
        refreshed = self.store.refresh_sheet(parent)
        candidate = next(i for i in refreshed['items'] if i['id']==region['id'])
        self.assertEqual(candidate['status'], 'pending')
        self.assertEqual(refreshed['counts']['approved'], 0)
        self.assertEqual(refreshed['sheets'][0]['decision']['note'], 'Missing detail map')

    def setUp(self):
        super().setUp()
        self.check()
        self.state_dir = self.root / "durable-review"
        self.store = self.open_store()

    def open_store(self):
        return review.ReviewStore(self.output.parent, self.state_dir, self.metadata, "test reviewer",
                                  {"SEC": self.sources, "TAC": self.sources}, "http://localhost:8093")

    def main_item(self):
        return next(i for i in self.store.snapshot()["items"] if i["kind"] == "cutline")

    def decision(self, item, verdict="approved", note=""):
        return self.store.decide({"id": item["id"], "digest": item["digest"], "verdict": verdict, "note": note})

    def test_approval_is_the_shared_repo_reference_and_survives_restart(self):
        item = self.main_item()
        self.decision(item)
        path = quality.reference_path(self.metadata, item["family"], item["id"])
        reference = json.loads(path.read_text())
        self.assertEqual(reference["approval"]["digest"], item["digest"])
        self.assertEqual(reference["approval"]["reviewed_by"], "test reviewer")
        self.assertEqual(self.open_store().snapshot()["counts"]["approved"], 1)
        before = path.read_bytes()
        self.decision(item)
        self.assertEqual(path.read_bytes(), before, "repeat requests must not rewrite an approval timestamp")

    def test_rejection_notes_and_pinned_images_survive_report_gc_and_restart(self):
        item = self.main_item()
        self.decision(item, "disapproved", "legend leaked at north edge")
        self.store.set_cursor(item["id"])
        shutil.rmtree(self.output / "reports")
        # Unchanged report pointers do not need their now-GC'd contents.
        reopened = self.open_store()
        self.assertEqual(reopened.snapshot()["cursor"], item["id"])
        self.assertEqual(reopened.rejections()[0]["decision"]["note"], "legend leaked at north edge")
        self.assertIn("family=SEC&chart=Test+SEC", reopened.rejections()[0]["editor_url"])
        self.assertTrue((reopened.candidate_path(item).parent / "review-overview.png").exists())
        self.assertFalse(quality.reference_path(self.metadata, "SEC", item["id"]).exists())

    def test_rejecting_accidental_approval_removes_only_that_reference(self):
        item = self.main_item()
        self.decision(item)
        self.decision(item, "disapproved")
        self.assertFalse(quality.reference_path(self.metadata, "SEC", item["id"]).exists())
        self.decision(item)
        path = quality.reference_path(self.metadata, "SEC", item["id"])
        approved = path.read_bytes()
        changed = self.original.copy()
        changed[:, 400:420, 450:480] = 20
        self.write_source(changed)
        self.check("2611")
        self.store.import_reports()
        self.decision(self.main_item(), "disapproved")
        self.assertEqual(path.read_bytes(), approved, "rejecting a newer candidate preserves the old trusted reference")

    def test_stale_tab_and_changed_metadata_cannot_approve(self):
        item = self.main_item()
        with self.assertRaisesRegex(review.ReviewError, "displayed candidate"):
            self.store.decide({"id": item["id"], "digest": "0" * 64, "verdict": "approved"})
        geojson = json.loads(self.cutline.read_text())
        geojson["features"][0]["geometry"]["coordinates"][0][0][0] += 10
        self.cutline.write_text(json.dumps(geojson))
        self.assertEqual(self.main_item()["status"], "stale")
        self.assertTrue(self.main_item()["image"].endswith("review-overview.png"))
        with self.assertRaisesRegex(review.ReviewError, "refresh"):
            self.store.projected_overview(item)
        with self.assertRaisesRegex(review.ReviewError, "refresh"):
            self.decision(item)

    def test_old_pinned_straight_mask_gets_editor_curves_without_changing_approval(self):
        from tools.test_chart_cutlines import CutlineProjectionTest
        from tools.chart_cutline_editor import EditorState

        fixture = CutlineProjectionTest()
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        fixture.document["features"][0]["geometry"]["coordinates"] = [fixture.dense_oracle(fixture.points, per_edge=1)]
        fixture.write(fixture.document)
        colors = quality.gdal.ColorTable()
        colors.SetColorEntry(123, (220, 225, 200, 255))
        fixture.source.GetRasterBand(1).SetColorTable(colors)
        fixture.source.FlushCache()
        output = fixture.root / "reports/SEC"
        # Reproduce a report created by the old vertex-only projector. Restarting
        # the server must not keep drawing this pinned straight-edged mask.
        with patch.object(quality, "pixel_geometry", return_value=quality.inset_geometry(fixture.points)):
            report = quality.run_check(fixture.root, fixture.root, "SEC", output, "old-renderer", "2610")
        self.assertEqual(report["critical_count"], 0, report)
        store = review.ReviewStore(output.parent, fixture.root / "review", fixture.root, "operator")
        item = store.snapshot()["items"][0]
        store.decide({**item, "verdict": "approved"})
        store.set_cursor(item["id"])
        editor = EditorState(fixture.root, fixture.cutline_dir, fixture.root / "cache", 600)
        expected = editor.chart_payload("Curved")["outline"]
        self.assertGreater(len(expected), 40)
        candidate_path = store.candidate_path(item)
        reference_path = quality.reference_path(fixture.root, "SEC", item["id"])
        paths = [candidate_path, reference_path, store.state_path]
        original = {path: path.read_bytes() for path in paths}
        candidate = json.loads(candidate_path.read_text())
        view = candidate["views"][0]
        correct_mask = quality.region_mask(quality.cutlines.pixel_document(fixture.document, fixture.source),
                                           view["window"], view["size"])
        self.assertGreater(np.count_nonzero(correct_mask != quality.decode_png(view["mask"])), 100)

        fixture.source_path.unlink()  # Drawing uses the pinned projection, not the FAA file.
        svg = ET.fromstring(store.projected_overview(item))
        line = svg.find("{http://www.w3.org/2000/svg}polyline")
        actual = [[float(v) for v in point.split(",")] for point in line.attrib["points"].split()]
        np.testing.assert_allclose(actual, expected, atol=0.0001)
        image = svg.find("{http://www.w3.org/2000/svg}image")
        self.assertEqual(image.attrib["href"], "data:image/png;base64," + view["pixels"])
        self.assertTrue(store.snapshot()["items"][0]["image"].endswith(".svg"))
        self.assertEqual(store.snapshot()["items"][0]["status"], "approved")
        self.assertEqual(original, {path: path.read_bytes() for path in paths})

    def test_edited_rejection_rerenders_current_cutline_and_requires_new_approval(self):
        item = self.main_item()
        self.decision(item, "disapproved", "trim the edge")
        geojson = json.loads(self.cutline.read_text())
        coordinates = geojson["features"][0]["geometry"]["coordinates"][0]
        for point in coordinates:
            if point[0] == 100:
                point[0] = 250
        self.cutline.write_text(json.dumps(geojson))
        before = (self.store.candidate_path(item).parent / "review-overview.png").read_bytes()
        self.store.refresh({"id": item["id"], "digest": item["digest"]})
        refreshed = self.main_item()
        self.assertNotEqual(refreshed["digest"], item["digest"])
        self.assertEqual(refreshed["status"], "recheck")
        self.assertNotEqual((self.store.candidate_path(refreshed).parent / "review-overview.png").read_bytes(), before)
        self.store = self.open_store()
        self.assertEqual(self.main_item()["digest"], refreshed["digest"], "restart must not reimport the obsolete report")
        self.decision(refreshed)
        self.assertEqual(self.main_item()["status"], "approved")
        self.assertEqual(self.store.rejections(), [])

    def test_insets_appear_once_despite_both_parent_and_destination_reports(self):
        (self.metadata / "TAC").mkdir()
        quality.run_check(self.sources, self.metadata, "TAC", self.output.parent / "TAC", "tac+sec", "2610")
        self.store.import_reports()
        self.assertEqual(len(self.store.snapshot()["items"]), 2)
        self.assertEqual(len(self.store.snapshot()["sheets"]), 1)

    def add_inventory_regions(self):
        document = json.loads(self.inset_file.read_text())
        document["insets"].append({**document["insets"][0], "id": "Draft", "enabled": False})
        self.inset_file.write_text(json.dumps(document))
        for kind, rectangles in [("inset", [{"x": 120, "y": 120, "width": 40, "height": 40}]),
                                 ("legend", [{"x": 10, "y": 10, "width": 50, "height": 50}])]:
            (self.metadata / "SEC" / f"Test SEC.{kind}.json").write_text(json.dumps({
                "schema_version": 1, "source": "Test SEC.tif", "source_width": 1000,
                "source_height": 800, "regions": rectangles}))

    def test_exclude_only_is_labelled_as_no_output_but_remains_reviewable(self):
        document = json.loads(self.inset_file.read_text())
        document['insets'][0]['target_family'] = 'EXCLUDE'
        document['insets'][0]['control_points'] = []
        document['insets'][0].pop('projection_wkt')
        self.inset_file.write_text(json.dumps(document))
        parent = self.store.snapshot()['sheets'][0]
        region = next(r for r in parent['regions'] if r['kind'] == 'inset')
        self.assertIn('Exclude only (no output layer)', region['name'])
        self.assertIn('type=navigable-inset', region['editor_url'])
        self.store.refresh_sheet(parent)
        refreshed = self.store.snapshot()['sheets'][0]
        region = next(r for r in refreshed['regions'] if r['id'] == region['id'])
        self.assertIsNotNone(region['review_id'])
        rendered = self.store.inventory.render(refreshed)
        shape = next(r for r in rendered['regions'] if r['id'] == region['id'])
        self.assertIn('explicit navigable-inset mask', shape['exclusion'])

    def test_whole_sheet_includes_all_definitions_and_distinguishes_extraction_from_exclusion(self):
        self.add_inventory_regions()
        snapshot = self.store.snapshot()
        self.assertEqual(len(snapshot["sheets"]), 1)
        parent = snapshot["sheets"][0]
        self.assertEqual([r["kind"] for r in parent["regions"]], ["cutline", "inset", "draft", "reference", "legend"])
        self.assertEqual(sum(r["review_id"] is not None for r in parent["regions"]), 2)
        actual = self.store.inventory.render(parent)
        self.assertEqual((actual["width"], actual["height"]), (1000, 800))
        image = review.quality.decode_png(base64.b64encode(
            (self.state_dir / actual["image"].lstrip("/")).read_bytes()).decode())
        np.testing.assert_array_equal(image, self.original, "do not crop source to the main region")
        captions = [r["exclusion"] for r in actual["regions"]]
        self.assertIn("explicit navigable-inset mask", captions[1])
        self.assertIn("100% overlaps", captions[3], "an ordinary extracted inset is NOT a parent exclusion")
        self.assertIn("outside its retained area", captions[4])
        self.assertIn("type=navigable-inset", parent["regions"][1]["editor_url"])
        self.assertIn("type=inset", parent["regions"][3]["editor_url"])

    def test_sheet_completeness_is_independent_durable_and_invalidated_by_any_definition_or_source_change(self):
        self.decision(self.main_item())
        self.add_inventory_regions()
        parent = self.store.snapshot()["sheets"][0]
        before = dict(self.store.state["items"])
        self.assertEqual(parent["status"], "pending", "main approval must not bless undiscovered insets")
        self.store.sheet_decision({**parent, "verdict": "complete", "note": "inspected every corner"})
        self.assertEqual(before, self.store.state["items"])
        reopened = self.open_store()
        self.assertEqual(reopened.snapshot()["sheets"][0]["status"], "complete")
        self.assertEqual(reopened.snapshot()["sheets"][0]["decision"]["note"], "inspected every corner")
        layout = self.metadata / "SEC/Test SEC.legend.json"
        document = json.loads(layout.read_text())
        document["regions"][0]["x"] += 1
        layout.write_text(json.dumps(document))
        self.assertEqual(reopened.snapshot()["sheets"][0]["status"], "stale")
        with self.assertRaisesRegex(review.ReviewError, "changed"):
            reopened.sheet_decision({**parent, "verdict": "complete"})
        changed = reopened.snapshot()["sheets"][0]
        reopened.sheet_decision({**changed, "verdict": "complete"})
        self.write_source(self.original)
        self.assertEqual(reopened.snapshot()["sheets"][0]["status"], "stale")

    def test_sheet_summary_tracks_region_approvals_separately_from_inventory(self):
        self.add_inventory_regions()
        self.decision(self.main_item())
        parent = self.store.snapshot()["sheets"][0]
        self.assertEqual(parent["review_summary"], [
            {"label": "Regions: 1/2 approved; 1 not reviewed", "tone": "pending"},
            {"label": "Inset inventory: Not reviewed", "tone": "pending"}])
        inset = next(i for i in self.store.snapshot()["items"] if i["kind"] == "inset")
        self.decision(inset, "disapproved")
        self.assertEqual(self.store.snapshot()["sheets"][0]["review_summary"][0],
                         {"label": "Regions: 1/2 approved; 1 disapproved", "tone": "disapproved"})
        self.decision(inset)
        self.assertEqual(self.open_store().snapshot()["sheets"][0]["review_summary"], [
            {"label": "Regions: Approved (2/2)", "tone": "approved"},
            {"label": "Inset inventory: Not reviewed", "tone": "pending"}])
        self.store.sheet_decision({**parent, "verdict": "incomplete"})
        self.assertEqual(self.store.snapshot()["sheets"][0]["review_summary"], [
            {"label": "Regions: Approved (2/2)", "tone": "approved"},
            {"label": "Inset inventory: Missing / unresolved regions", "tone": "incomplete"}])

    def test_sheet_summary_never_calls_inventory_only_or_stale_regions_approved(self):
        self.assertEqual(review.sheet_review_summary([
            {"review_id": None, "status": "inventory"}], "complete")[0],
            {"label": "Regions: No reference candidates", "tone": "inventory"})
        for status in ("stale", "recheck"):
            with self.subTest(status=status):
                summary = review.sheet_review_summary([
                    {"review_id": "main", "status": "approved"},
                    {"review_id": "inset", "status": status}], "complete")
                self.assertEqual(summary[0]["tone"], "stale")
                self.assertIn("1/2 approved", summary[0]["label"])

    def test_sheet_flags_and_region_rejections_remain_separate_and_cursor_migrates(self):
        item = self.main_item()
        self.decision(item, "disapproved", "trim main boundary")
        parent = self.store.snapshot()["sheets"][0]
        self.assertEqual(self.store.snapshot()["sheet_cursor"], parent["id"])
        self.store.sheet_decision({**parent, "verdict": "incomplete", "note": "unoutlined airport at southeast"})
        self.decision(item)
        reopened = self.open_store()
        self.assertEqual(reopened.snapshot()["sheets"][0]["status"], "incomplete")
        self.assertTrue(reopened.snapshot()["sheets"][0]["has_rejection"])
        self.assertEqual(reopened.rejections(), [])
        inset = next(r for r in parent["regions"] if r["kind"] == "inset")
        reopened.sheet_cursor({"id": parent["id"], "region_id": inset["id"]})
        self.assertEqual(self.open_store().snapshot()["cursor"], inset["id"])
        with self.assertRaisesRegex(review.ReviewError, "belong"):
            reopened.sheet_cursor({"id": parent["id"], "region_id": "unknown"})

    def test_sheet_overview_refuses_dimension_mismatch_instead_of_scaling_outlines(self):
        document = json.loads(self.inset_file.read_text())
        document["source_width"] += 1
        self.inset_file.write_text(json.dumps(document))
        parent = self.store.snapshot()["sheets"][0]
        with self.assertRaisesRegex(review.ReviewError, "dimensions"):
            self.store.inventory.render(parent)

    def test_failed_approval_does_not_record_success(self):
        item = self.main_item()
        with patch.object(review.quality, "approve", side_effect=OSError("disk full")):
            with self.assertRaisesRegex(OSError, "disk full"):
                self.decision(item)
        self.assertEqual(self.main_item()["status"], "pending")
        self.assertIsNone(self.main_item()["decision"])

    def test_http_only_accepts_same_origin_explicit_decisions(self):
        server = review.make_server(self.store, ("127.0.0.1", 0))
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        connection = http.client.HTTPConnection("127.0.0.1", server.server_port)
        self.addCleanup(connection.close)
        connection.request("GET", "/")
        response = connection.getresponse()
        page = response.read().decode()
        self.assertEqual(response.status, 200)
        token = page.split('name="review-token" content="')[1].split('"')[0]
        item = self.main_item()
        connection.request("GET", item["image"])
        response = connection.getresponse()
        self.assertEqual(response.status, 200)
        self.assertEqual(response.getheader("Content-Type"), "image/svg+xml")
        self.assertIsNotNone(ET.fromstring(response.read()).find("{http://www.w3.org/2000/svg}polyline"))
        body = json.dumps({"id": item["id"], "digest": item["digest"], "verdict": "disapproved", "note": "fix"})
        for headers in [{}, {"X-Review-Token": token, "Origin": "http://evil.invalid"}, {"X-Review-Token": token}]:
            connection.request("POST", "/api/decision", body, headers)
            response = connection.getresponse()
            response.read()
            self.assertEqual(response.status, 200 if headers == {"X-Review-Token": token} else 403)
        connection.request("GET", "/api/rejections")
        response = connection.getresponse()
        self.assertEqual(json.loads(response.read())[0]["decision"]["note"], "fix")
        connection.request("GET", "/items/../../metadata/SEC/Test%20SEC.geojson")
        response = connection.getresponse()
        response.read()
        self.assertEqual(response.status, 404)

    def test_http_warms_immutable_images_but_never_caches_mutable_review_state(self):
        server = review.make_server(self.store, ("127.0.0.1", 0))
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        connection = http.client.HTTPConnection("127.0.0.1", server.server_port)
        self.addCleanup(connection.close)

        def get(path, headers=None):
            connection.request("GET", path, headers=headers or {})
            response = connection.getresponse()
            payload = response.read()
            self.assertEqual(response.status, 200)
            return response, payload

        response, payload = get("/api/state", {"Accept-Encoding": "gzip"})
        self.assertEqual(response.getheader("Cache-Control"), "no-store")
        self.assertEqual(response.getheader("Content-Encoding"), "gzip")
        self.assertEqual(response.getheader("Vary"), "Accept-Encoding")
        snapshot = json.loads(gzip.decompress(payload))
        parent = snapshot["sheets"][0]
        response, payload = get(parent["overview_url"])
        self.assertEqual(response.getheader("Cache-Control"), "no-store")
        overview = json.loads(payload)
        response, payload = get(overview["image"], {"Accept-Encoding": "gzip"})
        self.assertEqual(response.getheader("Cache-Control"), "private, max-age=86400, immutable")
        self.assertIsNone(response.getheader("Content-Encoding"), "PNG is already compressed")
        self.assertTrue(payload.startswith(b"\x89PNG"))
        response, _ = get(snapshot["items"][0]["plain_image"])
        self.assertIn("immutable", response.getheader("Cache-Control"))
        for path in (snapshot["items"][0]["image"], "/review.js", "/prefetch.mjs", "/api/rejections"):
            response, _ = get(path)
            self.assertEqual(response.getheader("Cache-Control"), "no-store")
