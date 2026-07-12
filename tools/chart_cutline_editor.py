#!/usr/bin/env python3
"""Local browser editor for chart cutline GeoJSON files."""

from __future__ import annotations

import argparse
import json
import math
import os
import tempfile
import threading
import uuid
from dataclasses import dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

import numpy as np
from osgeo import gdal, osr

gdal.UseExceptions()

try:
    from chart_cutline_audit import (
        DEFAULT_CHART_METADATA_ROOT,
        geojson_srs,
        project_to_pixel,
        read_geojson_polygons,
        set_traditional_axis_mapping,
        slug,
    )
except ImportError:
    from tools.chart_cutline_audit import (
        DEFAULT_CHART_METADATA_ROOT,
        geojson_srs,
        project_to_pixel,
        read_geojson_polygons,
        set_traditional_axis_mapping,
        slug,
    )


ASSET_DIR = Path(__file__).with_name("chart_cutline_editor_assets")
DEFAULT_CACHE_DIR = Path("/tmp/aerobag-chart-cutline-editor")
MAX_BODY_BYTES = 2 * 1024 * 1024
MAX_CROP_DIMENSION = 8192
MAX_CROP_PIXELS = 16 * 1024 * 1024
OVERVIEW_RENDER_VERSION = 2
FAMILY_LABELS = {
    "SEC": "Sectional",
    "TAC": "TAC",
    "ENR_L": "IFR-L",
    "ENR_H": "IFR-H",
}


@dataclass(frozen=True)
class Chart:
    name: str
    source_path: Path
    cutline_path: Path
    width: int
    height: int


class EditorError(RuntimeError):
    pass


class RevisionConflict(EditorError):
    pass


class EditorCatalog:
    def __init__(self, families: dict[str, "EditorState"]) -> None:
        if not families:
            raise EditorError("editor requires at least one chart family")
        self.families = families

    def family(self, family_id: str) -> "EditorState":
        state = self.families.get(family_id)
        if state is None:
            raise EditorError(f"unknown chart family {family_id!r}")
        return state

    def family_list(self) -> list[dict[str, object]]:
        return [
            {
                "id": family_id,
                "label": FAMILY_LABELS.get(family_id, family_id),
                "chart_count": len(state.charts),
            }
            for family_id, state in self.families.items()
        ]

    def chart_payload(self, family_id: str, name: str) -> dict[str, object]:
        payload = self.family(family_id).chart_payload(name)
        payload["family"] = family_id
        payload["overview_url"] = (
            f"/api/overview?family={quote_query_value(family_id)}"
            f"&name={quote_query_value(name)}"
        )
        return payload


