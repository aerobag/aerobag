# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Reviewed chart pixels, compared in fixed source coordinates before tiling."""

import argparse
import base64
from contextlib import contextmanager
from datetime import datetime, timezone
import hashlib
import html
import importlib.util
import json
import math
import os
from pathlib import Path
import shutil
import sys
import tempfile
import uuid

SCHEMA = 1
REPORT_SCHEMA = 2
RECIPE = 1
POLICY = {
    "overview_pixels": 512,
    "control_pixels": 128,
    "context_fraction": 0.04,
    "blur_radius": 1,
    "pixel_difference": 20,
    "interior": {"warning": 0.04, "critical": 0.18},
    "boundary": {"warning": 0.10, "critical": 0.30},
    "control": {"warning": 0.04, "critical": 0.15},
}
SEVERITY = {"ok": 0, "warning": 1, "critical": 2}


def initialize_rendering():
    # Called only after recording the attempt, so missing native dependencies
    # cannot leave the previous successful check looking current.
    global np, gdal, ogr, osr, cutlines
    import numpy as np
    from osgeo import gdal, ogr, osr
    gdal.UseExceptions()
    ogr.UseExceptions()
    osr.UseExceptions()
    gdal.SetCacheMax(64 * 1024 * 1024)
    spec = importlib.util.spec_from_file_location("chart_cutlines", Path(__file__).with_name("chart_cutlines.py"))
    cutlines = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(cutlines)


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                     allow_nan=False).encode()).hexdigest()


def atomic_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, delete=False) as file:
        temporary = Path(file.name)
        json.dump(value, file, sort_keys=True, indent=2, allow_nan=False)
        file.write("\n")
        file.flush()
        os.fsync(file.fileno())
    temporary.replace(path)


def now():
    return datetime.now(timezone.utc).isoformat()


@contextmanager
def vsi_file():
    name = f"/vsimem/chart-quality-{uuid.uuid4().hex}.png"
    try:
        yield name
    finally:
        gdal.Unlink(name)


def png_bytes(pixels):
    pixels = np.asarray(pixels, dtype=np.uint8)
    if pixels.ndim == 2:
        pixels = pixels[None, :, :]
    bands, height, width = pixels.shape
    dataset = gdal.GetDriverByName("MEM").Create("", width, height, bands, gdal.GDT_Byte)
    for i, band in enumerate(pixels, 1):
        dataset.GetRasterBand(i).WriteArray(band)
    with vsi_file() as name:
        encoded = gdal.GetDriverByName("PNG").CreateCopy(name, dataset)
        encoded = None
        return bytes(gdal.VSIGetMemFileBuffer_unsafe(name))


def decode_png(encoded):
    data = base64.b64decode(encoded, validate=True)
    with vsi_file() as name:
        gdal.FileFromMemBuffer(name, data)
        dataset = gdal.Open(name)
        if dataset.RasterXSize > 1024 or dataset.RasterYSize > 1024:
            raise ValueError("Reference view exceeds the bounded image size")
        result = dataset.ReadAsArray()
        dataset = None
    return result


def encoded_png(pixels):
    return base64.b64encode(png_bytes(pixels)).decode("ascii")


def outlined_pixels(pixels, mask):
    outline = mask.astype(bool)
    boundary = neighborhood(outline, 1, np.logical_or.reduce) ^ neighborhood(outline, 1, np.logical_and.reduce)
    result = pixels.copy()
    result[:, boundary] = np.array([0, 240, 200])[:, None]
    return result


def pixel_geometry(geojson_path, source):
    return cutlines.pixel_geometry(geojson_path, source)


def inset_geometry(boundary):
    ring = ogr.Geometry(ogr.wkbLinearRing)
    for x, y in boundary:
        if not math.isfinite(x) or not math.isfinite(y):
            raise ValueError("Inset coordinates must be finite")
        ring.AddPoint_2D(x, y)
    ring.CloseRings()
    polygon = ogr.Geometry(ogr.wkbPolygon)
    polygon.AddGeometry(ring)
    if polygon.IsEmpty() or not polygon.IsValid() or polygon.GetArea() == 0:
        raise ValueError("Invalid inset boundary")
    return polygon


