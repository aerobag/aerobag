# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

import copy
import unittest

from audit_tfr_attribution import audit


class AttributionAuditTests(unittest.TestCase):
    def fixture(self):
        area = {"area_id": "6/4348:a", "notam_id": "6/4348", "schedule_fragments": [],
                "lower_limit": {"value_text": "SFC", "unit": ""},
                "upper_limit": {"value_text": "2400", "unit": "FT"},
                "polygon": [{"lat": 48, "lon": -114}, {"lat": 49, "lon": -113}]}
        groups = "".join(f"""<TFRAreaGroup><aseTFRArea>
            <valDistVerLower>{lower}</valDistVerLower><codeDistVerLower>ALT</codeDistVerLower><uomDistVerLower>FT</uomDistVerLower>
            <valDistVerUpper>{upper}</valDistVerUpper><codeDistVerUpper>ALT</codeDistVerUpper><uomDistVerUpper>FT</uomDistVerUpper>
            <dateEffective>2026-10-15T0{hour}:00:00</dateEffective><dateExpire>2026-10-15T04:00:00</dateExpire>
            </aseTFRArea><Avx><geoLat>48N</geoLat><geoLong>114W</geoLong></Avx>
            <Avx><geoLat>49N</geoLat><geoLong>113W</geoLong></Avx></TFRAreaGroup>"""
            for lower, upper, hour in [(0, 2400, 1), (2400, 18000, 2)])
        details = [{"tfr_id": "6/4348", "raw_xml": f"""<Not><NotUid><dateIndexYear>2026</dateIndexYear>
            <noSeqNo>4348</noSeqNo></NotUid>{groups}</Not>"""}]
        return {"areas": [area]}, details

    def test_distinct_area_facts_must_not_disappear_unnoticed(self):
        state, details = self.fixture()
        report = audit(state, details)
        self.assertEqual(len(report["collapsed_altitude_candidates"]), 1)
        self.assertEqual(len(report["collapsed_schedule_candidates"]), 1)
        self.assertEqual(report["disjoint_geometry_candidates"], [])
        self.assertEqual(report["identity_mismatches"], [])

    def test_preserved_area_differences_are_not_flagged(self):
        state, details = self.fixture()
        second = copy.deepcopy(state["areas"][0])
        second["area_id"] = "6/4348:b"
        second["lower_limit"]["value_text"] = "2400"
        second["upper_limit"]["value_text"] = "18000"
        state["areas"].append(second)
        for i, area in enumerate(state["areas"]):
            area["schedule_fragments"] = [{"kind": "effective", "value_utc": f"2026-10-15T0{i+1}:00:00Z"}]
        report = audit(state, details)
        self.assertEqual(report["collapsed_altitude_candidates"], [])
        self.assertEqual(report["collapsed_schedule_candidates"], [])

    def test_unrelated_response_is_an_identity_failure(self):
        state, details = self.fixture()
        details[0]["tfr_id"] = "6/9999"
        self.assertEqual(audit(state, details)["identity_mismatches"], [{"requested": "6/9999", "returned": "6/4348"}])

    def test_far_away_geometry_is_a_review_candidate(self):
        state, details = self.fixture()
        state["areas"][0]["polygon"] = [{"lat": 34, "lon": -118}]
        self.assertEqual(audit(state, details)["disjoint_geometry_candidates"], ["6/4348:a"])

    def test_dropping_all_areas_cannot_pass(self):
        _, details = self.fixture()
        details[0]["raw_xml"] = details[0]["raw_xml"].replace("<Avx>", "<abdMergedArea><Avx>", 1).replace("</Avx>", "</Avx></abdMergedArea>", 1)
        self.assertEqual(audit({"areas": []}, details)["missing_drawable_notices"], ["6/4348"])

    def test_exact_attribution_rejects_swapped_altitudes_without_guessing_geometry(self):
        state, details = self.fixture()
        xml = details[0]["raw_xml"].replace("</aseTFRArea>", "</aseTFRArea><abdMergedArea>").replace("</TFRAreaGroup>", "</abdMergedArea></TFRAreaGroup>")
        details[0]["raw_xml"] = xml
        second = copy.deepcopy(state["areas"][0])
        second["area_id"] = "6/4348:b"
        state["areas"].append(second)
        for area, lower, upper in zip(state["areas"], (0, 2400), (2400, 18000)):
            area["lower_limit"] = {"value_text": str(lower), "unit": "FT MSL"}
            area["upper_limit"] = {"value_text": str(upper), "unit": "FT MSL"}
        self.assertEqual(audit(state, details, True)["exact_area_mismatches"], [])
        state["areas"][0]["lower_limit"], state["areas"][1]["lower_limit"] = state["areas"][1]["lower_limit"], state["areas"][0]["lower_limit"]
        self.assertEqual(audit(state, details, True)["exact_area_mismatches"], [{"id": "6/4348", "missing": 2, "unexpected": 2}])


if __name__ == "__main__":
    unittest.main()
