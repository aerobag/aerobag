# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""One inset georeference for editor diagnostics and production raster warping.

The parent raster supplies only the projection, never the inset's placement.
Fit pixels to that projected plane, then let PROJ reproject the curved graticule.
"""

import argparse
import json
import math
from pathlib import Path
import sys

import numpy as np
from osgeo import gdal, ogr, osr

gdal.UseExceptions()
ogr.UseExceptions()
osr.UseExceptions()

MIN_CONSTRAINTS = 8
FIT_STEP_TOLERANCE_PX = 1e-5
CONTROL_FIELDS = {
    "intersection": ("latitude", "longitude"),
    "latitude": ("latitude",),
    "longitude": ("longitude",),
}


class GeoreferenceError(ValueError):
    pass


def normalize_control(point, *, allow_incomplete=False):
    kind = point.get("kind")
    if kind not in CONTROL_FIELDS:
        raise GeoreferenceError("Control kind must be intersection, latitude, or longitude")
    try:
        pixel = np.asarray(point.get("pixel"), dtype=float)
    except (TypeError, ValueError) as error:
        raise GeoreferenceError("Control must have two finite pixel coordinates") from error
    if pixel.shape != (2,) or not np.isfinite(pixel).all():
        raise GeoreferenceError("Control must have two finite pixel coordinates")
    result = {"kind": kind, "pixel": [float(value) for value in pixel]}
    for field, limit in (("latitude", 90), ("longitude", 180)):
        value = point.get(field)
        if field not in CONTROL_FIELDS[kind]:
            if value is not None:
                raise GeoreferenceError(f"{kind}-only control must not supply {field}")
        elif value is None:
            if not allow_incomplete:
                raise GeoreferenceError(f"{kind} control needs {field}")
        else:
            if isinstance(value, bool) or not isinstance(value, (int, float)):
                raise GeoreferenceError(f"{field} must be a number in decimal degrees")
            if not math.isfinite(value) or not -limit <= value <= limit:
                raise GeoreferenceError(f"Invalid {field}")
            value = float(value)
        result[field] = value
    return result


def complete_control(point):
    return all(point[field] is not None for field in CONTROL_FIELDS[point["kind"]])


def affine_fit(pixels, projected):
    # Centering avoids ill-conditioning when the inset is far from pixel (0, 0).
    origin = pixels.mean(axis=0)
    center = projected.mean(axis=0)
    design = np.column_stack((np.ones(len(pixels)), pixels - origin))
    coefficients, _, rank, _ = np.linalg.lstsq(design, projected - center, rcond=None)
    if rank < 3:
        raise GeoreferenceError("Control points are collinear")
    linear = coefficients[1:].T
    if np.linalg.matrix_rank(linear) < 2:
        raise GeoreferenceError("Control coordinates do not define a two-dimensional map")
    offset = center + coefficients[0] - linear @ origin
    return (offset[0], linear[0, 0], linear[0, 1], offset[1], linear[1, 0], linear[1, 1])


def ground_distance(a, b):
    """Approximate ground metres, not latitude-inflated Web Mercator metres."""
    lon1, lat1 = map(math.radians, a)
    lon2, lat2 = map(math.radians, b)
    h = math.sin((lat2 - lat1) / 2) ** 2
    h += math.cos(lat1) * math.cos(lat2) * math.sin((lon2 - lon1) / 2) ** 2
    return 2 * 6371008.8 * math.asin(math.sqrt(min(1.0, max(0.0, h))))


class CoordinateFit:
    """Six affine coefficients, constrained only by explicitly observed coordinates."""

    def __init__(self, pixels, observed, projection, inverse_projection):
        self.pixels = pixels
        self.origin = pixels.mean(axis=0)
        self.scale = pixels.std(axis=0)
        if np.any(self.scale < 1e-9):
            raise GeoreferenceError("Control points are collinear")
        self.design = np.column_stack((np.ones(len(pixels)), (pixels - self.origin) / self.scale))
        if np.linalg.matrix_rank(self.design) < 3:
            raise GeoreferenceError("Control points are collinear")
        self.observed = observed.copy()
        self.known = np.isfinite(observed)
        for axis, name in enumerate(("longitude", "latitude")):
            values = observed[self.known[:, axis], axis]
            if len(values) < 4:
                raise GeoreferenceError(f"Need at least four {name} observations for leave-one-out checks")
            if np.ptp(values) < 1e-8:
                raise GeoreferenceError(f"Need more than one distinct {name}")
        # Unwrap longitudes about one observed meridian, including antimeridian insets.
        anchor = observed[self.known[:, 0], 0][0]
        self.observed[:, 0] = anchor + (observed[:, 0] - anchor + 180) % 360 - 180
        reference_lat = np.mean(observed[self.known[:, 1], 1])
        self.metres = 6371008.8 * math.pi / 180 * np.array([math.cos(math.radians(reference_lat)), 1])
        self.projection = projection
        self.inverse_projection = inverse_projection

    def geographic(self, projected):
        try:
            result = np.array(self.inverse_projection.TransformPoints(projected.tolist()))[:, :2]
        except RuntimeError as error:
            raise GeoreferenceError("Fit leaves the chart projection's valid domain") from error
        if not np.isfinite(result).all():
            raise GeoreferenceError("Fit leaves the chart projection's valid domain")
        return result

    def geographic_gradient(self, projected):
        # A one-projection-unit central difference delegates projection mathematics to PROJ.
        columns = []
        for direction in (np.array([1.0, 0.0]), np.array([0.0, 1.0])):
            delta = self.geographic(projected + direction) - self.geographic(projected - direction)
            delta[:, 0] = (delta[:, 0] + 180) % 360 - 180
            columns.append(delta / 2)
        return np.stack(columns, axis=2)

    def residual(self, coefficients, keep):
        delta = self.geographic(self.design @ coefficients) - self.observed
        delta[:, 0] = (delta[:, 0] + 180) % 360 - 180
        return (delta * self.metres)[self.known & keep[:, None]]

    def solve(self, keep):
        # This angular affine estimate is only an optimizer seed. Missing coordinates
        # never enter the residual or become synthetic observations.
        angular = np.empty((3, 2))
        for axis, name in enumerate(("longitude", "latitude")):
            selected = keep & self.known[:, axis]
            angular[:, axis], _, rank, _ = np.linalg.lstsq(
                self.design[selected], self.observed[selected, axis], rcond=None
            )
            if rank < 3:
                raise GeoreferenceError(f"{name.capitalize()} controls need better spatial spread")
        seed = self.design @ angular
        seed[:, 0] = (seed[:, 0] + 180) % 360 - 180
        if np.any(np.abs(seed[:, 1]) >= 90):
            raise GeoreferenceError("Control geometry cannot initialize a valid projected fit")
        try:
            projected = np.array(self.projection.TransformPoints(seed.tolist()))[:, :2]
        except RuntimeError as error:
            raise GeoreferenceError("Control geometry cannot initialize a valid projected fit") from error
        if not np.isfinite(projected).all():
            raise GeoreferenceError("Control geometry cannot initialize a valid projected fit")
        coefficients = np.linalg.lstsq(self.design, projected, rcond=None)[0]
        selected = self.known & keep[:, None]
        for _ in range(30):
            residual = self.residual(coefficients, keep)
            cost = float(residual @ residual)
            gradient = self.geographic_gradient(self.design @ coefficients) * self.metres[None, :, None]
            jacobian = (gradient[:, :, None, :] * self.design[:, None, :, None]).reshape(-1, 2, 6)[selected]
            step, _, rank, singular = np.linalg.lstsq(jacobian, -residual, rcond=None)
            if rank < 6 or singular[-1] < singular[0] * 1e-8:
                raise GeoreferenceError("Control constraints do not independently determine the transform")
            # Stop at sub-pixel stability, not an arbitrary precision in CRS units.
            # Real, noisy controls retain nonzero residuals; PROJ roundoff can make
            # further objective reduction impossible even at the optimum.
            linear = coefficients[1:].T / self.scale
            if np.linalg.matrix_rank(linear) < 2:
                raise GeoreferenceError("Control coordinates do not define a two-dimensional map")
            motion = (self.design @ step.reshape(3, 2)) @ np.linalg.inv(linear).T
            if np.max(np.linalg.norm(motion, axis=1)) < FIT_STEP_TOLERANCE_PX or cost < 1e-10:
                return coefficients
            for fraction in (1, .5, .25, .125, .0625, .03125, .015625, .0078125):
                candidate = coefficients + fraction * step.reshape(3, 2)
                candidate_residual = self.residual(candidate, keep)
                candidate_cost = float(candidate_residual @ candidate_residual)
                if candidate_cost <= cost:
                    coefficients = candidate
                    break
            else:
                raise GeoreferenceError("Projected fit did not converge; check the control coordinates")
            if cost - candidate_cost < 1e-10 * max(1, cost):
                return coefficients
        raise GeoreferenceError("Projected fit did not converge; check the control coordinates")

    def geotransform(self, coefficients):
        linear = coefficients[1:].T / self.scale
        if np.linalg.matrix_rank(linear) < 2:
            raise GeoreferenceError("Control coordinates do not define a two-dimensional map")
        offset = coefficients[0] - linear @ self.origin
        return (offset[0], linear[0, 0], linear[0, 1], offset[1], linear[1, 0], linear[1, 1])

    def error(self, index, predicted, transform):
        geographic = self.geographic(np.array([predicted]))[0]
        known = self.known[index]
        delta = geographic - self.observed[index]
        delta[0] = (delta[0] + 180) % 360 - 180
        linear = np.array([[transform[1], transform[2]], [transform[4], transform[5]]])
        gradient = self.geographic_gradient(np.array([predicted]))[0] @ linear
        # A one-coordinate tick measures cross-track error, not position along its line.
        pixel_delta = np.linalg.lstsq(
            (gradient * self.metres[:, None])[known], (delta * self.metres)[known], rcond=None
        )[0]
        constrained = geographic.copy()
        constrained[known] = self.observed[index, known]
        return float(np.linalg.norm(pixel_delta)), ground_distance(geographic, constrained)


class InsetGeoreference:
    def __init__(self, source_path, control_points):
        controls = [normalize_control(point) for point in control_points]
        count = sum(len(CONTROL_FIELDS[point["kind"]]) for point in controls)
        if count < MIN_CONSTRAINTS:
            raise GeoreferenceError(
                f"Need at least {MIN_CONSTRAINTS} coordinate constraints; have {count} "
                "(intersection=2, tick=1)"
            )
        dataset = gdal.Open(str(source_path))
        self.srs = dataset.GetSpatialRef()
        dimensions = np.array([dataset.RasterXSize, dataset.RasterYSize])
        dataset = None
        if self.srs is None or not self.srs.IsProjected():
            raise GeoreferenceError("Source chart must supply a projected coordinate system")
        self.srs.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
        geographic = osr.SpatialReference()
        geographic.ImportFromEPSG(4326)
        geographic.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
        pixels = np.array([point["pixel"] for point in controls], dtype=float)
        if np.any(pixels < 0) or np.any(pixels > dimensions):
            raise GeoreferenceError("Control point is outside the source raster")
        observed = np.array([[point["longitude"], point["latitude"]] for point in controls], dtype=float)
        problem = CoordinateFit(
            pixels, observed, osr.CoordinateTransformation(geographic, self.srs),
            osr.CoordinateTransformation(self.srs, geographic),
        )
        keep = np.ones(len(controls), dtype=bool)
        coefficients = problem.solve(keep)
        self.transform = problem.geotransform(coefficients)
        fit_errors, check_errors, ground_errors = [], [], []
        for index in range(len(controls)):
            predicted = problem.design[index] @ coefficients
            fit_errors.append(problem.error(index, predicted, self.transform)[0])
            # Withhold the entire mark, including both coordinates of an intersection.
            keep[index] = False
            local = problem.solve(keep)
            keep[index] = True
            predicted = problem.design[index] @ local
            pixel_error, ground_error = problem.error(index, predicted, self.transform)
            check_errors.append(pixel_error)
            ground_errors.append(ground_error)
        rms_m = float(np.sqrt(np.mean(np.square(ground_errors))))
        self.diagnostics = {
            "ready": True,
            "fit": "leave-one-out constrained affine in source chart projection",
            "projection": self.srs.GetAttrValue("PROJECTION"),
            "constraint_count": count,
            "fit_max_error_px": round(max(fit_errors), 3),
            "max_error_px": round(max(check_errors), 3),
            "rms_error_m": round(rms_m, 1),
            "max_error_m": round(max(ground_errors), 1),
            "point_errors_px": [round(error, 3) for error in check_errors],
            "summary": f"Projected fit: max {max(check_errors):.2f}px / RMS {rms_m:.0f}m (leave-one-out)",
        }


def navigable_inset_diagnostics(source_path, control_points):
    try:
        controls = [normalize_control(point, allow_incomplete=True) for point in control_points]
        complete = [point for point in controls if complete_control(point)]
        return InsetGeoreference(source_path, complete).diagnostics
    except GeoreferenceError as error:
        return {"ready": False, "summary": str(error)}


def build_inset(source_path, inset, output_path):
    """Emit a cropped, georeferenced VRT and its clipped Web Mercator warp."""
    source_path = Path(source_path).resolve()
    output_path = Path(output_path).resolve()
    fit = InsetGeoreference(source_path, inset["control_points"])
    boundary = np.array(inset["boundary"], dtype=float)
    left, top = np.floor(boundary.min(axis=0)).astype(int)
    right, bottom = np.ceil(boundary.max(axis=0)).astype(int)
    source = gdal.Open(str(source_path))
    if not (0 <= left < right <= source.RasterXSize and 0 <= top < bottom <= source.RasterYSize):
        raise GeoreferenceError("Inset boundary exceeds source bounds")
    crop_path = output_path.with_name(output_path.stem + "-source.vrt")
    cutline_path = output_path.with_suffix(".gpkg")
    expand = "rgb" if source.GetRasterBand(1).GetColorTable() is not None else None
    crop = gdal.Translate(str(crop_path), source, format="VRT",
                          srcWin=[int(left), int(top), int(right-left), int(bottom-top)],
                          rgbExpand=expand)
    origin = gdal.ApplyGeoTransform(fit.transform, float(left), float(top))
    crop.SetGeoTransform((origin[0], *fit.transform[1:3], origin[1], *fit.transform[4:6]))
    crop.SetProjection(fit.srs.ExportToWkt())
    crop.SetGCPs([], "")
    crop = None
    source = None

    # Store the cutline in the fitted plane, where its pixel edges are straight.
    # Transforming just its vertices to lon/lat would incorrectly straighten curves.
    driver = ogr.GetDriverByName("GPKG")
    if cutline_path.exists():
        driver.DeleteDataSource(str(cutline_path))
    cutline = driver.CreateDataSource(str(cutline_path))
    layer = cutline.CreateLayer("cutline", srs=fit.srs, geom_type=ogr.wkbPolygon)
    ring = ogr.Geometry(ogr.wkbLinearRing)
    for pixel in boundary:
        ring.AddPoint_2D(*gdal.ApplyGeoTransform(fit.transform, *pixel))
    ring.CloseRings()
    polygon = ogr.Geometry(ogr.wkbPolygon)
    polygon.AddGeometry(ring)
    if not polygon.IsValid():
        raise GeoreferenceError("Inset boundary is not a valid polygon")
    feature = ogr.Feature(layer.GetLayerDefn())
    feature.SetGeometry(polygon)
    layer.CreateFeature(feature)
    feature = layer = cutline = None
    result = gdal.Warp(str(output_path), str(crop_path), format="VRT",
                       dstSRS="EPSG:3857", resampleAlg="cubicspline", dstNodata=51,
                       cutlineDSName=str(cutline_path), cropToCutline=True, errorThreshold=0)
    if result is None:
        raise GeoreferenceError("Inset warp failed")
    result = None
    output_path.with_suffix(".fit.json").write_text(json.dumps(fit.diagnostics, indent=2) + "\n")
    return fit.diagnostics


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    print(json.dumps(build_inset(args.source, json.load(sys.stdin), args.output)))


if __name__ == "__main__":
    main()