def regions(metadata_root, family, charts=None, include_imported=True):
    directory = Path(metadata_root) / family
    if not directory.is_dir():
        raise ValueError(f"Missing chart metadata family {family}")
    result = []
    for path in sorted(directory.glob("*.geojson")):
        if charts and path.stem not in charts:
            continue
        result.append({"id": digest([family, path.stem, "cutline"])[:24], "family": family,
                       "chart": path.stem, "name": "Main cutline", "kind": "cutline",
                       "definition": json.loads(path.read_text()), "metadata_file": path.name,
                       "source": path.stem + ".tif"})
    for path in sorted(directory.glob("*.navigable-insets.json")):
        document = json.loads(path.read_text())
        if document["schema_version"] != cutlines.INSET_LAYOUT_SCHEMA:
            raise ValueError(f"Unsupported inset layout: {path.name}")
        chart = Path(document["source"]).stem
        if charts and chart not in charts:
            continue
        for inset in document["insets"]:
            if not inset["enabled"]:
                continue
            result.append({"id": digest([family, chart, "inset", inset["id"]])[:24], "family": family,
                           "chart": chart, "name": inset["id"], "kind": "inset",
                           "definition": {"inset": inset, "width": document["source_width"],
                                          "height": document["source_height"]},
                           "metadata_file": path.name, "source": document["source"]})
    if include_imported:
        for source_family, targets in cutlines.INSET_TARGETS.items():
            if source_family == family or family not in targets or not (Path(metadata_root) / source_family).is_dir():
                continue
            # One source-owned reference supplies every output of the inset.
            result.extend(region for region in regions(metadata_root, source_family, charts, False)
                          if region['kind'] == 'inset'
                          and region['definition']['inset']['target_family'] == family)
    return result


def source_path(root, name):
    if Path(name).name != name:
        raise ValueError("Chart source must be a plain filename")
    roots = root if isinstance(root, (list, tuple)) else [root]
    # Mirrors the builder's ordered source-tree overlay (SEC is seeded last for
    # TAC/FLY). Some FAA archives legitimately contain the same sheet filename.
    files = {path.name: path for directory in roots for path in Path(directory).iterdir() if path.is_file()}
    matches = sorted({path.resolve() for filename, path in files.items() if filename.casefold() == name.casefold()})
    if len(matches) != 1:
        raise ValueError(f"Expected one source for {name}; found {len(matches)}")
    return matches[0]


def publication_regions(metadata_root, family, charts=None):
    """Check every manual calibration on a consumed source, not just this layer's inset.

    A failed TAC inset also invalidates a Flyway sibling and the parent's mask.
    Selection for the interactive reviewer remains per-output in regions().
    """
    selected = regions(metadata_root, family, charts)
    sources = {r['source'].casefold() for r in selected}
    by_id = {r['id']: r for r in selected}
    for owner in cutlines.INSET_TARGETS:
        if not (Path(metadata_root) / owner).is_dir():
            continue
        for region in regions(metadata_root, owner, include_imported=False):
            if region['kind'] == 'inset' and region['source'].casefold() in sources:
                by_id[region['id']] = region
    return sorted(by_id.values(), key=lambda r: r['id'])


def publication_decision(metadata_root, family, selected, results):
    """Produce an explicit build-input exclusion, never relocate or repair pixels."""
    sources = {r['source'].casefold(): r['source'] for r in results
               if r['kind'] == 'inset' and r['status'] == 'critical'}
    uncontained = [r for r in results if r['status'] == 'critical'
                   and r['source'].casefold() not in sources]
    if uncontained:
        return {'state': 'blocked', 'reason': 'Uncontained chart check failure: ' +
                '; '.join(f"{r['chart']}: {r['message']}" for r in uncontained)}
    excluded = []
    for owner in cutlines.INSET_TARGETS:
        directory = Path(metadata_root) / owner
        for path in sorted(directory.glob('*')):
            if path.suffix == '.geojson':
                source = path.stem + '.tif'
                dependencies = [source]
            elif any(path.name.endswith('.' + kind + '.json')
                     for kind in ('navigable-insets', 'inset', 'legend')):
                document = json.loads(path.read_text())
                dependencies = [document['source']]
                if document.get('coverage_source'):
                    dependencies.append(document['coverage_source'] + '.tif')
            else:
                continue
            if any(source.casefold() in sources for source in dependencies):
                excluded.append(str(path.relative_to(metadata_root)))
    has_map_sources = any(r['source'].casefold() not in sources and
                          ((r['kind'] == 'cutline' and r['family'] == family) or
                           (r['kind'] == 'inset' and r['definition']['inset']['target_family'] == family))
                          for r in selected)
    return {'state': 'ready', 'has_map_sources': has_map_sources,
            'quarantined_sources': sorted(sources.values()),
            'excluded_metadata': sorted(excluded)}