class EditorState:
    def __init__(
        self,
        work_dir: Path,
        cutline_dir: Path,
        cache_dir: Path,
        overview_width: int,
    ) -> None:
        self.work_dir = work_dir.resolve()
        self.cutline_dir = cutline_dir.resolve()
        self.cache_dir = cache_dir.resolve()
        self.overview_width = overview_width
        self.cache_dir.mkdir(parents=True, exist_ok=True)
        self._write_lock = threading.Lock()
        self._overview_locks: dict[str, threading.Lock] = {}
        self.charts = self._discover_charts()

    def _discover_charts(self) -> dict[str, Chart]:
        charts: dict[str, Chart] = {}
        for cutline_path in sorted(self.cutline_dir.glob("*.geojson")):
            source_path = self.work_dir / f"{cutline_path.stem}.tif"
            if not source_path.is_file():
                continue
            dataset = gdal.Open(str(source_path))
            if dataset is None:
                raise EditorError(f"failed to open source chart {source_path}")
            chart = Chart(
                name=cutline_path.stem,
                source_path=source_path,
                cutline_path=cutline_path,
                width=dataset.RasterXSize,
                height=dataset.RasterYSize,
            )
            charts[chart.name] = chart
        if not charts:
            raise EditorError(
                f"no matching GeoTIFF/cutline pairs in {self.work_dir} and {self.cutline_dir}"
            )
        return charts

    def chart(self, name: str) -> Chart:
        chart = self.charts.get(name)
        if chart is None:
            raise EditorError(f"unknown chart {name!r}")
        return chart

    def chart_list(self) -> list[dict[str, object]]:
        return [
            {
                "name": chart.name,
                "width": chart.width,
                "height": chart.height,
            }
            for chart in self.charts.values()
        ]

    def chart_payload(self, name: str) -> dict[str, object]:
        chart = self.chart(name)
        points = self._pixel_points(chart)
        return {
            "name": chart.name,
            "width": chart.width,
            "height": chart.height,
            "points": [[x, y] for x, y in points],
            "revision": file_revision(chart.cutline_path),
            "overview_url": f"/api/overview?name={quote_query_value(chart.name)}",
            "source_file": chart.source_path.name,
            "cutline_file": chart.cutline_path.name,
        }

    def legend_payload(self, name: str) -> dict[str, object]:
        chart = self.chart(name)
        path = self.legend_path(chart)
        if not path.is_file():
            return {
                "name": chart.name,
                "source": chart.source_path.name,
                "source_width": chart.width,
                "source_height": chart.height,
                "max_output_width": 1210,
                "regions": [],
                "revision": None,
            }
        document = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(document, dict) or document.get("schema_version") != 1:
            raise EditorError(f"unsupported legend layout schema in {path.name}")
        if document.get("source") != chart.source_path.name:
            raise EditorError(f"legend layout source mismatch in {path.name}")
        if document.get("source_width") != chart.width or document.get("source_height") != chart.height:
            raise EditorError(
                f"legend layout dimensions in {path.name} do not match {chart.source_path.name}"
            )
        regions = validate_legend_regions(document.get("regions"), chart)
        max_output_width = validate_max_output_width(document.get("max_output_width", 1210))
        return {
            "name": chart.name,
            "source": chart.source_path.name,
            "source_width": chart.width,
            "source_height": chart.height,
            "max_output_width": max_output_width,
            "regions": regions,
            "revision": file_revision(path),
        }

    def save_legend(
        self,
        name: str,
        regions_value: object,
        max_output_width_value: object,
        expected_revision: object,
    ) -> dict[str, object]:
        chart = self.chart(name)
        regions = validate_legend_regions(regions_value, chart)
        max_output_width = validate_max_output_width(max_output_width_value)
        path = self.legend_path(chart)
        if expected_revision is not None and not isinstance(expected_revision, str):
            raise EditorError("legend save revision must be a string or null")
        with self._write_lock:
            current_revision = file_revision(path) if path.is_file() else None
            if current_revision != expected_revision:
                raise RevisionConflict(
                    f"{path.name} changed on disk; reload before saving"
                )
            document = {
                "schema_version": 1,
                "source": chart.source_path.name,
                "source_width": chart.width,
                "source_height": chart.height,
                "max_output_width": max_output_width,
                "regions": regions,
            }
            atomic_write_json(path, document)
            revision = file_revision(path)
        return {"revision": revision, "regions": regions}

    def legend_path(self, chart: Chart) -> Path:
        return self.cutline_dir / f"{chart.name}.legend.json"

    def _pixel_points(self, chart: Chart) -> list[tuple[float, float]]:
        polygons = read_geojson_polygons(chart.cutline_path)
        if len(polygons) != 1:
            raise EditorError(
                f"{chart.cutline_path.name} has {len(polygons)} polygons; editor requires one"
            )
        dataset = self._open_dataset(chart)
        cutline_srs = geojson_srs(chart.cutline_path, polygons)
        set_traditional_axis_mapping(cutline_srs)
        image_srs = image_spatial_reference(dataset)
        transform = osr.CoordinateTransformation(cutline_srs, image_srs)
        inverse_gt = gdal.InvGeoTransform(dataset.GetGeoTransform())
        if inverse_gt is None:
            raise EditorError(f"failed to invert geotransform for {chart.source_path}")
        points = [project_to_pixel(transform, inverse_gt, point) for point in polygons[0]]
        if len(points) > 1 and point_distance(points[0], points[-1]) < 0.01:
            points.pop()
        return points

    def save_points(
        self,
        name: str,
        points_value: object,
        expected_revision: object,
    ) -> dict[str, object]:
        chart = self.chart(name)
        points = validate_pixel_points(points_value, chart)
        if not isinstance(expected_revision, str):
            raise EditorError("save request is missing revision")

        with self._write_lock:
            current_revision = file_revision(chart.cutline_path)
            if current_revision != expected_revision:
                raise RevisionConflict(
                    f"{chart.cutline_path.name} changed on disk; reload before saving"
                )
            document = json.loads(chart.cutline_path.read_text(encoding="utf-8"))
            feature = single_polygon_feature(document, chart.cutline_path)
            cutline_points = self._pixel_points_to_cutline(chart, points)
            closed_ring = [[x, y] for x, y in cutline_points]
            closed_ring.append(closed_ring[0].copy())
            feature["geometry"]["coordinates"][0] = closed_ring
            atomic_write_json(chart.cutline_path, document)
            revision = file_revision(chart.cutline_path)

        return {"revision": revision, "points": [[x, y] for x, y in points]}

    def _pixel_points_to_cutline(
        self,
        chart: Chart,
        points: list[tuple[float, float]],
    ) -> list[tuple[float, float]]:
        dataset = self._open_dataset(chart)
        polygons = read_geojson_polygons(chart.cutline_path)
        cutline_srs = geojson_srs(chart.cutline_path, polygons)
        set_traditional_axis_mapping(cutline_srs)
        image_srs = image_spatial_reference(dataset)
        transform = osr.CoordinateTransformation(image_srs, cutline_srs)
        gt = dataset.GetGeoTransform()
        result: list[tuple[float, float]] = []
        for pixel_x, pixel_y in points:
            image_x = gt[0] + gt[1] * pixel_x + gt[2] * pixel_y
            image_y = gt[3] + gt[4] * pixel_x + gt[5] * pixel_y
            cutline_x, cutline_y, _ = transform.TransformPoint(image_x, image_y)
            result.append((cutline_x, cutline_y))
        return result

    def overview_png(self, name: str) -> bytes:
        chart = self.chart(name)
        source_revision = file_revision(chart.source_path)
        cache_path = self.cache_dir / (
            f"{slug(chart.name)}-{source_revision}-{self.overview_width}"
            f"-v{OVERVIEW_RENDER_VERSION}.png"
        )
        lock = self._overview_locks.setdefault(chart.name, threading.Lock())
        with lock:
            if not cache_path.is_file():
                render_overview_png(chart.source_path, cache_path, self.overview_width)
                remove_aux_xml(cache_path)
        return cache_path.read_bytes()

    def crop_png(
        self,
        name: str,
        x: int,
        y: int,
        width: int,
        height: int,
    ) -> bytes:
        chart = self.chart(name)
        if width < 1 or height < 1:
            raise EditorError("crop dimensions must be positive")
        if width > MAX_CROP_DIMENSION or height > MAX_CROP_DIMENSION:
            raise EditorError(
                f"crop dimensions must not exceed {MAX_CROP_DIMENSION} pixels"
            )
        if width * height > MAX_CROP_PIXELS:
            raise EditorError(
                f"crop area must not exceed {MAX_CROP_PIXELS} source pixels"
            )
        if x < 0 or y < 0 or x + width > chart.width or y + height > chart.height:
            raise EditorError("crop falls outside source chart")
        vsi_path = f"/vsimem/cutline-editor-{uuid.uuid4().hex}.png"
        try:
            options = translate_png_options(
                chart.source_path,
                source_window=[x, y, width, height],
            )
            result = gdal.Translate(vsi_path, str(chart.source_path), options=options)
            if result is None:
                raise EditorError(f"failed to render crop for {chart.name}")
            result = None
            data = read_vsimem_file(vsi_path)
            if data is None:
                raise EditorError(f"failed to read rendered crop for {chart.name}")
            return data
        finally:
            gdal.Unlink(vsi_path)

    def snap_point(
        self,
        name: str,
        point_value: object,
        radius: int,
    ) -> dict[str, object]:
        chart = self.chart(name)
        point = validate_point(point_value)
        radius = max(48, min(radius, 384))
        x0 = max(0, int(math.floor(point[0])) - radius)
        y0 = max(0, int(math.floor(point[1])) - radius)
        x1 = min(chart.width, int(math.floor(point[0])) + radius + 1)
        y1 = min(chart.height, int(math.floor(point[1])) + radius + 1)
        rgb = read_rgb_crop(chart.source_path, x0, y0, x1 - x0, y1 - y0)
        local_target = (point[0] - x0, point[1] - y0)
        candidate = find_snap_candidate(rgb, local_target, min(radius - 4, 220))
        if candidate is None:
            raise EditorError("no convincing whitespace corner found near this vertex")
        local_x, local_y, confidence = candidate
        snapped = (x0 + local_x, y0 + local_y)
        return {
            "point": [snapped[0], snapped[1]],
            "confidence": confidence,
            "distance": point_distance(point, snapped),
        }

    @staticmethod
    def _open_dataset(chart: Chart) -> gdal.Dataset:
        dataset = gdal.Open(str(chart.source_path))
        if dataset is None:
            raise EditorError(f"failed to open source chart {chart.source_path}")
        return dataset


