# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Explicit-CRS cutlines: sparse editing geometry, bounded derived reprojection.

An edge is straight in the CRS stored in its document, not in the destination
CRS. The editor, visual review and tile builder all use this implementation.
"""

import argparse
import copy
import json
import math
from pathlib import Path

from osgeo import gdal, ogr, osr

PIXEL_CURVE_TOLERANCE = 0.025
EDIT_TOLERANCE = 0.20
CORNER_TURN_DEGREES = 30
MAX_VERTICES = 1_000_000
MAX_DEPTH = 24
INSET_LAYOUT_SCHEMA = 3
WORLD_WIDTH = 2 * math.pi * 6378137
INSET_EXCLUDE_TARGET = "EXCLUDE"
INSET_TARGETS = {"SEC": ("TAC", "FLY", INSET_EXCLUDE_TARGET), "TAC": ("TAC", "FLY", INSET_EXCLUDE_TARGET),
                 "FLY": ("FLY", "TAC", INSET_EXCLUDE_TARGET), "ENR_L": ("ENR_L", INSET_EXCLUDE_TARGET),
                 "ENR_H": ("ENR_H", INSET_EXCLUDE_TARGET)}
INSET_TARGET_LABELS = {"TAC": "TAC", "FLY": "Flyway", "ENR_L": "IFR-L", "ENR_H": "IFR-H",
                       INSET_EXCLUDE_TARGET: "Exclude only (no output layer)"}


def inset_requires_georeference(inset):
    return inset["target_family"] != INSET_EXCLUDE_TARGET


def source_sheet(document):
    """Optional registration of an FAA detail TIFF on its printed parent sheet.

    This is source-image placement, not the detail's geographic calibration.
    The latter always comes from its own FAA GeoTIFF.
    """
    value = document.get("source_sheet")
    if value is None:
        return None
    if (not isinstance(value, dict) or value.get("schema_version") != 1
            or not isinstance(value.get("source"), str) or Path(value["source"]).name != value["source"]
            or not value["source"].endswith('.tif')):
        raise ValueError("Invalid FAA detail source-sheet registration")
    for key in ('source_width', 'source_height', 'detail_width', 'detail_height'):
        if not isinstance(value.get(key), int) or value[key] <= 0:
            raise ValueError("FAA detail registration requires source dimensions")
    transform = value.get('pixel_transform')
    if (not isinstance(transform, list) or len(transform) != 6
            or not all(isinstance(v, (int, float)) and math.isfinite(v) for v in transform)
            or gdal.InvGeoTransform(transform) is None):
        raise ValueError("FAA detail registration requires an invertible pixel transform")
    return value


def inset_layouts(root, destination):
    paths = []
    for family, targets in INSET_TARGETS.items():
        for path in sorted((Path(root) / family).glob('*.navigable-insets.json')):
            document = json.loads(path.read_text())
            if document.get('schema_version') != INSET_LAYOUT_SCHEMA:
                raise ValueError(f'Unsupported inset layout: {path}')
            if any(i['target_family'] not in targets for i in document['insets']):
                raise ValueError(f'Unsupported source/destination family in {path}')
            if destination in targets:
                paths.append(str(path))
    return paths


def registered_detail_geometry(document, detail, parent):
    registration = source_sheet(document)
    if (parent.RasterXSize, parent.RasterYSize, detail.RasterXSize, detail.RasterYSize) != (
            registration['source_width'], registration['source_height'],
            registration['detail_width'], registration['detail_height']):
        raise ValueError("FAA detail registration dimensions changed; review source placement")
    geometry = pixel_document(document, detail)
    return mapped_geometry(geometry, lambda p: gdal.ApplyGeoTransform(registration['pixel_transform'], *p),
                           PIXEL_CURVE_TOLERANCE)


def spatial_reference(name):
    if not isinstance(name, str) or not name:
        raise ValueError("Cutline or raster requires an explicit coordinate system")
    srs = osr.SpatialReference()
    if srs.SetFromUserInput(name) != 0:
        raise ValueError("Invalid cutline coordinate system")
    srs.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
    return srs


def document_srs(document):
    crs = document.get("crs", {})
    if crs.get("type") != "name":
        raise ValueError("Cutline requires an explicit named coordinate system")
    return spatial_reference(crs.get("properties", {}).get("name"))


def read(path):
    return decode(json.loads(Path(path).read_text()))


def decode(document):
    srs = document_srs(document)
    if document.get("type") != "FeatureCollection" or not document.get("features"):
        raise ValueError("Cutline must be a nonempty FeatureCollection")
    polygons = ogr.Geometry(ogr.wkbMultiPolygon)
    for feature in document["features"]:
        geometry = ogr.CreateGeometryFromJson(json.dumps(feature["geometry"], allow_nan=False))
        if geometry is None:
            raise ValueError("Invalid cutline geometry")
        if geometry.GetGeometryName() == "POLYGON":
            polygons.AddGeometry(geometry)
        elif geometry.GetGeometryName() == "MULTIPOLYGON":
            for polygon in geometry:
                polygons.AddGeometry(polygon)
        else:
            raise ValueError("Cutline must contain only polygons")
    validate(polygons)
    return document, srs, polygons


def validate(geometry):
    if geometry.IsEmpty() or not geometry.IsValid() or geometry.GetArea() <= 0:
        raise ValueError("Invalid cutline polygon topology")
    for polygon in geometry if geometry.GetGeometryName() == "MULTIPOLYGON" else [geometry]:
        for ring in polygon:
            if ring.GetPointCount() < 4 or ring.GetPoint(0) != ring.GetPoint(ring.GetPointCount() - 1):
                raise ValueError("Cutline rings must be closed with at least three vertices")
            if any(not math.isfinite(v) for point in ring.GetPoints() for v in point):
                raise ValueError("Cutline coordinates must be finite")


def coordinate_map(source, target):
    if source.IsSame(target):
        return lambda p: tuple(p[:2])
    transform = osr.CoordinateTransformation(source, target)

    def project(p):
        result = transform.TransformPoint(*p[:2])[:2]
        if not all(math.isfinite(v) for v in result):
            raise ValueError("Cutline cannot be projected into the requested coordinate system")
        return result

    return project


def interpolate(a, b, t):
    return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t)


def mapped_geometry(geometry, project, tolerance, metric=lambda p: p):
    """Subdivide in the *input* CRS, measuring chord error in the requested metric."""
    output = ogr.Geometry(ogr.wkbMultiPolygon)
    count = 0
    for polygon in geometry:
        result = ogr.Geometry(ogr.wkbPolygon)
        for ring in polygon:
            points = [p[:2] for p in ring.GetPoints()]
            target = ogr.Geometry(ogr.wkbLinearRing)
            target.AddPoint_2D(*project(points[0]))
            for a, b in zip(points, points[1:]):
                stack = [(a, b, project(a), project(b), 0)]
                while stack:
                    a, b, pa, pb, depth = stack.pop()
                    samples = [interpolate(a, b, t) for t in (0.25, 0.5, 0.75)]
                    mapped = [project(p) for p in samples]
                    error = max(math.dist(metric(p), metric(interpolate(pa, pb, t)))
                                for p, t in zip(mapped, (0.25, 0.5, 0.75)))
                    if error > tolerance:
                        if depth >= MAX_DEPTH:
                            raise ValueError("Cutline projection is discontinuous or exceeds subdivision limit")
                        mid, pm = samples[1], mapped[1]
                        stack.extend([(mid, b, pm, pb, depth + 1), (a, mid, pa, pm, depth + 1)])
                    else:
                        target.AddPoint_2D(*pb)
                        count += 1
                        if count > MAX_VERTICES:
                            raise ValueError("Projected cutline exceeds vertex budget")
            result.AddGeometry(target)
        output.AddGeometry(result)
    validate(output)
    return output


def source_pixel_map(source):
    inverse = gdal.InvGeoTransform(source.GetGeoTransform())
    if inverse is None:
        raise ValueError("Chart has a singular geotransform")
    return lambda p: gdal.ApplyGeoTransform(inverse, *p[:2])


def pixel_geometry(path, source):
    return pixel_document(json.loads(Path(path).read_text()), source)


def pixel_document(document, source):
    _, srs, geometry = decode(document)
    project = coordinate_map(srs, spatial_reference(source.GetProjection()))
    pixels = source_pixel_map(source)
    return mapped_geometry(geometry, lambda p: pixels(project(p)), PIXEL_CURVE_TOLERANCE)


def segment_distance(point, a, b):
    dx, dy = b[0] - a[0], b[1] - a[1]
    length = dx * dx + dy * dy
    t = max(0, min(1, ((point[0] - a[0]) * dx + (point[1] - a[1]) * dy) / length)) if length else 0
    return math.dist(point, interpolate(a, b, t))


def simplified_ring(points, tolerance=EDIT_TOLERANCE):
    """Closed-ring RDP between protected corners; never repair invalid topology."""
    if not math.isfinite(tolerance) or tolerance <= 0:
        raise ValueError("Simplification tolerance must be positive and finite")
    points = [tuple(p[:2]) for p in points]
    if points and points[0] == points[-1]:
        points.pop()
    if len(points) < 3:
        raise ValueError("Cutline needs at least three vertices")
    n = len(points)
    pins = []
    for i, b in enumerate(points):
        a, c = points[i - 1], points[(i + 1) % n]
        u, v = (b[0] - a[0], b[1] - a[1]), (c[0] - b[0], c[1] - b[1])
        length = math.hypot(*u) * math.hypot(*v)
        if length == 0 or (u[0] * v[0] + u[1] * v[1]) / length <= math.cos(math.radians(CORNER_TURN_DEGREES)):
            pins.append(i)
    if not pins:
        pins = [0]
    if len(pins) < 2:
        pins.append(max(range(n), key=lambda i: math.dist(points[pins[0]], points[i])))
    pins.sort()
    keep = set(pins)
    stack = list(zip(pins, pins[1:] + [pins[0] + n]))
    while stack:
        a, b = stack.pop()
        if b - a <= 1:
            continue
        error, i = max((segment_distance(points[i % n], points[a % n], points[b % n]), i)
                       for i in range(a + 1, b))
        if error > tolerance:
            keep.add(i % n)
            stack.extend([(a, i), (i, b)])
    result = [points[i] for i in sorted(keep)]
    polygon = polygon_from_points(result)
    validate(polygon)
    return result


def polygon_from_points(points):
    ring = ogr.Geometry(ogr.wkbLinearRing)
    for point in points:
        ring.AddPoint_2D(*point[:2])
    ring.CloseRings()
    polygon = ogr.Geometry(ogr.wkbPolygon)
    polygon.AddGeometry(ring)
    return polygon


def edit_geometry(path, source):
    document, srs, geometry = read(path)
    if geometry.GetGeometryCount() != 1 or geometry.GetGeometryRef(0).GetGeometryCount() != 1:
        raise ValueError("Cutline editor requires one polygon without holes")
    ring = geometry.GetGeometryRef(0).GetGeometryRef(0)
    stored = ring.GetPoints()[:-1]
    project = coordinate_map(srs, spatial_reference(source.GetProjection()))
    pixels = source_pixel_map(source)
    handles = [pixels(project(p)) for p in stored]
    rendered = pixel_document(document, source)
    outline = [p[:2] for p in rendered.GetGeometryRef(0).GetGeometryRef(0).GetPoints()]
    # Choose the more compact edit representation, not a different visible shape.
    # A sparse Mercator outline may be curved in source pixels: keep its handles
    # in Mercator rather than replacing four handles with hundreds of samples.
    if len(stored) > 4:
        native = simplified_ring(outline)
        if len(native) < len(handles):
            handles = native
            srs = spatial_reference(source.GetProjection())
            outline = native + [native[0]]
    return {"points": handles, "outline": outline, "crs": srs.ExportToWkt(), "document": document}


def editable_points(path, source):
    return edit_geometry(path, source)["points"]


def native_document(document, source, points):
    """Save the straight edges the operator sees, with their full native CRS."""
    return edited_document(document, source, points, source.GetProjection())


def edited_document(document, source, points, crs):
    result = copy.deepcopy(document)
    if len(result["features"]) != 1 or result["features"][0]["geometry"]["type"] != "Polygon":
        raise ValueError("Cutline editor requires one polygon")
    if len(result["features"][0]["geometry"]["coordinates"]) != 1:
        raise ValueError("Cutline editor cannot discard polygon holes")
    srs = spatial_reference(crs)
    project = coordinate_map(spatial_reference(source.GetProjection()), srs)
    polygon = polygon_from_points([project(gdal.ApplyGeoTransform(source.GetGeoTransform(), *p)) for p in points])
    validate(polygon)
    result["crs"] = {"type": "name", "properties": {"name": srs.ExportToWkt()}}
    result["features"][0]["geometry"] = json.loads(polygon.ExportToJson())
    return result


def unwrapped_mercator_geometry(path, source=None):
    _, srs, geometry = read(path)
    if source is not None:
        # Registered FAA details share the same source-pixel exclusion contract
        # as the review overlay. Their own GeoTIFFs remain untouched.
        native = spatial_reference(source.GetProjection())
        to_cutline = coordinate_map(native, srs)
        to_pixels = source_pixel_map(source)
        from_cutline = coordinate_map(srs, native)
        for detail_path in sorted(Path(path).parent.glob('*.geojson')):
            document = json.loads(detail_path.read_text())
            registration = source_sheet(document)
            if registration is None or registration['source'].casefold() != Path(source.GetDescription()).name.casefold():
                continue
            detail = gdal.Open(str(Path(source.GetDescription()).parent / (detail_path.stem + '.tif')))
            pixels = registered_detail_geometry(document, detail, source)
            exclusion = mapped_geometry(pixels, lambda p: to_cutline(gdal.ApplyGeoTransform(source.GetGeoTransform(), *p)),
                                        PIXEL_CURVE_TOLERANCE, lambda p: to_pixels(from_cutline(p)))
            geometry = as_multipolygon(geometry.Difference(exclusion))
            validate(geometry)
    mercator = spatial_reference("EPSG:3857")
    if srs.IsSame(mercator):
        return geometry
    wrapped = coordinate_map(srs, mercator)
    anchor = wrapped(geometry.GetGeometryRef(0).GetGeometryRef(0).GetPoint(0))[0]

    def project(point):
        x, y = wrapped(point)
        return x + round((anchor - x) / WORLD_WIDTH) * WORLD_WIDTH, y

    if source is None:
        return mapped_geometry(geometry, project, 0.25)
    inverse = coordinate_map(mercator, spatial_reference(source.GetProjection()))
    pixels = source_pixel_map(source)
    return mapped_geometry(geometry, project, PIXEL_CURVE_TOLERANCE, lambda p: pixels(inverse(p)))


def as_multipolygon(geometry):
    if geometry.GetGeometryName() == 'POLYGON':
        polygons = ogr.Geometry(ogr.wkbMultiPolygon)
        polygons.AddGeometry(geometry)
        return polygons
    return geometry


def mercator_parts(path, source=None):
    """Split in a continuous projected plane, never join across the map seam."""
    geometry = unwrapped_mercator_geometry(path, source)
    left, right, bottom, top = geometry.GetEnvelope()
    first = math.floor((left + WORLD_WIDTH / 2) / WORLD_WIDTH)
    last = math.floor((right + WORLD_WIDTH / 2) / WORLD_WIDTH)
    parts = []
    for world in range(first, last + 1):
        west = (world - .5) * WORLD_WIDTH
        east = (world + .5) * WORLD_WIDTH
        part = geometry.Intersection(polygon_from_points([
            (west, bottom - 1), (east, bottom - 1), (east, top + 1), (west, top + 1)]))
        if part.IsEmpty() or not part.GetArea():
            continue
        part = as_multipolygon(part)
        shifted = mapped_geometry(part, lambda p: (p[0] - world * WORLD_WIDTH, p[1]), .25)
        parts.append(shifted)
    return parts


def mercator_geometry(path, source=None):
    return unwrapped_mercator_geometry(path, source)


def write_geometry(output, geometry):
    document = {"type": "FeatureCollection",
                "crs": {"type": "name", "properties": {"name": "EPSG:3857"}},
                "features": [{"type": "Feature", "properties": {},
                              "geometry": json.loads(geometry.ExportToJson())}]}
    output = Path(output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(document, allow_nan=False) + "\n")


def prepare(path, source, output):
    geometry = mercator_geometry(path, source)
    write_geometry(output, geometry)


def prepare_parts(path, source, output):
    paths = []
    output = Path(output)
    for i, geometry in enumerate(mercator_parts(path, source)):
        destination = output.with_name(f"{output.stem}.part-{i}.geojson")
        write_geometry(destination, geometry)
        west, east, south, north = geometry.GetEnvelope()
        paths.append({"cutline": str(destination), "bounds": [west, south, east, north]})
    return paths


def lon_lat(point):
    x, y = point[:2]
    radius = 6378137
    return [math.degrees(x / radius), math.degrees(2 * math.atan(math.exp(y / radius)) - math.pi / 2)]


def coverage(path):
    left, right, bottom, top = mercator_geometry(path).GetEnvelope()
    west, south = lon_lat((left, bottom))
    east, north = lon_lat((right, top))
    return {"lat_min": south, "lat_max": north, "lon_min": west, "lon_max": east}


def geographic_exteriors(directory):
    # Offline package coverage intentionally uses exterior footprints; the actual
    # raster clipping path above retains holes and every polygon component.
    result = []
    for path in sorted(Path(directory).glob("*.geojson")):
        for geometry in mercator_parts(path):
            for polygon in geometry:
                result.append([lon_lat(p) for p in polygon.GetGeometryRef(0).GetPoints()])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=["prepare", "prepare-parts", "coverage", "exteriors", "inset-layouts"])
    parser.add_argument("--cutline", required=True, type=Path)
    parser.add_argument("--source", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--destination")
    args = parser.parse_args()
    gdal.UseExceptions()
    ogr.UseExceptions()
    osr.UseExceptions()
    if args.operation == "inset-layouts":
        print(json.dumps(inset_layouts(args.cutline, args.destination)))
    elif args.operation == "coverage":
        print(json.dumps(coverage(args.cutline)))
    elif args.operation == "exteriors":
        print(json.dumps(geographic_exteriors(args.cutline)))
    else:
        if args.source is None or args.output is None:
            parser.error("prepare requires --source and --output")
        source = gdal.Open(str(args.source))
        if args.operation == "prepare-parts":
            print(json.dumps(prepare_parts(args.cutline, source, args.output)))
        else:
            prepare(args.cutline, source, args.output)


if __name__ == "__main__":
    main()
