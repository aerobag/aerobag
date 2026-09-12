#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Local browser editor for chart cutline GeoJSON files."""

from __future__ import annotations

import argparse
import importlib.util
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
from osgeo import gdal

gdal.UseExceptions()

# Import the same helper embedded in preprocessor-charts, not an editor-only fit.
_inset_spec = importlib.util.spec_from_file_location(
    "navigable_inset",
    Path(__file__).resolve().parents[1]
    / "product/preprocessor/preprocessor-charts/navigable_inset.py",
)
inset_georeference = importlib.util.module_from_spec(_inset_spec)
_inset_spec.loader.exec_module(inset_georeference)
navigable_inset_diagnostics = inset_georeference.navigable_inset_diagnostics

try:
    from chart_cutline_audit import (
        DEFAULT_CHART_METADATA_ROOT,
        cutlines,
        slug,
    )
except ImportError:
    from tools.chart_cutline_audit import (
        DEFAULT_CHART_METADATA_ROOT,
        cutlines,
        slug,
    )


ASSET_DIR = Path(__file__).with_name("chart_cutline_editor_assets")
DEFAULT_CACHE_DIR = Path("/tmp/aerobag-chart-cutline-editor")
MAX_BODY_BYTES = 2 * 1024 * 1024
MAX_CROP_DIMENSION = 8192
MAX_CROP_PIXELS = 16 * 1024 * 1024
MAX_OVERVIEW_DIMENSION = 4096
OVERVIEW_RENDER_VERSION = 4
FAMILY_LABELS = {
    "SEC": "Sectional",
    "TAC": "TAC",
    "FLY": "Flyway",
    "ENR_L": "IFR-L",
    "ENR_H": "IFR-H",
}
EXTRACT_TYPES = {"legend", "inset"}
NAVIGABLE_INSET_TYPE = "navigable-inset"
NAVIGABLE_INSET_SUFFIX = ".navigable-insets.json"
NAVIGABLE_INSET_CANDIDATES_FILE = "navigable-inset-candidates.json"