def image_spatial_reference(dataset: gdal.Dataset) -> osr.SpatialReference:
    srs = osr.SpatialReference()
    if srs.ImportFromWkt(dataset.GetProjection()) != 0:
        raise EditorError("source chart has an invalid projection")
    set_traditional_axis_mapping(srs)
    return srs


def translate_png_options(
    source_path: Path,
    *,
    width: int | None = None,
    source_window: list[int] | None = None,
) -> gdal.TranslateOptions:
    dataset = gdal.Open(str(source_path))
    if dataset is None:
        raise EditorError(f"failed to open {source_path}")
    kwargs: dict[str, object] = {"format": "PNG"}
    first_band = dataset.GetRasterBand(1)
    if dataset.RasterCount == 1 and first_band.GetColorTable() is not None:
        kwargs["rgbExpand"] = "rgb"
    if width is not None:
        kwargs.update(width=width, height=0, resampleAlg="average")
    if source_window is not None:
        kwargs["srcWin"] = source_window
    return gdal.TranslateOptions(**kwargs)


def render_overview_png(source_path: Path, output_path: Path, width: int) -> None:
    dataset = gdal.Open(str(source_path))
    if dataset is None:
        raise EditorError(f"failed to open {source_path}")
    first_band = dataset.GetRasterBand(1)
    is_paletted = dataset.RasterCount == 1 and first_band.GetColorTable() is not None
    if not is_paletted:
        options = gdal.TranslateOptions(
            format="PNG",
            width=width,
            height=0,
            resampleAlg="average",
        )
        result = gdal.Translate(str(output_path), dataset, options=options)
        if result is None:
            raise EditorError(f"failed to generate overview for {source_path.name}")
        result = None
        return

    rgb_vrt_path = f"/vsimem/cutline-editor-overview-{uuid.uuid4().hex}.vrt"
    try:
        expanded = gdal.Translate(
            rgb_vrt_path,
            dataset,
            options=gdal.TranslateOptions(format="VRT", rgbExpand="rgb"),
        )
        if expanded is None:
            raise EditorError(f"failed to expand {source_path.name} to RGB")
        expanded = None
        result = gdal.Warp(
            str(output_path),
            rgb_vrt_path,
            options=gdal.WarpOptions(
                format="PNG",
                width=width,
                height=0,
                resampleAlg="average",
            ),
        )
        if result is None:
            raise EditorError(f"failed to generate overview for {source_path.name}")
        result = None
    finally:
        gdal.Unlink(rgb_vrt_path)


