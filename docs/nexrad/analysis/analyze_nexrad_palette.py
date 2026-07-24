#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import argparse
import gzip
import json
import statistics
import tempfile
from collections import Counter
from multiprocessing import Pool
from pathlib import Path

from PIL import Image


DEFAULT_ROOT = Path(
    "/root/aerobag-five/tmp-fast-product-analysis/upstream-nexrad/"
    "2026-05-11T170642Z_2026-05-12T202440Z"
)
OUTPUT_ROOT = Path("/root/aerobag-five/tmp-fast-product-analysis/upstream-nexrad")


def frame_colors(gz_path_text):
    gz_path = Path(gz_path_text)
    with tempfile.TemporaryDirectory(prefix="nexrad-frame-colors-") as td:
        tif_path = Path(td) / "frame.tif"
        with gzip.open(gz_path, "rb") as src, open(tif_path, "wb") as dst:
            dst.write(src.read())
        image = Image.open(tif_path).convert("RGBA")
        rgba = set(image.getdata())
    opaque_rgb = sorted({(r, g, b) for r, g, b, a in rgba if a != 0})
    alpha_values = sorted({a for _, _, _, a in rgba})
    return {
        "file": gz_path.name,
        "opaque_rgb": opaque_rgb,
        "unique_rgba_count": len(rgba),
        "unique_opaque_rgb_count": len(opaque_rgb),
        "alpha_values": alpha_values,
    }


def load_or_compute_union(root, jobs, force):
    union_path = OUTPUT_ROOT / "whole-day-unique-opaque-rgb.json"
    per_frame_path = OUTPUT_ROOT / "whole-day-frame-color-counts.json"
    if union_path.exists() and per_frame_path.exists() and not force:
        colors = [tuple(color) for color in json.loads(union_path.read_text())]
        per_frame = json.loads(per_frame_path.read_text())
        return colors, per_frame

    files = sorted(root.rglob("CONUS_L2_CREF_QCD_*.tif.gz"))
    union = set()
    per_frame = []
    alpha_values = set()
    with Pool(processes=jobs) as pool:
        for idx, result in enumerate(pool.imap_unordered(frame_colors, map(str, files)), 1):
            union.update(tuple(color) for color in result["opaque_rgb"])
            alpha_values.update(result["alpha_values"])
            per_frame.append(
                {
                    "file": result["file"],
                    "unique_rgba_count": result["unique_rgba_count"],
                    "unique_opaque_rgb_count": result["unique_opaque_rgb_count"],
                }
            )
            if idx % 50 == 0 or idx == len(files):
                print(
                    f"progress {idx}/{len(files)} "
                    f"global_opaque_rgb={len(union)} alpha={sorted(alpha_values)}",
                    flush=True,
                )

    colors = sorted(union)
    per_frame.sort(key=lambda item: item["file"])
    union_path.write_text(json.dumps(colors))
    per_frame_path.write_text(json.dumps(per_frame, indent=2))
    return colors, per_frame


def dist_inf(a, b):
    return max(abs(a[0] - b[0]), abs(a[1] - b[1]), abs(a[2] - b[2]))


def greedy_palette(colors, size):
    palette = [min(colors)]
    nearest = [dist_inf(color, palette[0]) for color in colors]
    for _ in range(1, size):
        idx = max(range(len(colors)), key=lambda i: nearest[i])
        chosen = colors[idx]
        palette.append(chosen)
        for i, color in enumerate(colors):
            d = dist_inf(color, chosen)
            if d < nearest[i]:
                nearest[i] = d
    return palette


def improve_palette(colors, palette, passes):
    for pass_idx in range(passes):
        clusters = [[] for _ in palette]
        for color in colors:
            best_idx = min(range(len(palette)), key=lambda i: dist_inf(color, palette[i]))
            clusters[best_idx].append(color)
        changed = 0
        improved = []
        for old, cluster in zip(palette, clusters):
            if not cluster:
                improved.append(old)
                continue
            best = min(cluster, key=lambda candidate: max(dist_inf(candidate, color) for color in cluster))
            if best != old:
                changed += 1
            improved.append(best)
        palette = improved
        errors = palette_errors(colors, palette)
        print(f"improve_pass {pass_idx + 1} changed={changed} max_error={max(errors)}", flush=True)
        if changed == 0:
            break
    return palette


def palette_errors(colors, palette):
    return [min(dist_inf(color, p) for p in palette) for color in colors]


def percentile(values, pct):
    values = sorted(values)
    index = round((len(values) - 1) * pct / 100)
    return values[index]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    parser.add_argument("--jobs", type=int, default=12)
    parser.add_argument("--force", action="store_true")
    parser.add_argument("--palette-size", type=int, default=256)
    parser.add_argument("--improve-passes", type=int, default=5)
    args = parser.parse_args()

    colors, per_frame = load_or_compute_union(args.root, args.jobs, args.force)
    palette = greedy_palette(colors, args.palette_size)
    palette = improve_palette(colors, palette, args.improve_passes)
    errors = palette_errors(colors, palette)
    hist = Counter(errors)

    palette_path = OUTPUT_ROOT / f"whole-day-greedy-{args.palette_size}-palette.json"
    report_path = OUTPUT_ROOT / f"whole-day-greedy-{args.palette_size}-palette-report.json"
    palette_path.write_text(json.dumps(palette))

    per_counts = [item["unique_opaque_rgb_count"] for item in per_frame]
    report = {
        "unique_opaque_rgb": len(colors),
        "palette_size": len(palette),
        "max_rgb_channel_error": max(errors),
        "p50_rgb_channel_error": percentile(errors, 50),
        "p90_rgb_channel_error": percentile(errors, 90),
        "p95_rgb_channel_error": percentile(errors, 95),
        "p99_rgb_channel_error": percentile(errors, 99),
        "p999_rgb_channel_error": percentile(errors, 99.9),
        "per_frame_min_colors": min(per_counts),
        "per_frame_median_colors": statistics.median(per_counts),
        "per_frame_max_colors": max(per_counts),
        "error_histogram": dict(sorted(hist.items())),
    }
    for threshold in [0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 15]:
        over = sum(1 for error in errors if error > threshold)
        report[f"colors_error_gt_{threshold}"] = over
        report[f"colors_error_gt_{threshold}_pct"] = 100 * over / len(errors)
    report_path.write_text(json.dumps(report, indent=2))

    print("RESULT")
    for key, value in report.items():
        if key == "error_histogram":
            continue
        print(f"{key} {value}")
    print("error_histogram")
    for error, count in sorted(hist.items()):
        print(f"{error} {count}")
    print(f"saved_palette {palette_path}")
    print(f"saved_report {report_path}")


if __name__ == "__main__":
    main()
