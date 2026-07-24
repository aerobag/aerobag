#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Measure compression variants for current Aerobag ABT2 terrain tiles."""

from __future__ import annotations

import argparse
import gzip
import lzma
import random
import struct
import time
from collections import defaultdict
from pathlib import Path

import numpy as np


HEADER_BYTES = 20


def terrain_tiles(root: Path) -> list[Path]:
    tiles_root = root / "tiles" if (root / "tiles").is_dir() else root
    files = sorted(
        tiles_root.glob("*/*/*.terrain"),
        key=lambda path: (zoom_of(path), int(path.parent.name), int(path.stem)),
    )
    if not files:
        raise SystemExit(f"no .terrain tiles found under {tiles_root}")
    return files


def zoom_of(path: Path) -> int:
    return int(path.parts[-3])


def select_sample(files: list[Path], sample_per_zoom: int | None) -> list[Path]:
    if sample_per_zoom is None:
        return files
    rng = random.Random(51)
    selected: list[Path] = []
    by_zoom: dict[int, list[Path]] = defaultdict(list)
    for path in files:
        by_zoom[zoom_of(path)].append(path)
    for zoom in sorted(by_zoom):
        zoom_files = by_zoom[zoom]
        if len(zoom_files) <= sample_per_zoom:
            selected.extend(zoom_files)
        else:
            selected.extend(rng.sample(zoom_files, sample_per_zoom))
    return sorted(selected, key=lambda path: (zoom_of(path), int(path.parent.name), int(path.stem)))


def decode_gradient_delta(payload: bytes, width: int, height: int) -> np.ndarray:
    residual = np.frombuffer(payload, dtype="<u2", count=width * height).reshape((height, width))
    raw = residual.astype(np.uint32).cumsum(axis=0, dtype=np.uint32).cumsum(axis=1, dtype=np.uint32)
    return (raw & 0xFFFF).astype("<u2").view("<i2")


def parse_abt2(raw: bytes) -> tuple[bytes, np.ndarray]:
    if len(raw) < HEADER_BYTES or raw[:4] != b"ABT2":
        raise ValueError("terrain tile is not decoded ABT2")
    width, height, _nodata, _reserved, _scale, _offset = struct.unpack(
        "<HHhhff", raw[4:HEADER_BYTES]
    )
    expected = HEADER_BYTES + width * height * 2
    if len(raw) != expected:
        raise ValueError(f"invalid ABT2 tile length: expected {expected}, got {len(raw)}")
    return raw[:HEADER_BYTES], decode_gradient_delta(raw[HEADER_BYTES:expected], width, height)


def residual_payload(raw: bytes, predictor: str) -> bytes:
    header, samples = parse_abt2(raw)
    values = samples.astype("<i2", copy=False).view("<u2").astype(np.uint32)
    prediction = np.zeros_like(values)

    if predictor == "avg":
        prediction[:, 1:] = values[:, :-1]
        prediction[1:, 0] = values[:-1, 0]
        prediction[1:, 1:] = (values[1:, :-1] + values[:-1, 1:]) // 2
    elif predictor == "gradient":
        prediction[:, 1:] = values[:, :-1]
        prediction[1:, 0] = values[:-1, 0]
        prediction[1:, 1:] = (values[1:, :-1] + values[:-1, 1:] - values[:-1, :-1]) & 0xffff
    else:
        raise ValueError(f"unknown predictor {predictor}")

    residual = ((values - prediction) & 0xffff).astype("<u2")
    return header + residual.tobytes()


def encoded_sizes(raw: bytes, current_size: int, include_xz: bool) -> dict[str, int]:
    sizes = {
        "current_gzip": current_size,
        "avg_delta_gzip9": len(gzip.compress(residual_payload(raw, "avg"), compresslevel=9, mtime=0)),
        "gradient_delta_gzip9": len(
            gzip.compress(residual_payload(raw, "gradient"), compresslevel=9, mtime=0)
        ),
    }
    if include_xz:
        sizes["raw_xz6"] = len(lzma.compress(raw, preset=6))
        sizes["gradient_delta_xz6"] = len(lzma.compress(residual_payload(raw, "gradient"), preset=6))
    return sizes


def zip_overhead(root: Path, current_member_bytes: int) -> int | None:
    zips = sorted(root.glob("terrain_*.zip"))
    if len(zips) != 1:
        return None
    return zips[0].stat().st_size - current_member_bytes


def format_bytes(value: int) -> str:
    return f"{value:,}"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("terrain_root", type=Path, help="terrain output dir or its tiles dir")
    parser.add_argument(
        "--sample-per-zoom",
        type=int,
        help="measure a deterministic sample per zoom instead of every tile",
    )
    parser.add_argument("--include-xz", action="store_true", help="also run slower xz comparisons")
    args = parser.parse_args()

    root = args.terrain_root
    files = terrain_tiles(root)
    selected = select_sample(files, args.sample_per_zoom)
    totals: dict[str, int] = defaultdict(int)
    by_zoom: dict[int, dict[str, int]] = defaultdict(lambda: defaultdict(int))
    raw_total = 0
    started = time.monotonic()

    for index, path in enumerate(selected, 1):
        compressed = path.read_bytes()
        raw = gzip.decompress(compressed)
        raw_total += len(raw)
        sizes = encoded_sizes(raw, len(compressed), args.include_xz)
        zoom = zoom_of(path)
        for name, size in sizes.items():
            totals[name] += size
            by_zoom[zoom][name] += size
        if index % 500 == 0:
            elapsed = time.monotonic() - started
            print(f"processed {index}/{len(selected)} elapsed={elapsed:.1f}s", flush=True)

    current = totals["current_gzip"]
    print(f"tiles measured: {len(selected)} of {len(files)}")
    print(f"decoded ABT2 bytes: {format_bytes(raw_total)}")
    print()
    print("codec                         bytes   ratio")
    for name, total in sorted(totals.items(), key=lambda item: item[1]):
        print(f"{name:24s} {format_bytes(total):>14s} {total / current:7.3f}")

    overhead = zip_overhead(root, current)
    if overhead is not None and len(selected) == len(files):
        print()
        print(f"current zip overhead: {format_bytes(overhead)}")
        for name, total in sorted(totals.items(), key=lambda item: item[1]):
            print(f"projected_zip_{name}: {format_bytes(total + overhead)}")

    print()
    print("zoom current_gzip gradient_delta_gzip9 ratio")
    for zoom in sorted(by_zoom):
        row = by_zoom[zoom]
        gradient = row["gradient_delta_gzip9"]
        current_zoom = row["current_gzip"]
        print(f"{zoom:>4d} {format_bytes(current_zoom):>14s} {format_bytes(gradient):>20s} {gradient / current_zoom:7.3f}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