def remove_aux_xml(path: Path) -> None:
    aux_path = path.with_name(path.name + ".aux.xml")
    if aux_path.exists():
        aux_path.unlink()


def read_vsimem_file(path: str) -> bytes | None:
    if hasattr(gdal, "VSIGetMemFileBuffer"):
        data = gdal.VSIGetMemFileBuffer(path, False)
    else:
        data = gdal.VSIGetMemFileBuffer_unsafe(path)
    return None if data is None else bytes(data)


def file_revision(path: Path) -> str:
    stat = path.stat()
    return f"{stat.st_mtime_ns:x}-{stat.st_size:x}"


def point_distance(
    left: tuple[float, float],
    right: tuple[float, float],
) -> float:
    return math.hypot(left[0] - right[0], left[1] - right[1])


def validate_point(value: object) -> tuple[float, float]:
    if not isinstance(value, list) or len(value) != 2:
        raise EditorError("point must be a two-element array")
    try:
        point = (float(value[0]), float(value[1]))
    except (TypeError, ValueError) as error:
        raise EditorError("point coordinates must be numbers") from error
    if not all(math.isfinite(component) for component in point):
        raise EditorError("point coordinates must be finite")
    return point


def validate_pixel_points(value: object, chart: Chart) -> list[tuple[float, float]]:
    if not isinstance(value, list):
        raise EditorError("points must be an array")
    if len(value) < 3:
        raise EditorError("a cutline requires at least three vertices")
    if len(value) > 5000:
        raise EditorError("cutline has too many vertices")
    points = [validate_point(point) for point in value]
    for x, y in points:
        if x < -chart.width or x > chart.width * 2:
            raise EditorError("cutline x coordinate is implausibly far outside the chart")
        if y < -chart.height or y > chart.height * 2:
            raise EditorError("cutline y coordinate is implausibly far outside the chart")
    return points


