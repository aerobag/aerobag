#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Local, durable operator review of chart-quality candidates."""

import argparse
import gzip
import html
import importlib.util
import json
from pathlib import Path
import secrets
import shutil
import threading
from collections import OrderedDict
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import unquote, urlencode, urlparse

SPEC = importlib.util.spec_from_file_location(
    "chart_quality", Path(__file__).resolve().parents[1]
    / "product/preprocessor/preprocessor-charts/chart_quality.py",
)
quality = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(quality)
FAMILIES = ("SEC", "TAC", "FLY", "ENR_L", "ENR_H")
ASSETS = Path(__file__).with_name("chart_visual_review")
REGION_COLORS = {"cutline": "#00f0c8", "inset": "#ef99ef", "draft": "#f7c552",
                 "reference": "#80d5f5", "legend": "#ffad70", "detail": "#91b8ff"}


def sheet_review_summary(regions, inventory_status):
    candidates = [r for r in regions if r["review_id"] is not None]
    counts = {status: sum(r["status"] == status for r in candidates)
              for status in ("approved", "disapproved", "pending", "stale", "recheck")}
    total, approved = len(candidates), counts["approved"]
    if not total:
        label, tone = "Regions: No reference candidates", "inventory"
    elif approved == total:
        label, tone = f"Regions: Approved ({approved}/{total})", "approved"
    else:
        parts = [f"Regions: {approved}/{total} approved"]
        parts.extend(f"{counts[status]} {text}" for status, text in (
            ("disapproved", "disapproved"), ("pending", "not reviewed"),
            ("stale", "stale"), ("recheck", "need recheck")) if counts[status])
        label = "; ".join(parts)
        tone = ("disapproved" if counts["disapproved"] else "stale" if counts["stale"] or counts["recheck"]
                else "pending")
    inventory_label = {"pending": "Not reviewed", "complete": "All accounted for",
                       "incomplete": "Missing / unresolved regions", "stale": "Changed: review again"}[inventory_status]
    return [{"label": label, "tone": tone},
            {"label": f"Inset inventory: {inventory_label}", "tone": inventory_status}]


def geometry_rings(geometry):
    polygons = [geometry] if geometry.GetGeometryName() == "POLYGON" else geometry
    return [[[list(point[:2]) for point in ring.GetPoints()] for ring in polygon]
            for polygon in polygons]