@dataclass(frozen=True)
class Chart:
    name: str
    source_path: Path
    cutline_path: Path | None
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
                "inset_targets": [{"id": target, "label": cutlines.INSET_TARGET_LABELS[target]}
                                  for target in cutlines.INSET_TARGETS[family_id]],
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
        if not 1 <= overview_width <= MAX_OVERVIEW_DIMENSION:
            raise EditorError(f"overview size must be between 1 and {MAX_OVERVIEW_DIMENSION}")
        self.overview_width = overview_width
        self.cache_dir.mkdir(parents=True, exist_ok=True)
        self._write_lock = threading.Lock()
        self._edit_contexts: dict[str, tuple[str, dict]] = {}
        self._overview_locks: dict[str, threading.Lock] = {}
        self.charts = self._discover_charts()
        self.navigable_inset_candidates = self._load_navigable_inset_candidates()

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

        for extract_type in sorted(EXTRACT_TYPES):
            for layout_path in sorted(self.cutline_dir.glob(f"*.{extract_type}.json")):
                document = json.loads(layout_path.read_text(encoding="utf-8"))
                source_name = document.get("source")
                if not isinstance(source_name, str) or Path(source_name).name != source_name:
                    raise EditorError(f"invalid {extract_type} source in {layout_path.name}")
                source_path = self.work_dir / source_name
                if not source_path.is_file():
                    continue
                name = Path(source_name).stem
                if name in charts:
                    continue
                dataset = gdal.Open(str(source_path))
                if dataset is None:
                    raise EditorError(f"failed to open source chart {source_path}")
                charts[name] = Chart(
                    name=name,
                    source_path=source_path,
                    cutline_path=None,
                    width=dataset.RasterXSize,
                    height=dataset.RasterYSize,
                )
        if not charts:
            raise EditorError(
                f"no editable chart sources in {self.work_dir} and {self.cutline_dir}"
            )
        return charts

    def chart(self, name: str) -> Chart:
        chart = self.charts.get(name)
        if chart is None:
            raise EditorError(f"unknown chart {name!r}")
        return chart

    def chart_list(self, include_extract_only: bool = False) -> list[dict[str, object]]:
        result = []
        for chart in self.charts.values():
            if not include_extract_only and chart.cutline_path is None:
                continue
            candidate_names = self.navigable_inset_candidates.get(chart.source_path.name, ())
            inset_path = self.navigable_inset_path(chart)
            regions = []
            if inset_path.is_file():
                document = json.loads(inset_path.read_text(encoding="utf-8"))
                regions = validate_navigable_inset_document(document, chart, inset_path)
            result.append({
                "name": chart.name,
                "width": chart.width,
                "height": chart.height,
                "navigable_inset_candidates": list(candidate_names),
                "navigable_inset_defined_count": len(regions),
                "navigable_inset_enabled_count": sum(
                    1 for region in regions if region["enabled"]
                ),
            })
        return result

    def _load_navigable_inset_candidates(self) -> dict[str, tuple[str, ...]]:
        path = self.cutline_dir / NAVIGABLE_INSET_CANDIDATES_FILE
        if not path.is_file():
            return {}
        document = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(document, dict) or document.get("schema_version") != 1:
            raise EditorError(f"unsupported navigable-inset candidate schema in {path}")
        charts = document.get("charts")
        if not isinstance(charts, list):
            raise EditorError(f"navigable-inset candidates must be an array in {path}")
        known_sources = {chart.source_path.name for chart in self.charts.values()}
        result: dict[str, tuple[str, ...]] = {}
        for index, value in enumerate(charts):
            if not isinstance(value, dict):
                raise EditorError(f"navigable-inset candidate {index + 1} must be an object")
            source = value.get("source")
            if not isinstance(source, str) or source not in known_sources:
                raise EditorError(
                    f"navigable-inset candidate {index + 1} names unknown source {source!r}"
                )
            if source in result:
                raise EditorError(f"duplicate navigable-inset candidate source {source!r}")
            insets = value.get("insets")
            if not isinstance(insets, list) or not insets:
                raise EditorError(f"navigable-inset candidate {source!r} needs inset names")
            names = tuple(
                name.strip()
                for name in insets
                if isinstance(name, str) and name.strip()
            )
            if len(names) != len(insets) or len(set(name.casefold() for name in names)) != len(names):
                raise EditorError(f"navigable-inset candidate {source!r} has invalid inset names")
            result[source] = names
        return result

    def chart_payload(self, name: str) -> dict[str, object]:
        chart = self.chart(name)
        with self._write_lock:
            revision = file_revision(chart.cutline_path) if chart.cutline_path else None
            context = self._edit_context(chart, revision) if chart.cutline_path else {"points": [], "outline": []}
        return {
            "name": chart.name,
            "width": chart.width,
            "height": chart.height,
            "points": context["points"],
            "outline": context["outline"],
            "revision": revision,
            "overview_url": f"/api/overview?name={quote_query_value(chart.name)}",
            "source_file": chart.source_path.name,
            "cutline_file": chart.cutline_path.name if chart.cutline_path else None,
            "cutline_editable": chart.cutline_path is not None,
        }

    def extract_payload(self, name: str, extract_type_value: object) -> dict[str, object]:
        extract_type = validate_extract_type(extract_type_value)
        chart = self.chart(name)
        path = self.extract_path(chart, extract_type)
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
            raise EditorError(f"unsupported {extract_type} layout schema in {path.name}")
        if document.get("source") != chart.source_path.name:
            raise EditorError(f"{extract_type} layout source mismatch in {path.name}")
        if document.get("source_width") != chart.width or document.get("source_height") != chart.height:
            raise EditorError(
                f"{extract_type} layout dimensions in {path.name} do not match {chart.source_path.name}"
            )
        regions = validate_extract_regions(document.get("regions"), chart, extract_type)
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

    def save_extract(
        self,
        name: str,
        extract_type_value: object,
        regions_value: object,
        max_output_width_value: object,
        expected_revision: object,
    ) -> dict[str, object]:
        extract_type = validate_extract_type(extract_type_value)
        chart = self.chart(name)
        regions = validate_extract_regions(regions_value, chart, extract_type)
        max_output_width = validate_max_output_width(max_output_width_value)
        path = self.extract_path(chart, extract_type)
        if expected_revision is not None and not isinstance(expected_revision, str):
            raise EditorError(f"{extract_type} save revision must be a string or null")
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
            if current_revision is not None:
                current_document = json.loads(path.read_text(encoding="utf-8"))
                coverage_source = current_document.get("coverage_source")
                if coverage_source is not None:
                    document["coverage_source"] = coverage_source
            atomic_write_json(path, document)
            revision = file_revision(path)
        return {"revision": revision, "regions": regions}

    def extract_path(self, chart: Chart, extract_type: str) -> Path:
        return self.cutline_dir / f"{chart.name}.{extract_type}.json"

    def navigable_inset_payload(self, name: str) -> dict[str, object]:
        chart = self.chart(name)
        path = self.navigable_inset_path(chart)
        # Authoring seed only: copying it into a new draft makes the choice explicit.
        source = gdal.Open(str(chart.source_path))
        draft_projection = source.GetProjection()
        source = None
        if not path.is_file():
            return {
                "name": chart.name,
                "source": chart.source_path.name,
                "source_width": chart.width,
                "source_height": chart.height,
                "regions": [],
                "new_inset_projection_wkt": draft_projection,
                "revision": None,
            }
        document = json.loads(path.read_text(encoding="utf-8"))
        regions = validate_navigable_inset_document(document, chart, path)
        return {
            "name": chart.name,
            "source": chart.source_path.name,
            "source_width": chart.width,
            "source_height": chart.height,
            "regions": [region_with_diagnostics(chart, region) for region in regions],
            "new_inset_projection_wkt": draft_projection,
            "revision": file_revision(path),
        }

    def save_navigable_insets(
        self,
        name: str,
        regions_value: object,
        expected_revision: object,
    ) -> dict[str, object]:
        chart = self.chart(name)
        path = self.navigable_inset_path(chart)
        if expected_revision is not None and not isinstance(expected_revision, str):
            raise EditorError("navigable-inset save revision must be a string or null")
        document = {
            "schema_version": cutlines.INSET_LAYOUT_SCHEMA,
            "source": chart.source_path.name,
            "source_width": chart.width,
            "source_height": chart.height,
            "insets": regions_value,
        }
        regions = validate_navigable_inset_document(document, chart, path)
        document["insets"] = regions
        with self._write_lock:
            revision = self._write_navigable_insets(path, document, expected_revision)
        return {
            "revision": revision,
            "regions": [region_with_diagnostics(chart, region) for region in regions],
        }

    def _write_navigable_insets(self, path, document, expected_revision):
        """Caller holds the editor write lock, including across composed edits."""
        current_revision = file_revision(path) if path.is_file() else None
        if current_revision != expected_revision:
            raise RevisionConflict(f"{path.name} changed on disk; reload before saving")
        atomic_write_json(path, document)
        return file_revision(path)

    def start_georeferencing(self, name, index, reference_revision, inset_revision):
        """Copy a saved reference boundary into a disabled map draft; keep access
        to the reference image until the operator validates its replacement.
        """
        chart = self.chart(name)
        with self._write_lock:
            reference = self.extract_payload(name, "inset")
            if reference["revision"] != reference_revision:
                raise RevisionConflict("Reference changed; reload before starting georeference")
            if isinstance(index, bool) or not isinstance(index, int) or not 0 <= index < len(reference["regions"]):
                raise EditorError("Select a saved reference region first")
            layout = self.navigable_inset_payload(name)
            if layout["revision"] != inset_revision:
                raise RevisionConflict("Map insets changed; reload before starting georeference")
            x, y, w, h = (reference["regions"][index][k] for k in ("x", "y", "width", "height"))
            boundary = [[x, y], [x+w, y], [x+w, y+h], [x, y+h]]
            existing = next((r for r in layout["regions"] if r["boundary"] == boundary), None)
            if existing:
                return {"region_id": existing["id"]}
            used = {r["id"].casefold() for r in layout["regions"]}
            number = 1
            while f"Reference {number}".casefold() in used:
                number += 1
            draft = {"id": f"Reference {number}", "enabled": False,
                     "target_family": cutlines.INSET_TARGETS[self.cutline_dir.name][0],
                     "projection_wkt": layout["new_inset_projection_wkt"],
                     "boundary": boundary, "control_points": []}
            path = self.navigable_inset_path(chart)
            document = {"schema_version": cutlines.INSET_LAYOUT_SCHEMA, "source": chart.source_path.name,
                        "source_width": chart.width, "source_height": chart.height,
                        "insets": [*layout["regions"], draft]}
            document["insets"] = validate_navigable_inset_document(document, chart, path)
            self._write_navigable_insets(path, document, inset_revision)
            return {"region_id": draft["id"]}

    def navigable_inset_path(self, chart: Chart) -> Path:
        return self.cutline_dir / f"{chart.name}{NAVIGABLE_INSET_SUFFIX}"

    def _edit_context(self, chart: Chart, revision: str) -> dict:
        if chart.cutline_path is None:
            raise EditorError(f"{chart.name} has no georeferenced cutline")
        cached = self._edit_contexts.get(chart.name)
        if cached is None or cached[0] != revision:
            cached = (revision, cutlines.edit_geometry(chart.cutline_path, self._open_dataset(chart)))
            self._edit_contexts[chart.name] = cached
        return cached[1]

    def preview_points(self, name: str, points_value: object, expected_revision: object) -> dict:
        chart = self.chart(name)
        points = validate_pixel_points(points_value, chart)
        with self._write_lock:
            if chart.cutline_path is None or file_revision(chart.cutline_path) != expected_revision:
                raise RevisionConflict("Cutline changed; reload before previewing edits")
            context = self._edit_context(chart, expected_revision)
            source = self._open_dataset(chart)
            document = cutlines.edited_document(context["document"], source, points, context["crs"])
            return {"outline": self._outline(document, source)}

    @staticmethod
    def _outline(document, source):
        geometry = cutlines.pixel_document(document, source)
        return [p[:2] for p in geometry.GetGeometryRef(0).GetGeometryRef(0).GetPoints()]

    def save_points(
        self,
        name: str,
        points_value: object,
        expected_revision: object,
    ) -> dict[str, object]:
        chart = self.chart(name)
        if chart.cutline_path is None:
            raise EditorError(f"{chart.name} has no georeferenced cutline")
        points = validate_pixel_points(points_value, chart)
        if not isinstance(expected_revision, str):
            raise EditorError("save request is missing revision")

        with self._write_lock:
            current_revision = file_revision(chart.cutline_path)
            if current_revision != expected_revision:
                raise RevisionConflict(
                    f"{chart.cutline_path.name} changed on disk; reload before saving"
                )
            context = self._edit_context(chart, current_revision)
            source = self._open_dataset(chart)
            document = cutlines.edited_document(context["document"], source, points, context["crs"])
            outline = self._outline(document, source)
            atomic_write_json(chart.cutline_path, document)
            revision = file_revision(chart.cutline_path)

        return {"revision": revision, "points": [[x, y] for x, y in points], "outline": outline}

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

    def crop_overview_png(self, name: str, x: int, y: int, width: int, height: int) -> bytes:
        chart = self.chart(name)
        validate_source_window(chart, x, y, width, height)
        cache_path = self.cache_dir / (
            f"{slug(chart.name)}-{file_revision(chart.source_path)}"
            f"-crop-{x}-{y}-{width}-{height}-{self.overview_width}"
            f"-v{OVERVIEW_RENDER_VERSION}.png"
        )
        lock = self._overview_locks.setdefault(chart.name, threading.Lock())
        with lock:
            if not cache_path.is_file():
                render_overview_png(
                    chart.source_path, cache_path, self.overview_width,
                    source_window=[x, y, width, height],
                )
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
        validate_source_window(chart, x, y, width, height)
        if width > MAX_CROP_DIMENSION or height > MAX_CROP_DIMENSION:
            raise EditorError(
                f"crop dimensions must not exceed {MAX_CROP_DIMENSION} pixels"
            )
        if width * height > MAX_CROP_PIXELS:
            raise EditorError(
                f"crop area must not exceed {MAX_CROP_PIXELS} source pixels"
            )
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