def validate_legend_regions(value: object, chart: Chart) -> list[dict[str, int]]:
    if not isinstance(value, list):
        raise EditorError("legend regions must be an array")
    if len(value) > 100:
        raise EditorError("legend layout has too many regions")
    result = []
    for index, region in enumerate(value):
        if not isinstance(region, dict):
            raise EditorError(f"legend region {index + 1} must be an object")
        parsed: dict[str, int] = {}
        for key in ("x", "y", "width", "height"):
            raw = region.get(key)
            if isinstance(raw, bool):
                raise EditorError(f"legend region {index + 1} {key} must be a number")
            try:
                number = float(raw)
            except (TypeError, ValueError) as error:
                raise EditorError(
                    f"legend region {index + 1} {key} must be a number"
                ) from error
            if not math.isfinite(number):
                raise EditorError(f"legend region {index + 1} {key} must be finite")
            parsed[key] = int(round(number))
        if parsed["width"] < 1 or parsed["height"] < 1:
            raise EditorError(f"legend region {index + 1} must have positive dimensions")
        if parsed["x"] < 0 or parsed["y"] < 0:
            raise EditorError(f"legend region {index + 1} starts outside the source chart")
        if parsed["x"] + parsed["width"] > chart.width:
            raise EditorError(f"legend region {index + 1} exceeds the source chart width")
        if parsed["y"] + parsed["height"] > chart.height:
            raise EditorError(f"legend region {index + 1} exceeds the source chart height")
        result.append(parsed)
    return result


def validate_max_output_width(value: object) -> int:
    if isinstance(value, bool):
        raise EditorError("maximum output width must be an integer")
    try:
        width = int(value)
    except (TypeError, ValueError) as error:
        raise EditorError("maximum output width must be an integer") from error
    if width < 320 or width > 4096:
        raise EditorError("maximum output width must be between 320 and 4096")
    return width


def single_polygon_feature(document: object, path: Path) -> dict[str, object]:
    if not isinstance(document, dict) or document.get("type") != "FeatureCollection":
        raise EditorError(f"unsupported GeoJSON root in {path.name}")
    features = document.get("features")
    if not isinstance(features, list) or len(features) != 1:
        raise EditorError(f"{path.name} must contain exactly one feature")
    feature = features[0]
    if not isinstance(feature, dict):
        raise EditorError(f"invalid feature in {path.name}")
    geometry = feature.get("geometry")
    if not isinstance(geometry, dict) or geometry.get("type") != "Polygon":
        raise EditorError(f"{path.name} must contain one Polygon")
    coordinates = geometry.get("coordinates")
    if not isinstance(coordinates, list) or len(coordinates) != 1:
        raise EditorError(f"{path.name} must contain one exterior ring and no holes")
    return feature


def atomic_write_json(path: Path, document: object) -> None:
    with tempfile.NamedTemporaryFile(
        mode="w",
        encoding="utf-8",
        dir=path.parent,
        prefix=f".{path.name}.",
        suffix=".tmp",
        delete=False,
    ) as output:
        temp_path = Path(output.name)
        json.dump(document, output, indent=2, ensure_ascii=True)
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())
    try:
        os.replace(temp_path, path)
    finally:
        if temp_path.exists():
            temp_path.unlink()


def read_rgb_crop(path: Path, x: int, y: int, width: int, height: int) -> np.ndarray:
    dataset = gdal.Open(str(path))
    if dataset is None:
        raise EditorError(f"failed to open {path}")
    if dataset.RasterCount == 1:
        band = dataset.GetRasterBand(1)
        values = band.ReadAsArray(x, y, width, height)
        color_table = band.GetColorTable()
        if color_table is None:
            gray = values.astype(np.uint8, copy=False)
            return np.repeat(gray[:, :, np.newaxis], 3, axis=2)
        lut = np.zeros((color_table.GetCount(), 3), dtype=np.uint8)
        for index in range(color_table.GetCount()):
            entry = color_table.GetColorEntry(index)
            if entry is not None:
                lut[index] = entry[:3]
        return lut[values]
    bands = [
        dataset.GetRasterBand(index).ReadAsArray(x, y, width, height)
        for index in range(1, min(dataset.RasterCount, 3) + 1)
    ]
    if len(bands) == 2:
        bands.append(bands[-1])
    return np.stack(bands[:3], axis=2).astype(np.uint8, copy=False)


