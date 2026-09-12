#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import argparse
import importlib.util
import html
import json
import math
import shutil
import sys
from dataclasses import dataclass
from pathlib import Path

from osgeo import gdal

_cutline_spec = importlib.util.spec_from_file_location(
    "chart_cutlines", Path(__file__).resolve().parents[1]
    / "product/preprocessor/preprocessor-charts/chart_cutlines.py",
)
cutlines = importlib.util.module_from_spec(_cutline_spec)
_cutline_spec.loader.exec_module(cutlines)


DEFAULT_THUMB_WIDTH = 1000
DEFAULT_CHART_METADATA_ROOT = Path("product/chart-metadata")


@dataclass
class AuditCard:
    stem: str
    source_path: Path
    cutline_path: Path
    thumb_path: Path
    width: int
    height: int
    thumb_width: int
    thumb_height: int
    svg_paths: list[str]
    cutline_bounds: tuple[float, float, float, float] | None
    cutline_area: float


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build an HTML audit page for chart source images and cutline overlays.",
    )
    parser.add_argument(
        "--work-dir",
        required=True,
        type=Path,
        help="Chart work directory containing source GeoTIFFs, such as a charts-tac work dir.",
    )
    parser.add_argument(
        "--chart-metadata-root",
        default=DEFAULT_CHART_METADATA_ROOT,
        type=Path,
        help="Root containing family neatline and legend metadata directories.",
    )
    parser.add_argument(
        "--family",
        default="TAC",
        help="Cutline family directory to audit, such as TAC or SEC.",
    )
    parser.add_argument(
        "--output-dir",
        required=True,
        type=Path,
        help="Directory where the audit page and thumbnails will be written.",
    )
    parser.add_argument(
        "--thumb-width",
        default=DEFAULT_THUMB_WIDTH,
        type=int,
        help="Thumbnail width in CSS/image pixels.",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=None,
        help="Limit card count for quick iteration.",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Remove an existing output directory before writing.",
    )
    return parser.parse_args()