class SheetInventory:
    """Current source-sheet inventory, separate from immutable region candidates."""

    def __init__(self, store):
        self.store = store
        self.render_lock = threading.Lock()
        self.rendered = OrderedDict()

    def sheets(self):
        sheets = {}

        def sheet(family, name):
            if Path(name).name != name:
                raise ReviewError("Chart source must be a plain filename")
            key = quality.digest([family, name])[:24]
            return sheets.setdefault(key, {"id": key, "family": family, "source": name,
                                          "chart": Path(name).stem, "regions": []})

        def add_region(parent, kind, name, identity, definition, enabled=True, editor_chart=None):
            region = {"id": identity, "kind": kind, "name": name, "definition": definition,
                      "color": REGION_COLORS[kind], "enabled": enabled}
            query = {"family": parent["family"], "chart": editor_chart or parent["chart"]}
            if kind not in {"cutline", "detail"}:
                query["type"] = "navigable-inset" if kind in {"inset", "draft"} else "inset" if kind == "reference" else "legend"
                query["all"] = "1"
            region["editor_url"] = (self.store.editor_url.rstrip("/") +
                                    ("/" if kind in {"cutline", "detail"} else "/extracts") + "?" + urlencode(query)
                                    if self.store.editor_url else None)
            parent["regions"].append(region)

        for family in FAMILIES:
            directory = self.store.metadata_root / family
            for path in sorted(directory.glob("*.geojson")):
                document = json.loads(path.read_text())
                registration = quality.cutlines.source_sheet(document)
                parent = sheet(family, registration['source'] if registration else path.stem + ".tif")
                add_region(parent, "detail" if registration else "cutline",
                           f"FAA detail: {path.stem} -> {family} (review cutline; no manual georef)" if registration else f"Main -> {family}",
                           quality.digest([family, path.stem, "cutline"])[:24], document, editor_chart=path.stem)
                if registration:
                    parent['regions'][-1]['detail_source'] = path.stem + '.tif'
                    parent['regions'][-1]['work_note'] = registration.get('note', '')
            for suffix in ("navigable-insets", "inset", "legend"):
                for path in sorted(directory.glob(f"*.{suffix}.json")):
                    document = json.loads(path.read_text())
                    if document["schema_version"] != (quality.cutlines.INSET_LAYOUT_SCHEMA if suffix == "navigable-insets" else 1):
                        raise ReviewError(f"Unsupported layout: {path.name}")
                    parent = sheet(family, document["source"])
                    for index, region in enumerate(document["insets" if suffix == "navigable-insets" else "regions"]):
                        if suffix == "navigable-insets":
                            kind = "inset" if region["enabled"] else "draft"
                            name = f'{region["id"]} -> {region["target_family"]} ({"enabled" if region["enabled"] else "draft"})'
                            identity = quality.digest([family, parent["chart"], "inset", region["id"]])[:24]
                        else:
                            kind = "reference" if suffix == "inset" else "legend"
                            name = f'{"Reference image" if kind == "reference" else "Legend"} {index + 1} (not a map layer)'
                            identity = quality.digest([family, parent["chart"], suffix, index])[:24]
                        add_region(parent, kind, name, identity,
                                   {"region": region, "width": document["source_width"], "height": document["source_height"]},
                                   kind != "draft")
        for parent in sheets.values():
            root = self.store.source_trees.get(parent["family"])
            parent["source_error"] = None
            try:
                if root is None:
                    raise ValueError("No source directory configured for this family")
                source = quality.source_path(root, parent["source"])
                stat = source.stat()
                # Inputs are immutable build-cache files; include replacement and
                # in-place edit identity so old completeness decisions go stale.
                signature = [str(source), stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns]
                parent["source_path"] = str(source)
                for region in parent['regions']:
                    if region['kind'] == 'detail':
                        detail = quality.source_path(root, region['detail_source'])
                        stat = detail.stat()
                        signature.append([str(detail), stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns])
            except (OSError, ValueError) as error:
                signature = None
                parent["source_error"] = str(error)
            parent["source_digest"] = quality.digest(signature)
            parent["digest"] = quality.digest([1, signature,
                                               [{k: r[k] for k in ("id", "kind", "definition")} for r in parent["regions"]]])
        return sorted(sheets.values(), key=lambda s: (FAMILIES.index(s["family"]), s["chart"].casefold()))

    def find(self, body):
        parent = next((s for s in self.sheets() if s["id"] == body.get("id")), None)
        if parent is None or parent["digest"] != body.get("digest"):
            raise ReviewError("Source sheet or its regions changed; reload the inventory before reviewing")
        if parent["source_error"]:
            raise ReviewError(parent["source_error"])
        return parent

    def render(self, body):
        parent = self.find(body)
        # Full-sheet decoding is bounded and serialized, never inside the
        # review-model lock. Scrolling cannot herd full TIFF decodes into RAM.
        with self.render_lock:
            if parent["digest"] in self.rendered:
                self.rendered.move_to_end(parent["digest"])
                return self.rendered[parent["digest"]]
            source = quality.gdal.Open(parent["source_path"])
            width, height = source.RasterXSize, source.RasterYSize
            directory = self.store.directory / "sheets" / parent["id"]
            directory.mkdir(parents=True, exist_ok=True)
            image = directory / (parent["source_digest"] + ".png")
            if not image.exists():
                ratio = min(1, 2400 / max(width, height))
                pixels = quality.sample(source, [0, 0, width, height],
                                        [max(1, round(width * ratio)), max(1, round(height * ratio))])
                temporary = image.with_suffix(".partial")
                temporary.write_bytes(quality.png_bytes(pixels))
                temporary.replace(image)
            geometries = {}
            for region in parent["regions"]:
                definition = region["definition"]
                if region["kind"] == "cutline":
                    geometry = quality.cutlines.pixel_document(definition, source)
                elif region['kind'] == 'detail':
                    detail = quality.gdal.Open(str(quality.source_path(self.store.source_trees[parent['family']], region['detail_source'])))
                    geometry = quality.cutlines.registered_detail_geometry(definition, detail, source)
                else:
                    if (definition["width"], definition["height"]) != (width, height):
                        raise ReviewError(f'{region["name"]}: stored dimensions do not match current source; repair in editor')
                    value = definition["region"]
                    if region["kind"] in {"inset", "draft"}:
                        boundary = value["boundary"]
                    else:
                        x, y, w, h = (value[k] for k in ("x", "y", "width", "height"))
                        boundary = [[x, y], [x+w, y], [x+w, y+h], [x, y+h]]
                    geometry = quality.inset_geometry(boundary)
                geometries[region["id"]] = geometry
            main = next((geometries[r["id"]].Clone() for r in parent["regions"] if r["kind"] == "cutline"), None)
            if main is not None:
                for region in parent["regions"]:
                    if region["kind"] in {"inset", "detail"}:
                        main = main.Difference(geometries[region["id"]])
            regions = []
            for number, region in enumerate(parent["regions"], 1):
                geometry = geometries[region["id"]]
                fraction = (geometry.Intersection(main).GetArea() / geometry.GetArea()
                            if main is not None and geometry.GetArea() else None)
                if region["kind"] == "cutline":
                    masks = sum(r["kind"] in {"inset", "detail"} for r in parent["regions"])
                    exclusion = (f"Main outline; {masks} navigable inset / FAA detail polygon(s) masked out."
                                 if masks else "Main cutline only; no explicit inset masks.")
                elif region["kind"] == "inset":
                    exclusion = "Excluded from parent: explicit navigable-inset mask."
                elif region['kind'] == 'detail':
                    exclusion = "FAA-georeferenced detail; its registered printed copy is automatically excluded from the parent."
                elif fraction is None:
                    exclusion = "No main cutline is defined on this source."
                elif fraction < 0.00001:
                    exclusion = "Excluded from parent: outside its retained area."
                else:
                    exclusion = f"NOT fully excluded from parent: {fraction:.0%} overlaps retained area."
                regions.append({"id": region["id"], "number": number, "rings": geometry_rings(geometry),
                                "exclusion": exclusion})
            # Reject metadata/source races during a potentially slow TIFF read.
            self.find(body)
            result = {"id": parent["id"], "digest": parent["digest"], "width": width, "height": height,
                      "image": f'/sheets/{parent["id"]}/{parent["source_digest"]}.png', "regions": regions}
            self.rendered[parent["digest"]] = result
            while len(self.rendered) > 8:
                self.rendered.popitem(last=False)
            return result


