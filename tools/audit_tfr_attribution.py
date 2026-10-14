# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Read-only consistency audit of published TFR JSON and captured FAA detail XML.

Details input is a JSON array of {tfr_id, raw_xml}, e.g. a read-only SQLite
detail_records export. Area-limit/schedule and geographic findings are review
candidates, not inferred geometry joins or instructions to suppress a TFR.
"""

import argparse
import json
import lzma
from pathlib import Path
import xml.etree.ElementTree as ET


def bounds(points):
    if not points:
        return None
    return tuple(fn(p[axis] for p in points) for axis in (0, 1) for fn in (min, max))


def coordinate(value):
    return float(value[:-1]) * (-1 if value[-1] in "SW" else 1)


def polygon_key(points):
    points = tuple(points)
    if len(points) > 1 and points[0] == points[-1]:
        points = points[:-1]
    return min(p[i:] + p[:i] for p in (points, points[::-1]) for i in range(len(p))) if points else ()


def xml_limit(area, side):
    value = area.findtext(f"valDistVer{side}") or ""
    unit = area.findtext(f"uomDistVer{side}") or ""
    datum = area.findtext(f"codeDistVer{side}") or ""
    if value == "0" and datum == "HEI":
        return "SFC", ""
    if unit == "FT" and datum in ("ALT", "HEI"):
        unit += " MSL" if datum == "ALT" else " AGL"
    return value, unit


def audit(state, details, require_xml_areas=False):
    areas = {}
    for area in state["areas"]:
        areas.setdefault(area["notam_id"], []).append(area)
    report = {
        "published_notices": len(areas),
        "published_areas": len(state["areas"]),
        "checked_details": len(details),
        "geometry_notices_compared": 0,
        "identity_mismatches": [],
        "collapsed_altitude_candidates": [],
        "collapsed_schedule_candidates": [],
        "disjoint_geometry_candidates": [],
        "missing_drawable_notices": [],
        "exact_area_mismatches": [],
        "exact_areas_checked": 0,
    }
    for detail in details:
        ident = detail["tfr_id"]
        root = ET.fromstring(detail["raw_xml"].lstrip("\ufeff"))
        year, number = root.findtext(".//NotUid/dateIndexYear"), root.findtext(".//NotUid/noSeqNo")
        actual = f"{year[-1]}/{int(number):04}" if year and number and number.isdecimal() else None
        if ident != actual:
            report["identity_mismatches"].append({"requested": ident, "returned": actual})
            continue
        published = areas.get(ident, [])
        drawable = [g for g in root.findall(".//TFRAreaGroup") if g.find("abdMergedArea") is not None]
        if drawable and not published:
            report["missing_drawable_notices"].append(ident)
        if not published:
            continue
        if require_xml_areas:
            from collections import Counter
            expected = Counter()
            for group in drawable:
                area = group.find("aseTFRArea")
                polygon = polygon_key((coordinate(v.findtext("geoLat")), coordinate(v.findtext("geoLong")))
                                      for v in group.findall("abdMergedArea/Avx"))
                expected[(polygon, xml_limit(area, "Lower"), xml_limit(area, "Upper"))] += 1
            actual = Counter((polygon_key((p["lat"], p["lon"]) for p in a["polygon"]),
                              tuple(a["lower_limit"][k] for k in ("value_text", "unit")),
                              tuple(a["upper_limit"][k] for k in ("value_text", "unit"))) for a in published)
            report["exact_areas_checked"] += sum(expected.values())
            if expected != actual:
                report["exact_area_mismatches"].append({"id": ident,
                    "missing": sum((expected-actual).values()), "unexpected": sum((actual-expected).values())})
        groups = root.findall(".//TFRAreaGroup")
        limits = sorted({tuple(group.findtext(".//" + key) or "" for key in (
            "valDistVerLower", "codeDistVerLower", "uomDistVerLower",
            "valDistVerUpper", "codeDistVerUpper", "uomDistVerUpper",
        )) for group in groups})
        published_limits = sorted({tuple(area[limit][key] for limit in ("lower_limit", "upper_limit")
                                          for key in ("value_text", "unit")) for area in published})
        if len(limits) > 1 and len(published_limits) == 1:
            report["collapsed_altitude_candidates"].append({
                "id": ident, "source_limits": limits, "published_limits": published_limits,
            })
        schedules = sorted({tuple(group.findtext(".//" + key) or "" for key in ("dateEffective", "dateExpire")) for group in groups})
        if len(schedules) > 1 and all(not area["schedule_fragments"] for area in published):
            report["collapsed_schedule_candidates"].append({"id": ident, "source_schedules": schedules})
        xml_points = []
        for vertex in root.findall(".//Avx"):
            lat, lon = vertex.findtext("geoLat"), vertex.findtext("geoLong")
            if lat and lon:
                xml_points.append((float(lat[:-1]) * (-1 if lat[-1] == "S" else 1),
                                   float(lon[:-1]) * (-1 if lon[-1] == "W" else 1)))
        expected = bounds(xml_points)
        if expected is None:
            continue
        report["geometry_notices_compared"] += 1
        for area in published:
            actual = bounds([(p["lat"], p["lon"]) for p in area["polygon"]])
            # Coarse, deliberately tolerant independent check. This is not a
            # polygon matcher and does not prove coincident geometry is correct.
            if actual and (actual[0] > expected[1] + 0.1 or actual[1] < expected[0] - 0.1
                           or actual[2] > expected[3] + 0.1 or actual[3] < expected[2] - 0.1):
                report["disjoint_geometry_candidates"].append(area["area_id"])
    return report


def read_json(path):
    data = Path(path).read_bytes()
    return json.loads(lzma.decompress(data) if str(path).endswith(".xz") else data)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--state", required=True, help="published TFR JSON, optionally .xz")
    parser.add_argument("--details", required=True, help="captured detail_records JSON export")
    parser.add_argument("--require-xml-areas", action="store_true", help="require exact per-area XML geometry and altitude attribution")
    args = parser.parse_args()
    report = audit(read_json(args.state), read_json(args.details), args.require_xml_areas)
    print(json.dumps(report, indent=2))
    return int(bool(report["identity_mismatches"] or report["missing_drawable_notices"] or report["exact_area_mismatches"]))


if __name__ == "__main__":
    raise SystemExit(main())