def main() -> None:
    if "--review-server" in sys.argv:
        from chart_visual_review import main as review_main
        review_main([arg for arg in sys.argv[1:] if arg != "--review-server"])
        return
    if any(option in sys.argv for option in ("--quality", "--quality-help", "--approve-reference")):
        spec = importlib.util.spec_from_file_location(
            "chart_quality", Path(__file__).resolve().parents[1]
            / "product/preprocessor/preprocessor-charts/chart_quality.py",
        )
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        args = [arg for arg in sys.argv[1:] if arg != "--quality"]
        if "--quality-help" in args:
            args = ["--help"]
        raise SystemExit(module.main(args))
    gdal.UseExceptions()
    args = parse_args()
    work_dir = args.work_dir.resolve()
    cutline_dir = (args.chart_metadata_root / args.family).resolve()
    output_dir = args.output_dir.resolve()
    thumb_dir = output_dir / "thumbs"

    if not work_dir.is_dir():
        raise SystemExit(f"work dir does not exist: {work_dir}")
    if not cutline_dir.is_dir():
        raise SystemExit(f"cutline dir does not exist: {cutline_dir}")
    if output_dir.exists() and args.force:
        shutil.rmtree(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    thumb_dir.mkdir(parents=True, exist_ok=True)

    cutline_paths = sorted(cutline_dir.glob("*.geojson"))
    if args.limit is not None:
        cutline_paths = cutline_paths[: args.limit]

    cards: list[AuditCard] = []
    missing_sources: list[Path] = []
    for cutline_path in cutline_paths:
        stem = cutline_path.stem
        source_path = work_dir / f"{stem}.tif"
        if not source_path.is_file():
            missing_sources.append(cutline_path)
            continue
        cards.append(
            build_card(
                stem=stem,
                source_path=source_path,
                cutline_path=cutline_path,
                thumb_path=thumb_dir / f"{slug(stem)}.png",
                thumb_width=args.thumb_width,
            )
        )

    cutline_stems = {path.stem for path in cutline_paths}
    source_extras = sorted(
        path for path in work_dir.glob("*.tif") if path.stem not in cutline_stems
    )
    write_index(
        output_dir / "index.html",
        args.family,
        work_dir,
        cutline_dir,
        cards,
        missing_sources,
        source_extras,
    )
    print(output_dir / "index.html")


def build_card(
    stem: str,
    source_path: Path,
    cutline_path: Path,
    thumb_path: Path,
    thumb_width: int,
) -> AuditCard:
    dataset = gdal.Open(str(source_path))
    if dataset is None:
        raise RuntimeError(f"failed to open {source_path}")
    width = dataset.RasterXSize
    height = dataset.RasterYSize
    thumb_height = max(1, round(height * (thumb_width / width)))
    if not thumb_path.is_file():
        gdal.Translate(
            str(thumb_path),
            str(source_path),
            options=gdal.TranslateOptions(
                format="PNG",
                width=thumb_width,
                height=0,
                rgbExpand="rgb",
                resampleAlg="average",
            ),
        )
        aux_path = thumb_path.with_name(thumb_path.name + ".aux.xml")
        if aux_path.exists():
            aux_path.unlink()

    geometry = cutlines.pixel_geometry(cutline_path, dataset)
    pixel_polygons = [
        [point[:2] for point in ring.GetPoints()]
        for polygon in geometry for ring in polygon
    ]
    svg_paths = [svg_path_for_polygon(polygon) for polygon in pixel_polygons if polygon]
    cutline_bounds = bounds_for_polygons(pixel_polygons)
    cutline_area = sum(abs(polygon_area(polygon)) for polygon in pixel_polygons)
    return AuditCard(
        stem=stem,
        source_path=source_path,
        cutline_path=cutline_path,
        thumb_path=thumb_path,
        width=width,
        height=height,
        thumb_width=thumb_width,
        thumb_height=thumb_height,
        svg_paths=svg_paths,
        cutline_bounds=cutline_bounds,
        cutline_area=cutline_area,
    )


def svg_path_for_polygon(polygon: list[tuple[float, float]]) -> str:
    if not polygon:
        return ""
    commands = [f"M {polygon[0][0]:.2f} {polygon[0][1]:.2f}"]
    commands.extend(f"L {x:.2f} {y:.2f}" for x, y in polygon[1:])
    commands.append("Z")
    return " ".join(commands)


def bounds_for_polygons(
    polygons: list[list[tuple[float, float]]],
) -> tuple[float, float, float, float] | None:
    points = [point for polygon in polygons for point in polygon]
    if not points:
        return None
    min_x = min(point[0] for point in points)
    min_y = min(point[1] for point in points)
    max_x = max(point[0] for point in points)
    max_y = max(point[1] for point in points)
    return min_x, min_y, max_x, max_y


def polygon_area(points: list[tuple[float, float]]) -> float:
    if len(points) < 3:
        return 0.0
    area = 0.0
    for index, (x0, y0) in enumerate(points):
        x1, y1 = points[(index + 1) % len(points)]
        area += x0 * y1 - x1 * y0
    return area / 2.0


def write_index(
    path: Path,
    family: str,
    work_dir: Path,
    cutline_dir: Path,
    cards: list[AuditCard],
    missing_sources: list[Path],
    source_extras: list[Path],
) -> None:
    relative_thumb_root = Path("thumbs")
    body = "\n".join(card_html(card, relative_thumb_root) for card in cards)
    missing_html = "\n".join(
        f"<li><code>{escape(str(cutline.name))}</code></li>" for cutline in missing_sources
    ) or "<li>None</li>"
    extras_html = "\n".join(
        f"<li><code>{escape(source.name)}</code></li>" for source in source_extras
    ) or "<li>None</li>"
    path.write_text(
        f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{escape(family)} Cutline Audit</title>
  <style>
    :root {{
      color-scheme: light;
      font-family: Inter, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: #f4f6f8;
      color: #17242d;
    }}
    body {{
      margin: 0;
      padding: 24px;
    }}
    header {{
      max-width: 1120px;
      margin: 0 auto 20px;
    }}
    h1 {{
      margin: 0 0 8px;
      font-size: 28px;
      line-height: 1.15;
    }}
    h2 {{
      margin: 18px 0 8px;
      font-size: 16px;
    }}
    p {{
      margin: 6px 0;
      line-height: 1.35;
    }}
    code {{
      font-family: "SFMono-Regular", Consolas, monospace;
      font-size: 0.92em;
    }}
    .summary {{
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
      gap: 12px;
      margin-top: 16px;
    }}
    .summaryPanel {{
      background: #fff;
      border: 1px solid #d8e0e6;
      border-radius: 8px;
      padding: 12px;
    }}
    .summaryPanel ul {{
      max-height: 180px;
      overflow: auto;
      margin: 8px 0 0;
      padding-left: 20px;
    }}
    .grid {{
      max-width: 1120px;
      margin: 0 auto;
      display: grid;
      grid-template-columns: 1fr;
      gap: 24px;
    }}
    .card {{
      background: #fff;
      border: 1px solid #d8e0e6;
      border-radius: 8px;
      overflow: hidden;
      box-shadow: 0 10px 28px rgba(24, 37, 48, 0.08);
    }}
    .cardHeader {{
      display: flex;
      flex-wrap: wrap;
      justify-content: space-between;
      gap: 8px;
      padding: 12px 14px;
      border-bottom: 1px solid #e2e8ee;
    }}
    .title {{
      font-weight: 720;
    }}
    .facts {{
      display: flex;
      flex-wrap: wrap;
      gap: 10px;
      color: #536571;
      font-size: 13px;
    }}
    .stage {{
      position: relative;
      width: 100%;
      background:
        linear-gradient(45deg, #e9eef2 25%, transparent 25%),
        linear-gradient(-45deg, #e9eef2 25%, transparent 25%),
        linear-gradient(45deg, transparent 75%, #e9eef2 75%),
        linear-gradient(-45deg, transparent 75%, #e9eef2 75%);
      background-size: 20px 20px;
      background-position: 0 0, 0 10px, 10px -10px, -10px 0;
    }}
    .stage img {{
      display: block;
      width: 100%;
      height: auto;
    }}
    .stage svg {{
      position: absolute;
      inset: 0;
      width: 100%;
      height: 100%;
      pointer-events: none;
    }}
    .cutlinePath {{
      fill: rgba(230, 48, 148, 0.16);
      stroke: rgb(224, 24, 128);
      stroke-width: 18;
      vector-effect: non-scaling-stroke;
    }}
    .cardFooter {{
      padding: 10px 14px 12px;
      color: #536571;
      font-size: 13px;
      border-top: 1px solid #e2e8ee;
    }}
  </style>
</head>
<body>
  <header>
    <h1>{escape(family)} Cutline Audit</h1>
    <p>Pink overlay is the current cutline projected back onto the source GeoTIFF pixels.</p>
    <p><strong>Source:</strong> <code>{escape(str(work_dir))}</code></p>
    <p><strong>Cutlines:</strong> <code>{escape(str(cutline_dir))}</code></p>
    <div class="summary">
      <section class="summaryPanel">
        <h2>Coverage</h2>
        <p>{len(cards)} source images matched to cutlines.</p>
        <p>{len(missing_sources)} cutlines had no source image.</p>
        <p>{len(source_extras)} source TIFFs had no matching cutline.</p>
      </section>
      <section class="summaryPanel">
        <h2>Cutlines Missing Sources</h2>
        <ul>{missing_html}</ul>
      </section>
      <section class="summaryPanel">
        <h2>Source TIFFs Without Cutlines</h2>
        <ul>{extras_html}</ul>
      </section>
    </div>
  </header>
  <main class="grid">
    {body}
  </main>
</body>
</html>
""",
        encoding="utf-8",
    )


def card_html(card: AuditCard, thumb_root: Path) -> str:
    retained_pct = (
        card.cutline_area / (card.width * card.height) * 100.0
        if card.width > 0 and card.height > 0
        else 0.0
    )
    margin_text = margins_text(card)
    paths = "\n".join(
        f'          <path class="cutlinePath" d="{escape(path)}"></path>'
        for path in card.svg_paths
    )
    thumb_rel = thumb_root / card.thumb_path.name
    return f"""<article class="card">
      <div class="cardHeader">
        <div class="title">{escape(card.stem)}</div>
        <div class="facts">
          <span>{card.width}x{card.height}px</span>
          <span>retained approx {retained_pct:.1f}%</span>
          <span>{escape(margin_text)}</span>
        </div>
      </div>
      <div class="stage">
        <img src="{escape(str(thumb_rel))}" width="{card.thumb_width}" height="{card.thumb_height}" alt="{escape(card.stem)} source chart">
        <svg viewBox="0 0 {card.width} {card.height}" aria-hidden="true">
{paths}
        </svg>
      </div>
      <div class="cardFooter">
        <code>{escape(card.source_path.name)}</code> with <code>{escape(card.cutline_path.name)}</code>
      </div>
    </article>"""


def margins_text(card: AuditCard) -> str:
    if card.cutline_bounds is None:
        return "no bounds"
    min_x, min_y, max_x, max_y = card.cutline_bounds
    left = max(0.0, min_x)
    top = max(0.0, min_y)
    right = max(0.0, card.width - max_x)
    bottom = max(0.0, card.height - max_y)
    return (
        f"margins L{left:.0f} T{top:.0f} R{right:.0f} B{bottom:.0f}px"
    )


def slug(value: str) -> str:
    return "".join(ch if ch.isalnum() else "-" for ch in value).strip("-").lower()


def escape(value: str) -> str:
    return html.escape(value, quote=True)


if __name__ == "__main__":
    main()