def window_for(geometry):
    left, right, top, bottom = geometry.GetEnvelope()
    padding = max(right - left, bottom - top) * POLICY["context_fraction"]
    left, top = math.floor(left - padding), math.floor(top - padding)
    right, bottom = math.ceil(right + padding), math.ceil(bottom + padding)
    width, height = right - left, bottom - top
    ratio = POLICY["overview_pixels"] / max(width, height)
    return [left, top, width, height], [max(1, round(width * ratio)), max(1, round(height * ratio))]


def sample(source, window, size):
    # Translate reads a bounded output directly from the source, avoiding a full
    # chart warp or full-resolution RGB materialization. Out-of-sheet context is 0.
    options = {"format": "MEM", "srcWin": window, "width": size[0], "height": size[1],
               "resampleAlg": "average"}
    if source.GetRasterBand(1).GetColorTable() is not None:
        options["rgbExpand"] = "rgb"
    elif source.RasterCount >= 3:
        options["bandList"] = [1, 2, 3]
    else:
        raise ValueError("Chart source is neither palette nor RGB")
    image = gdal.Translate("", source, **options)
    if image is None:
        raise ValueError("Chart sample could not be rendered")
    return image.ReadAsArray()


class ChartSamples:
    """One region's bounded views, shared by candidate capture and comparison."""

    def __init__(self, source):
        self.source = source
        self.views = {}

    def read(self, window, size):
        key = (*window, *size)
        if key not in self.views:
            self.views[key] = sample(self.source, window, size)
        return self.views[key]


def region_mask(geometry, window, size):
    left, top, width, height = window
    mask = gdal.GetDriverByName("MEM").Create("", *size, 1, gdal.GDT_Byte)
    mask.SetGeoTransform((left, width / size[0], 0, top, 0, height / size[1]))
    pixels = osr.SpatialReference()
    pixels.SetLocalCS("Source chart pixels")
    mask.SetProjection(pixels.ExportToWkt())
    data = ogr.GetDriverByName("Memory").CreateDataSource("")
    layer = data.CreateLayer("mask", srs=pixels, geom_type=geometry.GetGeometryType())
    feature = ogr.Feature(layer.GetLayerDefn())
    feature.SetGeometry(geometry)
    layer.CreateFeature(feature)
    gdal.RasterizeLayer(mask, [1], layer, burn_values=[1])
    return mask.ReadAsArray().astype(bool)


def neighborhood(array, radius, reducer):
    padded = np.pad(array, [(0, 0)] * (array.ndim - 2) + [(radius, radius)] * 2, mode="edge")
    height, width = array.shape[-2:]
    views = [padded[..., y:y + height, x:x + width]
             for y in range(2 * radius + 1) for x in range(2 * radius + 1)]
    return reducer(views)


def changed_fractions(before, current, mask, kind):
    if before.shape != current.shape or before.ndim != 3 or before.shape[0] != 3:
        raise ValueError("Reference and current RGB sampling shapes differ")
    blurred = [neighborhood(a.astype(np.float32), POLICY["blur_radius"], lambda a: np.mean(a, axis=0))
               for a in (before, current)]
    difference = np.max(np.abs(blurred[0] - blurred[1]), axis=0)
    changed = difference > POLICY["pixel_difference"]
    masks = {"control": np.ones(changed.shape, dtype=bool)} if kind == "control" else {
        "interior": mask,
        "boundary": neighborhood(mask, 2, np.logical_or.reduce)
        ^ neighborhood(mask, 2, np.logical_and.reduce),
    }
    scores = {}
    for name, selected in masks.items():
        if not selected.any():
            raise ValueError(f"Empty {name} comparison region")
        scores[name] = float(changed[selected].mean())
    return scores, np.clip(np.abs(before.astype(float) - current) * 3, 0, 255).astype(np.uint8)


def reference_path(metadata_root, family, region_id):
    return Path(metadata_root) / family / "visual-references" / f"{region_id}.json"