class ReviewError(ValueError):
    pass


def hex_id(value, length):
    return isinstance(value, str) and len(value) == length and all(c in "0123456789abcdef" for c in value)


class ReviewStore:
    def __init__(self, report_root, state_dir, metadata_root, reviewer, source_trees=None, editor_url=None):
        if not reviewer.strip():
            raise ReviewError("A reviewer name is required")
        self.report_root = Path(report_root).resolve()
        self.directory = Path(state_dir).resolve()
        self.metadata_root = Path(metadata_root).resolve()
        self.reviewer = reviewer
        self.source_trees = source_trees or {}
        self.editor_url = editor_url
        self.lock = threading.RLock()
        self.reference_cache = {}
        self.directory.mkdir(parents=True, exist_ok=True)
        self.state_path = self.directory / "review.json"
        self.state = (json.loads(self.state_path.read_text()) if self.state_path.exists()
                      else {"schema_version": 1, "items": {}, "imported_reports": {}, "cursor": None})
        if self.state["schema_version"] != 1:
            raise ReviewError("Unsupported review state; do not discard it")
        self.inventory = SheetInventory(self)
        quality.initialize_rendering()
        self.import_reports()

    def save(self):
        quality.atomic_json(self.state_path, self.state)

    def candidate_path(self, item):
        return self.directory / "items" / item["id"] / item["digest"] / "candidate.json"

    def pin(self, region, folder):
        candidate = json.loads((folder / "candidate.json").read_text())
        if not hex_id(region["id"], 24) or candidate["region_id"] != region["id"]:
            raise ReviewError("Report region identity does not match its candidate")
        if candidate["family"] not in FAMILIES:
            raise ReviewError("Unknown candidate family")
        digest = quality.digest(candidate)
        destination = self.directory / "items" / region["id"] / digest
        if not destination.exists():
            # Pin the exact reviewed images outside the checker's report GC.
            temporary = destination.with_name(digest + ".partial")
            if temporary.exists():
                shutil.rmtree(temporary)
            shutil.copytree(folder, temporary)
            quality.initialize_rendering()
            overview = candidate["views"][0]
            # Existing comparison images intentionally use the OLD approved
            # window. This review must show the NEW candidate being approved.
            pixels = quality.outlined_pixels(quality.decode_png(overview["pixels"]),
                                             quality.decode_png(overview["mask"]))
            (temporary / "review-overview.png").write_bytes(quality.png_bytes(pixels))
            (temporary / "review-plain.png").write_bytes(quality.png_bytes(quality.decode_png(overview["pixels"])))
            temporary.rename(destination)
        old = self.state["items"].get(region["id"], {})
        item = {key: region[key] for key in ("id", "chart", "name", "kind")}
        item.update(family=candidate["family"], digest=digest,
                    definition_hash=candidate["definition_hash"], source=candidate["source"],
                    decision=old.get("decision"), views=[view["name"] for view in candidate["views"]])
        return item

    def import_reports(self):
        with self.lock:
            reports = {}
            chosen = {}
            for family in FAMILIES:
                path = self.report_root / family / "current.json"
                if not path.exists():
                    continue
                report = json.loads(path.read_text())
                if report["status"] == "checking":
                    raise ReviewError(f"{family} is still rendering; retry importing when it finishes")
                if report["schema_version"] != quality.REPORT_SCHEMA or not hex_id(report["report_id"], 32):
                    raise ReviewError(f"Invalid {family} report")
                reports[family] = report["report_id"]
                for region in report["regions"]:
                    if not hex_id(region["id"], 24):
                        raise ReviewError("Invalid region ID")
                    folder = path.parent / "reports" / report["report_id"] / region["id"]
                    # Origin family comes first: SEC insets need one decision,
                    # not separate approvals in SEC, TAC and Flyway queues.
                    chosen.setdefault(region["id"], (family, region, folder))
            for region_id, (family, region, folder) in chosen.items():
                if self.state["imported_reports"].get(family) != reports[family]:
                    if not (folder / "candidate.json").exists():
                        raise ReviewError(f"{region['chart']}: no review candidate; repair the failed check first")
                    self.state["items"][region_id] = self.pin(region, folder)
            self.state["imported_reports"].update(reports)
            self.save()

    def definitions(self):
        return {r["id"]: r for family in FAMILIES if (self.metadata_root / family).is_dir()
                for r in quality.regions(self.metadata_root, family) if r["family"] == family}

    def reference_digest(self, item):
        path = quality.reference_path(self.metadata_root, item["family"], item["id"])
        if not path.exists():
            return None
        stat = path.stat()
        signature = (stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns)
        cached = self.reference_cache.get(path)
        if cached is None or cached[0] != signature:
            reference = json.loads(path.read_text())
            quality.validate_reference(reference, item)
            cached = signature, reference["approval"]["digest"]
            self.reference_cache[path] = cached
        return cached[1]

    def snapshot(self):
        with self.lock:
            definitions = self.definitions()
            items = []
            for item in sorted(self.state["items"].values(), key=lambda i: (FAMILIES.index(i["family"]), i["chart"].casefold(), i["kind"], i["name"].casefold())):
                region = definitions.get(item["id"])
                decision = item["decision"]
                stale = region is None or quality.digest(region["definition"]) != item["definition_hash"]
                if stale:
                    status = "stale"
                elif decision and decision["verdict"] == "disapproved":
                    status = "disapproved" if decision["digest"] == item["digest"] else "recheck"
                elif self.reference_digest(item) == item["digest"]:
                    status = "approved"
                else:
                    status = "pending"
                url = f'/items/{item["id"]}/{item["digest"]}/'
                editor = (self.editor_url.rstrip("/") + "/?" + urlencode({"family": item["family"], "chart": item["chart"]})
                          if self.editor_url and item["family"] in self.source_trees else None)
                image = (url + "review-overview.png" if stale
                         else f'/api/outline/{item["id"]}/{item["digest"]}.svg')
                caption = ("Previous candidate and outline; refresh after editing metadata."
                           if stale else "Pinned candidate pixels. Cyan shows the projected cutline. Click the image to enlarge.")
                items.append({**item, "status": status, "image": image, "image_caption": caption,
                              "plain_image": url + "review-plain.png", "detail_url": url + "index.html",
                              "editor_url": editor, "can_refresh": region is not None and item["family"] in self.source_trees})
            counts = {status: sum(i["status"] == status for i in items)
                      for status in ("approved", "disapproved", "pending", "stale", "recheck")}
            by_id = {item["id"]: item for item in items}
            sheets = []
            for parent in self.inventory.sheets():
                decision = self.state.get("sheets", {}).get(parent["id"])
                status = ("pending" if decision is None else "stale" if decision["digest"] != parent["digest"]
                          else decision["verdict"])
                regions = []
                for number, region in enumerate(parent["regions"], 1):
                    item = by_id.get(region["id"])
                    regions.append({key: value for key, value in region.items() if key != "definition"} |
                                   {"number": number, "review_id": item["id"] if item else None,
                                    "status": item["status"] if item else "inventory",
                                    "review_note": ("" if item else "Inventory only: no visual-reference candidate. Use the sheet note to flag corrections.")})
                sheets.append({key: parent[key] for key in ("id", "digest", "family", "chart", "source", "source_error")} |
                              {"regions": regions, "status": status, "decision": decision,
                               "add_region_actions": self.add_region_actions(parent),
                               "work_note": ("FAA detail sources are prepared below. Review their cutlines; do not calibrate them manually. Original sheet flags remain until you finish reviewing."
                                             if any(r['kind'] == 'detail' for r in regions) else ""),
                               "review_summary": sheet_review_summary(regions, status),
                               "overview_url": f'/api/sheet/{parent["id"]}/{parent["digest"]}',
                               "needs_review": status in {"pending", "stale"} or any(r["status"] in {"pending", "stale", "recheck"} for r in regions),
                               "has_rejection": bool(decision and decision["verdict"] == "incomplete") or any(r["status"] in {"disapproved", "recheck"} for r in regions)})
            cursor = self.state.get("sheet_cursor")
            if cursor is None:
                cursor = next((s["id"] for s in sheets if any(r["id"] == self.state["cursor"] for r in s["regions"])), None)
            return {"items": items, "counts": counts, "cursor": self.state["cursor"], "reviewer": self.reviewer,
                    "sheets": sheets, "sheet_cursor": cursor,
                    "sheet_counts": {status: sum(s["status"] == status for s in sheets)
                                     for status in ("complete", "incomplete", "pending", "stale")}}

    def add_region_actions(self, parent):
        if not self.editor_url:
            return []
        query = {"family": parent['family'], "chart": parent['chart'], "all": "1", "mode": "boundary"}
        return [{"label": label, "url": self.editor_url.rstrip('/') + '/extracts?' + urlencode({**query, 'type': kind})}
                for label, kind in (("Map inset (manual georeference)", "navigable-inset"),
                                    ("Reference-only diagram (not a map)", "inset"), ("Legend", "legend"))]

    def item(self, body):
        item = self.state["items"].get(body.get("id"))
        if not item:
            raise ReviewError("Unknown review item")
        if body.get("digest") != item["digest"]:
            raise ReviewError("The displayed candidate has changed. Reload before reviewing it.")
        return item

    def projected_overview(self, body):
        with self.lock:
            item = self.item(body)
            region = self.definitions().get(item["id"])
            if region is None or quality.digest(region["definition"]) != item["definition_hash"]:
                raise ReviewError("Cutline/georeference changed; refresh before drawing its current outline")
            candidate = json.loads(self.candidate_path(item).read_text())
            if quality.digest(candidate) != item["digest"]:
                raise ReviewError("Pinned candidate content hash mismatch")
        # Keep approved pixels and sampling masks immutable. Only the display
        # overlay is derived afresh, including for reports pinned by older code.
        # Reuse the candidate's georeference, never a newer cycle's source TIFF.
        source = quality.gdal.GetDriverByName("MEM").Create("", 1, 1, 1)
        source.SetProjection(candidate["source"]["projection"])
        source.SetGeoTransform(candidate["source"]["transform"])
        geometry = (quality.inset_geometry(region["definition"]["inset"]["boundary"])
                    if region["kind"] == "inset"
                    else quality.cutlines.pixel_document(region["definition"], source))
        view = candidate["views"][0]
        left, top, width, height = view["window"]
        lines = []
        for polygon in [geometry] if geometry.GetGeometryName() == "POLYGON" else geometry:
            for ring in polygon:
                points = " ".join(f"{x:.4f},{y:.4f}" for x, y, *_ in ring.GetPoints())
                lines.append(f'<polyline points="{points}" fill="none" stroke="#00f0c8" '
                             'stroke-width="2" vector-effect="non-scaling-stroke"/>')
        return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{view["size"][0]}" '
                f'height="{view["size"][1]}" viewBox="{left} {top} {width} {height}">'
                f'<image x="{left}" y="{top}" width="{width}" height="{height}" '
                f'preserveAspectRatio="none" href="data:image/png;base64,{html.escape(view["pixels"], quote=True)}"/>'
                + "".join(lines) + '</svg>').encode()

    def decide(self, body):
        with self.lock:
            item = self.item(body)
            verdict, note = body.get("verdict"), body.get("note", "")
            if verdict not in {"approved", "disapproved"} or not isinstance(note, str) or len(note) > 4000:
                raise ReviewError("Invalid review decision or note")
            if verdict == "approved":
                # The one approval implementation validates current metadata,
                # including when the editor changed it after this page loaded.
                region = self.definitions().get(item["id"])
                if region is None or quality.digest(region["definition"]) != item["definition_hash"]:
                    raise ReviewError("Cutline/georeference changed; refresh this image before approving")
                if self.reference_digest(item) != item["digest"]:
                    quality.approve(self.candidate_path(item), self.metadata_root, self.reviewer)
            elif self.reference_digest(item) == item["digest"]:
                # Correct an accidental approval of THIS candidate only.
                # Rejecting a new cycle does not erase its older trusted reference.
                quality.reference_path(self.metadata_root, item["family"], item["id"]).unlink()
            item["decision"] = {"verdict": verdict, "note": note, "digest": item["digest"],
                                "reviewed_by": self.reviewer, "reviewed_at": quality.now()}
            self.state["cursor"] = item["id"]
            self.save()
            return self.snapshot()

    def set_cursor(self, region_id):
        with self.lock:
            if region_id is not None and region_id not in self.state["items"]:
                raise ReviewError("Unknown cursor")
            self.state["cursor"] = region_id
            self.save()

    def sheet_decision(self, body):
        with self.lock:
            parent = self.inventory.find(body)
            verdict, note = body.get("verdict"), body.get("note", "")
            if verdict not in {"complete", "incomplete"} or not isinstance(note, str) or len(note) > 4000:
                raise ReviewError("Invalid source-sheet decision")
            self.state.setdefault("sheets", {})[parent["id"]] = {
                "verdict": verdict, "note": note, "digest": parent["digest"],
                "reviewed_by": self.reviewer, "reviewed_at": quality.now()}
            self.state["sheet_cursor"] = parent["id"]
            self.save()
            return self.snapshot()

    def sheet_cursor(self, body):
        with self.lock:
            parent = next((s for s in self.inventory.sheets() if s["id"] == body.get("id")), None)
            if parent is None:
                raise ReviewError("Unknown source sheet")
            region_id = body.get("region_id")
            if region_id is not None and not any(r["id"] == region_id for r in parent["regions"]):
                raise ReviewError("Region does not belong to source sheet")
            self.state["sheet_cursor"] = parent["id"]
            if region_id in self.state["items"]:
                self.state["cursor"] = region_id
            self.save()

    def refresh(self, body):
        with self.lock:
            region = self.definitions().get(body.get('id'))
            if region is None or region["family"] not in self.source_trees:
                raise ReviewError("Current metadata or configured FAA source is unavailable")
            item = self.state['items'].get(region['id'])
            if item:
                self.item(body)
            elif body.get('digest') != quality.digest(region['definition']):
                raise ReviewError("New region changed; reload the source inventory")
            old_digest = item['digest'] if item else None
            provenance = item['source'] if item else {'identity': 'local-review', 'cycle': 'editor-preview'}
            roots = [self.source_trees[region["family"]]]
            if region["family"] in {"TAC", "FLY"}:
                roots.append(self.source_trees["SEC"])
        # Native rendering is outside the model lock; state/decision requests
        # remain responsive and results cannot overwrite a concurrent new candidate.
        import tempfile
        with tempfile.TemporaryDirectory(dir=self.directory) as temporary:
            folder = Path(temporary)
            quality.initialize_rendering()
            result = quality.check_region(region, roots, self.metadata_root,
                                          provenance["identity"], provenance["cycle"], folder)
            if not (folder / "candidate.json").exists():
                raise ReviewError(result["message"])
            with self.lock:
                if self.state["items"].get(region["id"], {}).get("digest") != old_digest:
                    raise ReviewError("Another refresh replaced this candidate; reload")
                if quality.digest(self.definitions()[region['id']]['definition']) != quality.digest(region['definition']):
                    raise ReviewError("Region changed while rendering; refresh again")
                self.state["items"][region["id"]] = self.pin(region, folder)
                self.save()
        return self.snapshot()

    def refresh_sheet(self, body):
        parent = self.inventory.find(body)
        definitions = self.definitions()
        for r in parent['regions']:
            if r['id'] not in definitions:
                continue
            item = self.state['items'].get(r['id'])
            self.refresh({'id': r['id'], 'digest': item['digest'] if item else quality.digest(definitions[r['id']]['definition'])})
        return self.snapshot()

    def rejections(self):
        return [{key: item[key] for key in ("id", "family", "chart", "name", "status", "decision", "editor_url")}
                for item in self.snapshot()["items"]
                if item["decision"] and item["decision"]["verdict"] == "disapproved"]