def find_snap_candidate(
    rgb: np.ndarray,
    target: tuple[float, float],
    max_distance: int,
) -> tuple[float, float, float] | None:
    """Find a nearby intersection of long white/non-white raster boundaries."""
    if rgb.ndim != 3 or rgb.shape[2] < 3 or min(rgb.shape[:2]) < 24:
        return None
    height, width = rgb.shape[:2]
    target_x = int(round(target[0]))
    target_y = int(round(target[1]))
    if target_x < 0 or target_y < 0 or target_x >= width or target_y >= height:
        return None

    rgb16 = rgb[:, :, :3].astype(np.int16)
    white = (
        (rgb16.min(axis=2) >= 242)
        & ((rgb16.max(axis=2) - rgb16.min(axis=2)) <= 24)
    ).astype(np.float32)
    gray = (
        rgb16[:, :, 0] * 30 + rgb16[:, :, 1] * 59 + rgb16[:, :, 2] * 11
    ) / 100.0
    edge_x = np.zeros((height, width), dtype=np.float32)
    edge_y = np.zeros((height, width), dtype=np.float32)
    edge_x[:, 1:-1] = np.abs(gray[:, 2:] - gray[:, :-2]) / 255.0
    edge_y[1:-1, :] = np.abs(gray[2:, :] - gray[:-2, :]) / 255.0

    vertical = np.zeros(width, dtype=np.float32)
    horizontal = np.zeros(height, dtype=np.float32)
    strip = 7
    split_pad = 3
    for x in range(strip, width - strip):
        left = white[:, x - strip : x - split_pad]
        right = white[:, x + split_pad : x + strip]
        top_contrast = abs(float(left[:target_y].mean()) - float(right[:target_y].mean())) if target_y > 8 else 0.0
        bottom_contrast = abs(float(left[target_y:].mean()) - float(right[target_y:].mean())) if height - target_y > 8 else 0.0
        top_edge = float(edge_x[:target_y, x].mean()) if target_y > 8 else 0.0
        bottom_edge = float(edge_x[target_y:, x].mean()) if height - target_y > 8 else 0.0
        vertical[x] = 0.78 * max(top_contrast, bottom_contrast) + 0.22 * max(top_edge, bottom_edge)
    for y in range(strip, height - strip):
        above = white[y - strip : y - split_pad, :]
        below = white[y + split_pad : y + strip, :]
        left_contrast = abs(float(above[:, :target_x].mean()) - float(below[:, :target_x].mean())) if target_x > 8 else 0.0
        right_contrast = abs(float(above[:, target_x:].mean()) - float(below[:, target_x:].mean())) if width - target_x > 8 else 0.0
        left_edge = float(edge_y[y, :target_x].mean()) if target_x > 8 else 0.0
        right_edge = float(edge_y[y, target_x:].mean()) if width - target_x > 8 else 0.0
        horizontal[y] = 0.78 * max(left_contrast, right_contrast) + 0.22 * max(left_edge, right_edge)

    x_candidates = strongest_local_candidates(vertical, target_x, max_distance, 12)
    y_candidates = strongest_local_candidates(horizontal, target_y, max_distance, 12)
    if not x_candidates or not y_candidates:
        return None

    best: tuple[float, int, int, float] | None = None
    for x in x_candidates:
        for y in y_candidates:
            distance = math.hypot(x - target[0], y - target[1])
            if distance > max_distance:
                continue
            quadrant = quadrant_whitespace_spread(white, x, y, 12)
            axis_strength = min(float(vertical[x]), float(horizontal[y]))
            score = (
                float(vertical[x])
                + float(horizontal[y])
                + 0.45 * quadrant
                - 0.30 * (distance / max_distance)
            )
            candidate = (score, x, y, axis_strength)
            if best is None or candidate > best:
                best = candidate
    if best is None or best[3] < 0.025:
        return None
    confidence = max(0.0, min(1.0, best[0] / 2.0))
    return float(best[1]), float(best[2]), confidence


def strongest_local_candidates(
    scores: np.ndarray,
    center: int,
    max_distance: int,
    limit: int,
) -> list[int]:
    start = max(1, center - max_distance)
    end = min(len(scores) - 1, center + max_distance + 1)
    candidates = [
        index
        for index in range(start, end)
        if scores[index] >= scores[index - 1] and scores[index] >= scores[index + 1]
    ]
    candidates.sort(key=lambda index: (float(scores[index]), -abs(index - center)), reverse=True)
    return candidates[:limit]


def quadrant_whitespace_spread(
    white: np.ndarray,
    x: int,
    y: int,
    radius: int,
) -> float:
    height, width = white.shape
    x0 = max(0, x - radius)
    x1 = min(width, x + radius)
    y0 = max(0, y - radius)
    y1 = min(height, y + radius)
    quadrants = [
        white[y0:y, x0:x],
        white[y0:y, x:x1],
        white[y:y1, x0:x],
        white[y:y1, x:x1],
    ]
    means = [float(value.mean()) for value in quadrants if value.size]
    return max(means) - min(means) if means else 0.0