def capture(region, samples, family, source_id, cycle, geometry):
    source = samples.source
    window, size = window_for(geometry)
    views = [{"name": "overview", "kind": "overview", "window": window, "size": size,
              "mask": encoded_png(region_mask(geometry, window, size).astype(np.uint8))}]
    if region["kind"] == "inset":
        for i, point in enumerate(region["definition"]["inset"]["control_points"]):
            x, y = point["pixel"]
            side = POLICY["control_pixels"]
            views.append({"name": f"control-{i + 1}", "kind": "control",
                          "window": [round(x - side / 2), round(y - side / 2), side, side],
                          "size": [side, side], "control": point})
    for view in views:
        view["pixels"] = encoded_png(samples.read(view["window"], view["size"]))
    return {"schema_version": SCHEMA, "recipe": RECIPE, "family": family,
            "region_id": region["id"], "definition_hash": digest(region["definition"]),
            "source": {"name": region["source"], "cycle": cycle, "identity": source_id,
                       "dimensions": [source.RasterXSize, source.RasterYSize],
                       "projection": source.GetProjection(), "transform": source.GetGeoTransform()},
            "views": views}


def validate_reference(reference, region):
    if reference["schema_version"] != SCHEMA or reference["recipe"] != RECIPE:
        raise ValueError("Reference schema/rendering recipe requires a new review")
    content = {key: value for key, value in reference.items() if key != "approval"}
    if reference["approval"]["digest"] != digest(content):
        raise ValueError("Approved reference content hash mismatch")
    if reference["region_id"] != region["id"]:
        raise ValueError("Reference belongs to a different chart region")


STYLE = """body{font:18px sans-serif;background:#16212b;color:#edf2f5;margin:24px}
a{color:#80d5f5} .warning{color:#f7c552}.critical{color:#ff8d8d}.ok{color:#9bdaaa}
img{max-width:100%;image-rendering:auto} .views{display:flex;flex-wrap:wrap;gap:12px}
figure{margin:0;max-width:32%}figcaption{margin:8px 0}pre{white-space:pre-wrap}
table{border-collapse:collapse}td,th{padding:10px;text-align:left;border-bottom:1px solid #51606b}
.overlay{position:relative;display:inline-block}.overlay .current{position:absolute;inset:0;opacity:.5}
input.blink:checked~.overlay .current{animation:blink 1s steps(1) infinite}
@keyframes blink{50%{opacity:0}100%{opacity:1}}summary{cursor:pointer}
@media(max-width:700px){figure{max-width:100%}}"""


def html_page(title, body):
    return f'<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>{html.escape(title)}</title><style>{STYLE}</style><h1>{html.escape(title)}</h1>{body}'


def render_region(region, candidate, reference, samples, output):
    """Write review evidence and return worst score; comparisons use approved windows."""
    source = samples.source
    severity, reasons, scores, body = "ok", [], {}, []
    if reference is None:
        severity, reasons = "warning", ["No approved reference; candidate awaits manual review"]
        views = candidate["views"]
    else:
        validate_reference(reference, region)
        views = reference["views"]
        if reference["definition_hash"] != candidate["definition_hash"]:
            severity = "critical" if region["kind"] == "inset" else "warning"
            reasons.append("Cutline/georeference changed since approval; review the new candidate")
        if reference["source"]["dimensions"] != candidate["source"]["dimensions"]:
            severity = "critical" if region["kind"] == "inset" else "warning"
            reasons.append("Source dimensions changed")
        if not cutlines.spatial_reference(reference["source"]["projection"]).IsSame(source.GetSpatialRef()):
            severity = "critical" if region["kind"] == "inset" else "warning"
            reasons.append("Source projection changed")
        if region["kind"] == "cutline" and reference["source"]["transform"] != list(source.GetGeoTransform()):
            severity = "warning"
            reasons.append("Source pixel-to-map placement changed")
    for view in views:
        current = samples.read(view["window"], view["size"])
        prefix = view["name"]
        (output / f"{prefix}-current.png").write_bytes(png_bytes(current))
        figures = f'<figure><figcaption>Current</figcaption><img src="{prefix}-current.png"></figure>'
        if reference:
            before = decode_png(view["pixels"])
            mask = decode_png(view["mask"]).astype(bool) if view["kind"] == "overview" else None
            measured, difference = changed_fractions(before, current, mask, view["kind"])
            scores[prefix] = measured
            for metric, value in measured.items():
                level = "critical" if value >= POLICY[metric]["critical"] else "warning" if value >= POLICY[metric]["warning"] else "ok"
                if region["kind"] != "inset" and level == "critical":
                    level = "warning"
                if SEVERITY[level] > SEVERITY[severity]:
                    severity = level
                if level != "ok":
                    reasons.append(f"{prefix} {metric}: {value:.1%} changed")
            for name, pixels in [("reference", before), ("difference", difference)]:
                (output / f"{prefix}-{name}.png").write_bytes(png_bytes(pixels))
            figures = f'<figure><figcaption>Approved</figcaption><img src="{prefix}-reference.png"></figure>' + figures + f'<figure><figcaption>Difference (3x)</figcaption><img src="{prefix}-difference.png"></figure>'
            figures += f'<details><summary>Blink / overlay</summary><input class="blink" type="checkbox" id="{prefix}"><label for="{prefix}">Blink</label><br><div class="overlay"><img src="{prefix}-reference.png"><img class="current" src="{prefix}-current.png"></div></details>'
        if view["kind"] == "overview":
            outlined = outlined_pixels(current, decode_png(view["mask"]))
            (output / f"{prefix}-outline.png").write_bytes(png_bytes(outlined))
            figures += f'<figure><figcaption>Sampling cutline (cyan)</figcaption><img src="{prefix}-outline.png"></figure>'
        control = f'<p>Stored control: {html.escape(json.dumps(view["control"], sort_keys=True))}</p>' if "control" in view else ""
        body.append(f'<h2>{html.escape(prefix)}</h2>{control}<div class="views">{figures}</div>')
    title = f'{region["chart"]}: {region["name"]}'
    text = "; ".join(reasons) if reasons else "Matches the approved reference"
    command = f'python3 tools/chart_cutline_audit.py --approve-reference {output / "candidate.json"} --reviewed-by YOUR_NAME'
    body.insert(0, f'<p class="{severity}">{html.escape(text)}</p><p>Fixed source-pixel windows; no alignment correction. A match does not prove the georeference.</p><p>After inspecting (and correcting the metadata in the cutline editor if needed), approve this exact candidate:</p><pre>{html.escape(command)}</pre>')
    if reference and reference["definition_hash"] != candidate["definition_hash"]:
        body.append(f'<h2>New candidate after metadata changes</h2><p>The comparison above always uses approved windows. The candidate below follows the current metadata.</p><img src="data:image/png;base64,{candidate["views"][0]["pixels"]}">')
    (output / "index.html").write_text(html_page(title, "".join(body)))
    return severity, text, scores


