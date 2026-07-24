#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import argparse
import gzip
import json
import statistics
import tempfile
import zlib
from collections import Counter
from multiprocessing import Pool
from pathlib import Path

import numpy as np
from PIL import Image


DEFAULT_ROOT = Path(
    "/root/aerobag-five/tmp-fast-product-analysis/upstream-nexrad/"
    "2026-05-11T170642Z_2026-05-12T202440Z"
)
DEFAULT_PALETTE = Path(
    "/root/aerobag-five/tmp-fast-product-analysis/upstream-nexrad/"
    "whole-day-greedy-255-palette.json"
)
DEFAULT_UNION = Path(
    "/root/aerobag-five/tmp-fast-product-analysis/upstream-nexrad/"
    "whole-day-unique-opaque-rgb.json"
)
OUTPUT_ROOT = Path("/root/aerobag-five/tmp-fast-product-analysis/upstream-nexrad")
RGB_LUT = None


def percentile(values, pct):
    values = sorted(values)
    index = round((len(values) - 1) * pct / 100)
    return values[index]


def mb(value):
    return value / 1024 / 1024


def load_frame_rgba(gz_path):
    with tempfile.TemporaryDirectory(prefix="nexrad-index-frame-") as td:
        tif_path = Path(td) / "frame.tif"
        with gzip.open(gz_path, "rb") as src, open(tif_path, "wb") as dst:
            dst.write(src.read())
        return np.array(Image.open(tif_path).convert("RGBA"), dtype=np.uint8)


def dist_inf(a, b):
    return max(abs(a[0] - b[0]), abs(a[1] - b[1]), abs(a[2] - b[2]))


def build_rgb_lut(union_colors, palette):
    # 256^3 bytes = 16 MiB. That is cheap enough and makes per-pixel mapping a vectorized lookup.
    lut = np.zeros(256 * 256 * 256, dtype=np.uint8)
    for color in union_colors:
        best_idx = min(range(len(palette)), key=lambda i: dist_inf(color, palette[i])) + 1
        key = (color[0] << 16) | (color[1] << 8) | color[2]
        lut[key] = best_idx
    return lut


def init_worker(lut):
    global RGB_LUT
    RGB_LUT = lut


def palette_indices_from_lut(rgba):
    # Index 0 is transparent. Opaque radar colors use indices 1..255.
    rgb = rgba[:, :, :3].astype(np.uint32)
    keys = (rgb[:, :, 0] << 16) | (rgb[:, :, 1] << 8) | rgb[:, :, 2]
    out = RGB_LUT[keys]
    out[rgba[:, :, 3] == 0] = 0
    return out


def compressed_size(data):
    return len(zlib.compress(data, level=6))