def validate_source_window(chart: Chart, x: int, y: int, width: int, height: int) -> None:
    if width < 1 or height < 1:
        raise EditorError("crop dimensions must be positive")
    if x < 0 or y < 0 or x + width > chart.width or y + height > chart.height:
        raise EditorError("crop falls outside source chart")


def render_overview_png(
    source_path: Path, output_path: Path, width: int,
    *, source_window: list[int] | None = None,
) -> None:
    dataset = gdal.Open(str(source_path))
    if dataset is None:
        raise EditorError(f"failed to open {source_path}")
    first_band = dataset.GetRasterBand(1)
    is_paletted = dataset.RasterCount == 1 and first_band.GetColorTable() is not None
    region_width, region_height = (
        source_window[2:] if source_window else (dataset.RasterXSize, dataset.RasterYSize)
    )
    scale = min(1.0, width / max(region_width, region_height))
    output_width = max(1, round(region_width * scale))
    output_height = max(1, round(region_height * scale))
    rgb_vrt_path = f"/vsimem/cutline-editor-overview-{uuid.uuid4().hex}.vrt"
    try:
        expanded = gdal.Translate(
            rgb_vrt_path,
            dataset,
            options=gdal.TranslateOptions(
                format="VRT", rgbExpand="rgb" if is_paletted else None,
                srcWin=source_window,
            ),
        )
        if expanded is None:
            raise EditorError(f"failed to expand {source_path.name} to RGB")
        # The editor overlays source-pixel geometry. Do not reproject or rotate it.
        # Expand palette indices before averaging; the VRT keeps the large source lazy.
        expanded.SetProjection("")
        expanded.SetGCPs([], "")
        expanded.SetGeoTransform((0.0, 1.0, 0.0, float(region_height), 0.0, -1.0))
        expanded.FlushCache()
        expanded = None
        result = gdal.Warp(
            str(output_path),
            rgb_vrt_path,
            options=gdal.WarpOptions(
                format="PNG",
                width=output_width,
                height=output_height,
                resampleAlg="average",
                warpMemoryLimit=32,
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


def validate_extract_type(value: object) -> str:
    if not isinstance(value, str) or value not in EXTRACT_TYPES:
        raise EditorError(f"extract type must be one of {', '.join(sorted(EXTRACT_TYPES))}")
    return value


def validate_extract_regions(
    value: object,
    chart: Chart,
    extract_type: str,
) -> list[dict[str, int]]:
    if not isinstance(value, list):
        raise EditorError(f"{extract_type} regions must be an array")
    if len(value) > 100:
        raise EditorError(f"{extract_type} layout has too many regions")
    result = []
    for index, region in enumerate(value):
        if not isinstance(region, dict):
            raise EditorError(f"{extract_type} region {index + 1} must be an object")
        parsed: dict[str, int] = {}
        for key in ("x", "y", "width", "height"):
            raw = region.get(key)
            if isinstance(raw, bool):
                raise EditorError(f"{extract_type} region {index + 1} {key} must be a number")
            try:
                number = float(raw)
            except (TypeError, ValueError) as error:
                raise EditorError(
                    f"{extract_type} region {index + 1} {key} must be a number"
                ) from error
            if not math.isfinite(number):
                raise EditorError(f"{extract_type} region {index + 1} {key} must be finite")
            parsed[key] = int(round(number))
        if parsed["width"] < 1 or parsed["height"] < 1:
            raise EditorError(f"{extract_type} region {index + 1} must have positive dimensions")
        if parsed["x"] < 0 or parsed["y"] < 0:
            raise EditorError(f"{extract_type} region {index + 1} starts outside the source chart")
        if parsed["x"] + parsed["width"] > chart.width:
            raise EditorError(f"{extract_type} region {index + 1} exceeds the source chart width")
        if parsed["y"] + parsed["height"] > chart.height:
            raise EditorError(f"{extract_type} region {index + 1} exceeds the source chart height")
        result.append(parsed)
    return result


def validate_navigable_inset_document(
    document: object,
    chart: Chart,
    path: Path,
) -> list[dict[str, object]]:
    if not isinstance(document, dict) or document.get("schema_version") != cutlines.INSET_LAYOUT_SCHEMA:
        raise EditorError(f"unsupported navigable-inset schema in {path.name}")
    if document.get("source") != chart.source_path.name:
        raise EditorError(f"navigable-inset source mismatch in {path.name}")
    if document.get("source_width") != chart.width or document.get("source_height") != chart.height:
        raise EditorError(
            f"navigable-inset dimensions in {path.name} do not match {chart.source_path.name}"
        )
    if "target_family" in document:
        raise EditorError(f"target_family belongs to each inset, not the source chart, in {path.name}")
    values = document.get("insets")
    if not isinstance(values, list):
        raise EditorError("navigable insets must be an array")
    if len(values) > 64:
        raise EditorError("navigable-inset layout has too many regions")

    regions: list[dict[str, object]] = []
    identifiers: set[str] = set()
    for index, value in enumerate(values):
        if not isinstance(value, dict):
            raise EditorError(f"navigable inset {index + 1} must be an object")
        identifier = value.get("id")
        if not isinstance(identifier, str) or not identifier.strip():
            raise EditorError(f"navigable inset {index + 1} needs a non-empty id")
        identifier = identifier.strip()
        if Path(identifier).name != identifier or len(identifier) > 80:
            raise EditorError(f"navigable inset {index + 1} has an invalid id")
        identity = identifier.casefold()
        if identity in identifiers:
            raise EditorError(f"duplicate navigable inset id {identifier!r}")
        identifiers.add(identity)

        target_family = value.get("target_family")
        allowed = cutlines.INSET_TARGETS.get(path.parent.name)
        if allowed is None:
            raise EditorError(f"unsupported source chart family {path.parent.name!r}")
        if target_family not in allowed:
            raise EditorError(f"navigable inset {identifier!r} target_family must be {' or '.join(allowed)}")
        requires_georeference = cutlines.inset_requires_georeference(value)

        boundary = validate_pixel_points(value.get("boundary"), chart)
        if len(boundary) < 3:
            raise EditorError(f"navigable inset {identifier!r} needs at least 3 boundary points")
        if any(
            x < 0.0 or x > chart.width or y < 0.0 or y > chart.height
            for x, y in boundary
        ):
            raise EditorError(
                f"navigable inset {identifier!r} boundary is outside the source raster"
            )
        enabled = value.get("enabled", False)
        if not isinstance(enabled, bool):
            raise EditorError(f"navigable inset {identifier!r} enabled must be boolean")
        controls_value = value.get("control_points", [])
        if not isinstance(controls_value, list):
            raise EditorError(
                f"navigable inset {identifier!r} control_points must be an array"
            )
        if len(controls_value) > 32:
            raise EditorError(f"navigable inset {identifier!r} has too many control points")
        control_points = [
            validate_navigable_control_point(
                point,
                chart,
                identifier,
                point_index,
                allow_incomplete=not enabled or not requires_georeference,
            )
            for point_index, point in enumerate(controls_value)
        ]
        projection_wkt = value.get("projection_wkt")
        if requires_georeference or projection_wkt is not None:
            try:
                inset_georeference.inset_projection(projection_wkt)
            except inset_georeference.GeoreferenceError as error:
                raise EditorError(f"inset {identifier!r}: {error}") from error
        region: dict[str, object] = {
            "id": identifier,
            "target_family": target_family,
            "enabled": enabled,
            "boundary": [[round(x, 3), round(y, 3)] for x, y in boundary],
            "control_points": control_points,
        }
        if projection_wkt is not None:
            region["projection_wkt"] = projection_wkt
        if enabled and requires_georeference:
            diagnostics = navigable_inset_diagnostics(chart.source_path, control_points, projection_wkt=projection_wkt)
            if not diagnostics["ready"]:
                raise EditorError(
                    f"enabled navigable inset {identifier!r}: {diagnostics['summary']}"
                )
        regions.append(region)
    return regions


def validate_navigable_control_point(
    value: object,
    chart: Chart,
    identifier: str,
    index: int,
    *,
    allow_incomplete: bool,
) -> dict[str, object]:
    if not isinstance(value, dict):
        raise EditorError(
            f"navigable inset {identifier!r} control point {index + 1} must be an object"
        )
    try:
        point = inset_georeference.normalize_control(value, allow_incomplete=allow_incomplete)
    except inset_georeference.GeoreferenceError as error:
        raise EditorError(f"inset {identifier!r} control {index + 1}: {error}") from error
    x, y = point["pixel"]
    if not (0.0 <= x <= chart.width and 0.0 <= y <= chart.height):
        raise EditorError(
            f"navigable inset {identifier!r} control point {index + 1} is outside the source raster"
        )
    point["pixel"] = [round(x, 3), round(y, 3)]
    for field in ("latitude", "longitude"):
        if point[field] is not None:
            point[field] = round(point[field], 9)
    return point


def region_with_diagnostics(chart: Chart, region: dict[str, object]) -> dict[str, object]:
    boundary = [tuple(point) for point in region["boundary"]]
    return {
        **region,
        **pixel_polygon_bounds(boundary),
        "diagnostics": navigable_inset_diagnostics(
            chart.source_path, region["control_points"], projection_wkt=region["projection_wkt"],
        ) if cutlines.inset_requires_georeference(region) else {
            "ready": True,
            "summary": "Exclude only: boundary is removed from the parent; no georeference or output layer required.",
        },
    }


def pixel_polygon_bounds(points: list[tuple[float, float]]) -> dict[str, int]:
    left = math.floor(min(point[0] for point in points))
    top = math.floor(min(point[1] for point in points))
    right = math.ceil(max(point[0] for point in points))
    bottom = math.ceil(max(point[1] for point in points))
    return {
        "x": left,
        "y": top,
        "width": max(1, right - left),
        "height": max(1, bottom - top),
    }




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
            elif parsed.path in {"/extracts", "/extracts.html"}:
                self._send_file(ASSET_DIR / "extracts.html", "text/html; charset=utf-8")
            elif parsed.path == "/assets/editor.css":
                self._send_file(ASSET_DIR / "editor.css", "text/css; charset=utf-8")
            elif parsed.path == "/assets/editor.js":
                self._send_file(ASSET_DIR / "editor.js", "text/javascript; charset=utf-8")
            elif parsed.path == "/assets/cutline-points.js":
                self._send_file(ASSET_DIR / "cutline-points.js", "text/javascript; charset=utf-8")
            elif parsed.path == "/assets/latest-preview.js":
                self._send_file(ASSET_DIR / "latest-preview.js", "text/javascript; charset=utf-8")
            elif parsed.path == "/assets/coordinate-input.js":
                self._send_file(ASSET_DIR / "coordinate-input.js", "text/javascript; charset=utf-8")
            elif parsed.path == "/assets/extracts.css":
                self._send_file(ASSET_DIR / "extracts.css", "text/css; charset=utf-8")
            elif parsed.path == "/assets/extracts.js":
                self._send_file(ASSET_DIR / "extracts.js", "text/javascript; charset=utf-8")
            elif parsed.path == "/api/families":
                self._send_json({"families": self.state.family_list()})
            elif parsed.path == "/api/charts":
                family = required_query(query, "family")
                self._send_json(
                    {
                        "charts": self.state.family(family).chart_list(
                            include_extract_only=query.get("purpose") == ["extract"]
                        )
                    }
                )
            elif parsed.path == "/api/chart":
                self._send_json(
                    self.state.chart_payload(
                        required_query(query, "family"),
                        required_query(query, "name"),
                    )
                )
            elif parsed.path == "/api/extract":
                family = required_query(query, "family")
                self._send_json(
                    self.state.family(family).extract_payload(
                        required_query(query, "name"),
                        required_query(query, "type"),
                    )
                )
            elif parsed.path == "/api/navigable-insets":
                family = required_query(query, "family")
                self._send_json(
                    self.state.family(family).navigable_inset_payload(
                        required_query(query, "name")
                    )
                )
            elif parsed.path == "/api/overview":
                family = required_query(query, "family")
                self._send_bytes(
                    self.state.family(family).overview_png(required_query(query, "name")),
                    "image/png",
                    cache_control="private, max-age=3600",
                )
            elif parsed.path in ("/api/crop", "/api/crop-overview"):
                family = required_query(query, "family")
                renderer = (
                    self.state.family(family).crop_overview_png
                    if parsed.path == "/api/crop-overview"
                    else self.state.family(family).crop_png
                )
                self._send_bytes(
                    renderer(
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
            elif parsed.path == "/api/preview":
                self._send_json(family.preview_points(str(body.get("name", "")), body.get("points"), body.get("revision")))
            elif parsed.path == "/api/snap":
                result = family.snap_point(
                    str(body.get("name", "")),
                    body.get("point"),
                    int(body.get("radius", 192)),
                )
                self._send_json(result)
            elif parsed.path == "/api/extract/save":
                result = family.save_extract(
                    str(body.get("name", "")),
                    body.get("type"),
                    body.get("regions"),
                    body.get("max_output_width", 1210),
                    body.get("revision"),
                )
                self._send_json(result)
            elif parsed.path == "/api/navigable-insets/save":
                result = family.save_navigable_insets(
                    str(body.get("name", "")),
                    body.get("regions"),
                    body.get("revision"),
                )
                self._send_json(result)
            elif parsed.path == "/api/extract/start-georeferencing":
                self._send_json(family.start_georeferencing(
                    str(body.get("name", "")), body.get("index"),
                    body.get("reference_revision"), body.get("inset_revision")))
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
        help="add a chart family; may be repeated (SEC, TAC, FLY, ENR_L, ENR_H)",
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