def check_region(region, source_root, metadata_root, source_id, cycle, output):
    output.mkdir(parents=True, exist_ok=True)
    result = {key: region[key] for key in ("id", "chart", "name", "kind", "family", "source")}
    family = region["family"]
    try:
        source = gdal.Open(str(source_path(source_root, region["source"])))
        if source.GetSpatialRef() is None or source.GetGeoTransform(can_return_null=True) is None:
            raise ValueError("Source lacks a georeference")
        if region["kind"] == "inset" and [source.RasterXSize, source.RasterYSize] != [region["definition"]["width"], region["definition"]["height"]]:
            raise ValueError("Inset calibration source dimensions no longer match the FAA sheet")
        geometry = (inset_geometry(region["definition"]["inset"]["boundary"]) if region["kind"] == "inset"
                    else pixel_geometry(Path(metadata_root) / family / region["metadata_file"], source))
        samples = ChartSamples(source)
        candidate = capture(region, samples, family, source_id, cycle, geometry)
        atomic_json(output / "candidate.json", candidate)
        path = reference_path(metadata_root, family, region["id"])
        reference = json.loads(path.read_text()) if path.exists() else None
        status, message, scores = render_region(region, candidate, reference, samples, output)
        result.update(status=status, message=message, scores=scores, unreviewed=reference is None)
    except (RuntimeError, ValueError, KeyError, TypeError, OSError) as error:
        result.update(status="critical", message=f"Check failed: {error}", scores={}, unreviewed=False)
        (output / "index.html").write_text(html_page(region["chart"], f'<p class="critical">{html.escape(result["message"])}</p>'))
    return result


def approve(candidate_path, metadata_root, reviewer):
    if not reviewer.strip():
        raise ValueError("Approval needs the reviewer's name")
    candidate = json.loads(Path(candidate_path).read_text())
    matches = [region for region in regions(metadata_root, candidate["family"])
               if region["id"] == candidate["region_id"]]
    if len(matches) != 1 or digest(matches[0]["definition"]) != candidate["definition_hash"]:
        raise ValueError("Candidate is stale: cutline/georeference changed or region was removed")
    if candidate["schema_version"] != SCHEMA or candidate["recipe"] != RECIPE or "approval" in candidate:
        raise ValueError("Expected an unapproved candidate from this rendering recipe")
    candidate["approval"] = {"digest": digest(candidate), "reviewed_by": reviewer.strip(), "reviewed_at": now()}
    path = reference_path(metadata_root, candidate["family"], candidate["region_id"])
    atomic_json(path, candidate)
    return path