def quote_query_value(value: str) -> str:
    from urllib.parse import quote

    return quote(value, safe="")


class EditorRequestHandler(BaseHTTPRequestHandler):
    state: EditorCatalog

    def do_GET(self) -> None:
        try:
            parsed = urlparse(self.path)
            query = parse_qs(parsed.query)
            if parsed.path == "/":
                self._send_file(ASSET_DIR / "index.html", "text/html; charset=utf-8")
            elif parsed.path in {"/legends", "/legends.html"}:
                self._send_file(ASSET_DIR / "legends.html", "text/html; charset=utf-8")
            elif parsed.path == "/assets/editor.css":
                self._send_file(ASSET_DIR / "editor.css", "text/css; charset=utf-8")
            elif parsed.path == "/assets/editor.js":
                self._send_file(ASSET_DIR / "editor.js", "text/javascript; charset=utf-8")
            elif parsed.path == "/assets/legends.css":
                self._send_file(ASSET_DIR / "legends.css", "text/css; charset=utf-8")
            elif parsed.path == "/assets/legends.js":
                self._send_file(ASSET_DIR / "legends.js", "text/javascript; charset=utf-8")
            elif parsed.path == "/api/families":
                self._send_json({"families": self.state.family_list()})
            elif parsed.path == "/api/charts":
                family = required_query(query, "family")
                self._send_json({"charts": self.state.family(family).chart_list()})
            elif parsed.path == "/api/chart":
                self._send_json(
                    self.state.chart_payload(
                        required_query(query, "family"),
                        required_query(query, "name"),
                    )
                )
            elif parsed.path == "/api/legend":
                family = required_query(query, "family")
                self._send_json(
                    self.state.family(family).legend_payload(required_query(query, "name"))
                )
            elif parsed.path == "/api/overview":
                family = required_query(query, "family")
                self._send_bytes(
                    self.state.family(family).overview_png(required_query(query, "name")),
                    "image/png",
                    cache_control="private, max-age=3600",
                )
            elif parsed.path == "/api/crop":
                family = required_query(query, "family")
                self._send_bytes(
                    self.state.family(family).crop_png(
                        required_query(query, "name"),
                        int_query(query, "x"),
                        int_query(query, "y"),
                        int_query(query, "width"),
                        int_query(query, "height"),
                    ),
                    "image/png",
                    cache_control="no-store",
                )
            else:
                self.send_error(404)
        except (EditorError, ValueError) as error:
            self._send_json({"error": str(error)}, status=400)
        except Exception as error:
            self.log_error("unhandled GET error: %s", error)
            self._send_json({"error": f"internal error: {error}"}, status=500)

    def do_POST(self) -> None:
        try:
            parsed = urlparse(self.path)
            body = self._read_json_body()
            family = self.state.family(str(body.get("family", "")))
            if parsed.path == "/api/save":
                result = family.save_points(
                    str(body.get("name", "")),
                    body.get("points"),
                    body.get("revision"),
                )
                self._send_json(result)
            elif parsed.path == "/api/snap":
                result = family.snap_point(
                    str(body.get("name", "")),
                    body.get("point"),
                    int(body.get("radius", 192)),
                )
                self._send_json(result)
            elif parsed.path == "/api/legend/save":
                result = family.save_legend(
                    str(body.get("name", "")),
                    body.get("regions"),
                    body.get("max_output_width", 1210),
                    body.get("revision"),
                )
                self._send_json(result)
            else:
                self.send_error(404)
        except RevisionConflict as error:
            self._send_json({"error": str(error)}, status=409)
        except (EditorError, ValueError, TypeError) as error:
            self._send_json({"error": str(error)}, status=400)
        except Exception as error:
            self.log_error("unhandled POST error: %s", error)
            self._send_json({"error": f"internal error: {error}"}, status=500)

    def _read_json_body(self) -> dict[str, object]:
        try:
            length = int(self.headers.get("Content-Length", "0"))
        except ValueError as error:
            raise EditorError("invalid Content-Length") from error
        if length <= 0 or length > MAX_BODY_BYTES:
            raise EditorError("invalid request body size")
        value = json.loads(self.rfile.read(length))
        if not isinstance(value, dict):
            raise EditorError("request body must be an object")
        return value

    def _send_file(self, path: Path, content_type: str) -> None:
        if not path.is_file():
            raise EditorError(f"editor asset missing: {path.name}")
        self._send_bytes(path.read_bytes(), content_type, cache_control="no-store")

    def _send_json(self, value: object, status: int = 200) -> None:
        self._send_bytes(
            json.dumps(value, separators=(",", ":")).encode("utf-8"),
            "application/json; charset=utf-8",
            status=status,
            cache_control="no-store",
        )

    def _send_bytes(
        self,
        body: bytes,
        content_type: str,
        *,
        status: int = 200,
        cache_control: str,
    ) -> None:
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", cache_control)
        self.send_header("X-Content-Type-Options", "nosniff")
        self.end_headers()
        try:
            self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def log_message(self, format: str, *args: object) -> None:
        if self.path.startswith("/api/save") or self.path.startswith("/api/snap"):
            super().log_message(format, *args)