def encode_frame(gz_path_text):
    gz_path = Path(gz_path_text)
    rgba = load_frame_rgba(gz_path)
    indices = palette_indices_from_lut(rgba)
    raw = indices.tobytes()
    return {
        "file": gz_path.name,
        "source_gz": gz_path.stat().st_size,
        "index_raw": len(raw),
        "index_z": compressed_size(raw),
        "nonzero_pixels": int(np.count_nonzero(indices)),
        "indices": indices,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    parser.add_argument("--palette", type=Path, default=DEFAULT_PALETTE)
    parser.add_argument("--union", type=Path, default=DEFAULT_UNION)
    parser.add_argument("--jobs", type=int, default=12)
    parser.add_argument("--limit", type=int, default=0)
    args = parser.parse_args()

    files_by_name = {}
    for path in args.root.rglob("CONUS_L2_CREF_QCD_*.tif.gz"):
        files_by_name.setdefault(path.name, path)
    files = [files_by_name[name] for name in sorted(files_by_name)]
    if args.limit:
        files = files[: args.limit]
    palette = [tuple(color) for color in json.loads(args.palette.read_text())]
    if len(palette) != 255:
        raise SystemExit(f"expected 255 opaque palette colors, got {len(palette)}")
    union_colors = [tuple(color) for color in json.loads(args.union.read_text())]
    lut = build_rgb_lut(union_colors, palette)

    frame_rows = []
    delta_rows = []
    previous = None
    previous_name = None
    with Pool(processes=args.jobs, initializer=init_worker, initargs=(lut,)) as pool:
        encoded = pool.imap(encode_frame, map(str, files), chunksize=1)
        for idx, result in enumerate(encoded, 1):
            indices = result.pop("indices")
            raw_z = result["index_z"]
            gz_name = result["file"]
            frame_rows.append(
                {
                    "file": gz_name,
                    "source_gz": result["source_gz"],
                    "index_raw": result["index_raw"],
                    "index_z": raw_z,
                    "nonzero_pixels": result["nonzero_pixels"],
                }
            )
            if previous is not None:
                xor_delta = np.bitwise_xor(indices, previous)
                mod_delta = (indices.astype(np.int16) - previous.astype(np.int16)) & 0xFF
                mod_delta = mod_delta.astype(np.uint8)
                zero_pixels = int(np.count_nonzero(indices == previous))
                changed_pixels = indices.size - zero_pixels
                delta_rows.append(
                    {
                        "from": previous_name,
                        "to": gz_name,
                        "changed_pixels": changed_pixels,
                        "changed_pct": 100 * changed_pixels / indices.size,
                        "xor_z": compressed_size(xor_delta.tobytes()),
                        "mod_z": compressed_size(mod_delta.tobytes()),
                        "raw_z": raw_z,
                    }
                )
            previous = indices
            previous_name = gz_name
            if idx % 25 == 0 or idx == len(files):
                print(f"progress {idx}/{len(files)}", flush=True)

    frame_z = [row["index_z"] for row in frame_rows]
    source = [row["source_gz"] for row in frame_rows]
    xor_z = [row["xor_z"] for row in delta_rows]
    mod_z = [row["mod_z"] for row in delta_rows]
    changed_pct = [row["changed_pct"] for row in delta_rows]

    report = {
        "frames": len(frame_rows),
        "deltas": len(delta_rows),
        "source_tif_gz_median_mb": mb(statistics.median(source)),
        "indexed_frame_zlib_median_mb": mb(statistics.median(frame_z)),
        "indexed_frame_zlib_p90_mb": mb(percentile(frame_z, 90)),
        "xor_delta_zlib_median_mb": mb(statistics.median(xor_z)),
        "xor_delta_zlib_p90_mb": mb(percentile(xor_z, 90)),
        "mod_delta_zlib_median_mb": mb(statistics.median(mod_z)),
        "mod_delta_zlib_p90_mb": mb(percentile(mod_z, 90)),
        "changed_pixels_median_pct": statistics.median(changed_pct),
        "changed_pixels_p90_pct": percentile(changed_pct, 90),
        "mod_delta_vs_indexed_frame_median": statistics.median([d / f["index_z"] for d, f in zip(mod_z, frame_rows[1:])]),
        "xor_delta_vs_indexed_frame_median": statistics.median([d / f["index_z"] for d, f in zip(xor_z, frame_rows[1:])]),
    }

    report_path = OUTPUT_ROOT / "whole-day-index-delta-report.json"
    rows_path = OUTPUT_ROOT / "whole-day-index-delta-rows.json"
    report_path.write_text(json.dumps(report, indent=2))
    rows_path.write_text(json.dumps({"frames": frame_rows, "deltas": delta_rows}, indent=2))

    print("RESULT")
    for key, value in report.items():
        print(f"{key} {value}")
    print("delta_hist_mod_z_kib")
    hist = Counter(round(row["mod_z"] / 1024) for row in delta_rows)
    for size_kib, count in sorted(hist.items()):
        print(f"{size_kib} {count}")
    print(f"saved_report {report_path}")
    print(f"saved_rows {rows_path}")


if __name__ == "__main__":
    main()