def run_check(source_root, metadata_root, family, output, source_id, cycle, charts=None):
    output = Path(output)
    output.mkdir(parents=True, exist_ok=True)
    # Serializes attempts for one family, including the report's atomic current pointer.
    import fcntl
    with (output / ".lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        report_id = uuid.uuid4().hex
        report_root = output / "reports" / report_id
        report_root.mkdir(parents=True)
        report = {"schema_version": REPORT_SCHEMA, "family": family, "cycle": cycle,
                  "source_id": source_id, "report_id": report_id, "started_at": now(),
                  "status": "checking", "policy": POLICY, "regions": []}
        atomic_json(output / "current.json", report)
        try:
            initialize_rendering()
            selected = publication_regions(metadata_root, family, charts)
            if not selected:
                raise ValueError("No chart regions selected")
            report["regions"] = [check_region(region, source_root, metadata_root, source_id, cycle,
                                               report_root / region["id"]) for region in selected]
            report["status"] = max((r["status"] for r in report["regions"]), key=SEVERITY.get)
            report['publication'] = publication_decision(metadata_root, family, selected, report['regions'])
        except (ImportError, RuntimeError, ValueError, OSError, KeyError) as error:
            report["status"] = "critical"
            report["error"] = str(error)
            report['publication'] = {'state': 'blocked', 'reason': str(error)}
        report["completed_at"] = now()
        report["warning_count"] = sum(r["status"] == "warning" for r in report["regions"])
        report["critical_count"] = sum(r["status"] == "critical" for r in report["regions"]) + int("error" in report)
        report["unreviewed_count"] = sum(r["unreviewed"] for r in report["regions"])
        rows = "".join(f'<tr><td class="{r["status"]}">{r["status"]}</td><td><a href="{r["id"]}/index.html">{html.escape(r["chart"])} / {html.escape(r["name"])}</a></td><td>{html.escape(r["message"])}</td></tr>' for r in sorted(report["regions"], key=lambda r: -SEVERITY[r["status"]]))
        title = f'Chart visual checks: {family}, cycle {cycle}'
        decision = report['publication']
        if decision['state'] == 'blocked':
            disposition = 'Publication blocked: ' + decision['reason']
        elif decision['quarantined_sources']:
            disposition = 'CRITICAL: omitting these source sheets and ALL derived layers/references: ' + ', '.join(decision['quarantined_sources']) + '. Remaining sheets may publish. Manual review required.'
        else:
            disposition = 'No source sheets quarantined.'
        (report_root / "index.html").write_text(html_page(title, f'<p class="{report["status"]}">{html.escape(disposition)}</p><p>References require explicit review; these images are never auto-approved.</p><table><tr><th>Status</th><th>Region</th><th>Reason</th></tr>{rows}</table><details><summary>Publication decision</summary><pre>{html.escape(json.dumps(decision, indent=2))}</pre></details><details><summary>Scoring policy</summary><pre>{html.escape(json.dumps(POLICY, indent=2))}</pre></details>'))
        atomic_json(report_root / "report.json", report)
        atomic_json(output / "current.json", report)
        retained = sorted((p for p in (output / "reports").iterdir() if p != report_root),
                          key=lambda p: p.stat().st_mtime, reverse=True)
        for old in retained[2:]:
            shutil.rmtree(old)
        return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, action="append",
                        help="Source tree; repeat in builder overlay order (primary, then SEC)")
    parser.add_argument("--metadata-root", type=Path, default=Path("product/chart-metadata"))
    parser.add_argument("--family")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--source-id", default="manual-review")
    parser.add_argument("--cycle", default="manual-review")
    parser.add_argument("--chart", action="append")
    parser.add_argument("--approve-reference", type=Path)
    parser.add_argument("--reviewed-by")
    args = parser.parse_args(argv)
    if args.approve_reference:
        if not args.reviewed_by:
            parser.error("--reviewed-by is required for approval")
        print(approve(args.approve_reference, args.metadata_root, args.reviewed_by))
        return 0
    if not all((args.family, args.source_root, args.output)):
        parser.error("--family, --source-root and --output are required for checks")
    report = run_check(args.source_root, args.metadata_root, args.family, args.output,
                       args.source_id, args.cycle, args.chart)
    # stdout is the result of THIS invocation, not the racy monitoring current.json.
    print(json.dumps(report))
    return 2 if report['publication']['state'] == 'blocked' else 0


if __name__ == "__main__":
    sys.exit(main())