def required_query(query: dict[str, list[str]], key: str) -> str:
    values = query.get(key)
    if not values or not values[0]:
        raise EditorError(f"missing query parameter {key}")
    return values[0]


def int_query(query: dict[str, list[str]], key: str) -> int:
    try:
        return int(required_query(query, key))
    except ValueError as error:
        raise EditorError(f"query parameter {key} must be an integer") from error


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-dir", type=Path)
    parser.add_argument("--chart-metadata-root", type=Path, default=DEFAULT_CHART_METADATA_ROOT)
    parser.add_argument("--family", default="TAC")
    parser.add_argument(
        "--family-work",
        action="append",
        default=[],
        metavar="FAMILY=WORK_DIR",
        help="add a chart family; may be repeated (SEC, TAC, ENR_L, ENR_H)",
    )
    parser.add_argument("--cache-dir", type=Path, default=DEFAULT_CACHE_DIR)
    parser.add_argument("--overview-width", type=int, default=1000)
    parser.add_argument("--host", default="0.0.0.0")
    parser.add_argument("--port", type=int, default=8585)
    return parser.parse_args()


def main() -> None:
    gdal.UseExceptions()
    args = parse_args()
    if not ASSET_DIR.is_dir():
        raise SystemExit(f"editor asset directory does not exist: {ASSET_DIR}")
    family_work = parse_family_work_args(args)
    families: dict[str, EditorState] = {}
    for family_id, work_dir in family_work:
        cutline_dir = (args.chart_metadata_root / family_id).resolve()
        if not work_dir.is_dir():
            raise SystemExit(f"work directory does not exist: {work_dir}")
        if not cutline_dir.is_dir():
            raise SystemExit(f"cutline directory does not exist: {cutline_dir}")
        families[family_id] = EditorState(
            work_dir,
            cutline_dir,
            args.cache_dir / family_id,
            args.overview_width,
        )
    catalog = EditorCatalog(families)
    handler = type("BoundEditorRequestHandler", (EditorRequestHandler,), {"state": catalog})
    server = ThreadingHTTPServer((args.host, args.port), handler)
    family_summary = ", ".join(
        f"{family_id}={len(state.charts)}" for family_id, state in families.items()
    )
    print(
        f"chart cutline editor: {family_summary} charts at "
        f"http://{args.host}:{args.port}/",
        flush=True,
    )
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


def parse_family_work_args(args: argparse.Namespace) -> list[tuple[str, Path]]:
    if args.family_work and args.work_dir is not None:
        raise SystemExit("use either --work-dir/--family or --family-work, not both")
    raw_values = args.family_work
    if not raw_values:
        if args.work_dir is None:
            raise SystemExit("--work-dir or at least one --family-work is required")
        raw_values = [f"{args.family}={args.work_dir}"]

    result: list[tuple[str, Path]] = []
    seen: set[str] = set()
    for value in raw_values:
        family_id, separator, raw_path = value.partition("=")
        family_id = family_id.strip().upper().replace("-", "_")
        if not separator or not family_id or not raw_path:
            raise SystemExit(f"invalid --family-work {value!r}; expected FAMILY=WORK_DIR")
        if family_id not in FAMILY_LABELS:
            choices = ", ".join(FAMILY_LABELS)
            raise SystemExit(f"unknown chart family {family_id!r}; expected one of {choices}")
        if family_id in seen:
            raise SystemExit(f"duplicate --family-work for {family_id}")
        seen.add(family_id)
        result.append((family_id, Path(raw_path).expanduser().resolve()))
    return result


if __name__ == "__main__":
    main()