def safe_asset(root, relative):
    path = (root / unquote(relative)).resolve()
    if path.is_relative_to(root) and path.is_file() and path.suffix in {".html", ".png", ".json"}:
        return path
    return None


def make_server(store, address):
    csrf = secrets.token_urlsafe(32)

    class Handler(BaseHTTPRequestHandler):
        def send(self, payload, content_type="application/json", status=200, cache_control="no-store"):
            if isinstance(payload, (dict, list)):
                payload = json.dumps(payload, separators=(",", ":")).encode()
            compressible = content_type in {"application/json", "image/svg+xml"}
            compressed = compressible and len(payload) > 1024 and "gzip" in self.headers.get("Accept-Encoding", "")
            if compressed:
                payload = gzip.compress(payload, compresslevel=3)
            self.send_response(status)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(payload)))
            self.send_header("Cache-Control", cache_control)
            if compressible:
                self.send_header("Vary", "Accept-Encoding")
            if compressed:
                self.send_header("Content-Encoding", "gzip")
            self.send_header("X-Content-Type-Options", "nosniff")
            self.end_headers()
            try:
                self.wfile.write(payload)
            except (BrokenPipeError, ConnectionResetError):
                pass

        def do_GET(self):
            try:
                path = urlparse(self.path).path
                if path in {"/", "/review"}:
                    page = (ASSETS / "index.html").read_text().replace("__CSRF__", csrf)
                    return self.send(page.encode(), "text/html; charset=utf-8")
                if path == "/review.js":
                    return self.send((ASSETS / "review.js").read_bytes(), "text/javascript")
                if path == "/prefetch.mjs":
                    return self.send((ASSETS / "prefetch.mjs").read_bytes(), "text/javascript")
                if path == "/api/state":
                    return self.send(store.snapshot())
                if path == "/api/rejections":
                    return self.send(store.rejections())
                if path == "/api/sheet-rejections":
                    return self.send([s for s in store.snapshot()["sheets"]
                                      if s["decision"] and s["decision"]["verdict"] == "incomplete"])
                if path.startswith("/api/sheet/"):
                    parts = path.split("/")
                    if len(parts) != 5:
                        return self.send_error(404)
                    return self.send(store.inventory.render({"id": parts[3], "digest": parts[4]}))
                if path.startswith("/api/outline/"):
                    parts = path.split("/")
                    if len(parts) != 5 or not parts[4].endswith(".svg"):
                        return self.send_error(404)
                    return self.send(store.projected_overview({"id": parts[3], "digest": parts[4][:-4]}),
                                     "image/svg+xml")
                root = store.directory if path.startswith(("/items/", "/sheets/")) else store.report_root
                asset = safe_asset(root, path.lstrip("/"))
                if asset:
                    parts = path.split("/")
                    immutable_image = (asset.suffix == ".png" and
                                       ((len(parts) == 4 and parts[1] == "sheets" and hex_id(parts[2], 24) and hex_id(asset.stem, 64))
                                        or (len(parts) == 5 and parts[1] == "items" and hex_id(parts[2], 24) and hex_id(parts[3], 64))))
                    return self.send(asset.read_bytes(), {".html": "text/html; charset=utf-8", ".png": "image/png", ".json": "application/json"}[asset.suffix],
                                     cache_control="private, max-age=86400, immutable" if immutable_image else "no-store")
                self.send_error(404)
            except (OSError, ValueError, KeyError, RuntimeError) as error:
                self.send({"error": str(error)}, status=500)

        def do_POST(self):
            # Local operator tool, not an Internet deployment. Still refuse
            # cross-origin forms/fetches that could bless references accidentally.
            if self.headers.get("X-Review-Token") != csrf:
                return self.send({"error": "Missing review token; reload this page"}, status=403)
            origin = self.headers.get("Origin")
            if origin and urlparse(origin).netloc != self.headers.get("Host"):
                return self.send({"error": "Cross-origin review refused"}, status=403)
            try:
                length = int(self.headers.get("Content-Length", "0"))
                if not 0 < length <= 8192:
                    return self.send({"error": "Invalid request size"}, status=413)
                body = json.loads(self.rfile.read(length))
                if not isinstance(body, dict):
                    raise ReviewError("Expected a JSON object")
                path = urlparse(self.path).path
                if path == "/api/decision":
                    return self.send(store.decide(body))
                if path == "/api/cursor":
                    store.set_cursor(body.get("id"))
                    return self.send({"ok": True})
                if path == "/api/sheet-cursor":
                    store.sheet_cursor(body)
                    return self.send({"ok": True})
                if path == "/api/sheet-decision":
                    return self.send(store.sheet_decision(body))
                if path == "/api/refresh":
                    return self.send(store.refresh(body))
                if path == "/api/refresh-sheet":
                    return self.send(store.refresh_sheet(body))
                if path == "/api/import":
                    store.import_reports()
                    return self.send(store.snapshot())
                self.send_error(404)
            except ReviewError as error:
                self.send({"error": str(error)}, status=409)
            except (ValueError, KeyError, TypeError) as error:
                self.send({"error": str(error)}, status=400)
            except OSError as error:
                self.send({"error": str(error)}, status=500)

    return ThreadingHTTPServer(address, Handler)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report-root", type=Path, required=True)
    parser.add_argument("--state-dir", type=Path, required=True)
    parser.add_argument("--metadata-root", type=Path, default=Path("product/chart-metadata"))
    parser.add_argument("--reviewed-by", required=True)
    parser.add_argument("--family-work", action="append", default=[], metavar="FAMILY=SOURCE_DIR")
    parser.add_argument("--editor-url")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8094)
    args = parser.parse_args(argv)
    sources = {}
    for value in args.family_work:
        family, path = value.split("=", 1)
        if family not in FAMILIES or family in sources or not Path(path).is_dir():
            parser.error(f"Invalid or repeated family source: {value}")
        sources[family] = Path(path).resolve()
    if ({"TAC", "FLY"} & sources.keys()) and "SEC" not in sources:
        parser.error("TAC/FLY refresh requires the supplemental SEC source tree")
    # Only one process may mutate this durable review queue.
    import fcntl
    args.state_dir.mkdir(parents=True, exist_ok=True)
    with (args.state_dir / ".server.lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        store = ReviewStore(args.report_root, args.state_dir, args.metadata_root, args.reviewed_by, sources, args.editor_url)
        server = make_server(store, (args.host, args.port))
        print(f"Chart review at http://{args.host}:{server.server_port}/; decisions: {store.state_path}", flush=True)
        server.serve_forever()


if __name__ == "__main__":
    main()
