#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Rebuild with the deployed tiler, then compare decoded PNG tiles to NOAA RGBA."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil

import numpy as np
from osgeo import gdal
from PIL import Image

ASSETS = Path(__file__).resolve().parent / 'pixel_review'
gdal.UseExceptions()


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def reconstruct(frame, evidence, site):
    state = frame['state_id']
    folder = site / 'frames' / state
    folder.mkdir(parents=True, exist_ok=True)
    source = evidence / 'sources' / frame['source_file']
    assert sha(source) == frame['cache_metadata']['sha256']
    tiler_path = evidence / (frame['commit'] + '-tiler.py')
    palette_path = evidence / (frame['commit'] + '-palette.json')
    assert sha(palette_path).startswith(state.split('_png8')[1])
    spec = importlib.util.spec_from_file_location('deployed_tiler', tiler_path)
    tiler = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(tiler)
    palette, _, _, palette_sha = tiler.load_fixed_palette(palette_path)
    dataset = gdal.Open('/vsigzip/' + str(source))
    # Keep the actual source samples, including alpha. Check the deployed reader
    # agrees; otherwise the review must distinguish decoding from quantization.
    assert dataset.RasterCount == 4
    original = np.dstack([dataset.GetRasterBand(i).ReadAsArray() for i in range(1, 5)])
    assert np.array_equal(original, tiler.read_rgba(dataset)), 'NOAA decoder changed source RGBA'
    transform = dataset.GetGeoTransform()
    frame.update(source_grid=dict(width=dataset.RasterXSize, height=dataset.RasterYSize,
                                  geo_transform=transform, projection=dataset.GetProjection()),
                 palette_sha256=palette_sha, tiler_sha256=sha(tiler_path), levels=[])
    for res in range(4):
        stride = 1 << res
        raw = np.ascontiguousarray(original[::stride, ::stride])
        level = tiler.write_tiles(original, folder, res, 512, transform, False, palette)
        height, width = raw.shape[:2]
        compressed = np.zeros_like(raw)
        tile_hashes = {}
        for y in range(level['tile_rows']):
            for x in range(level['tile_cols']):
                tile_path = folder / 'tiles' / f'res{res}' / str(x) / f'{y}.png'
                with Image.open(tile_path) as image:
                    tile = np.asarray(image.convert('RGBA'))
                    compressed[y*512:y*512+tile.shape[0], x*512:x*512+tile.shape[1]] = tile
                tile_hashes[str(tile_path.relative_to(folder))] = sha(tile_path)
        opaque = raw[:, :, 3] > 0
        errors = np.max(np.abs(raw[:, :, :3].astype(np.int16) - compressed[:, :, :3].astype(np.int16)), axis=2)
        errors[~opaque] = 0
        flagged = np.argwhere(errors > tiler.POOR_COLOR_MATCH_THRESHOLD)
        assert len(flagged) == level['quality']['poor_color_match_count']
        assert int(errors.max()) == level['quality']['palette_error_max']
        assert np.array_equal(raw[:, :, 3], compressed[:, :, 3]), 'PNG alpha differs from NOAA'
        level.update(opaque_pixels=int(opaque.sum()), flagged_pixels=[], crops=[], tile_sha256=tile_hashes,
                     mean_error=float(errors[opaque].mean()),
                     changed_opaque_pixels=int(np.count_nonzero(errors)),
                     error_histogram={str(n): int(c) for n, c in zip(*np.unique(errors[opaque], return_counts=True))})
        Image.fromarray(raw).save(folder / f'res{res}-original.png')
        Image.fromarray(compressed).save(folder / f'res{res}-compressed.png')
        # A full-resolution binary mask, not a contrast-enhanced radar image.
        mask = np.zeros_like(raw)
        mask[errors > tiler.POOR_COLOR_MATCH_THRESHOLD] = [255, 0, 160, 255]
        Image.fromarray(mask).save(folder / f'res{res}-flags.png')
        for y, x in flagged:
            y, x = int(y), int(x)
            sx, sy = x * stride, y * stride
            level['flagged_pixels'].append(dict(
                x=x, y=y, source_x=sx, source_y=sy,
                lon=transform[0] + (sx + .5)*transform[1],
                lat=transform[3] + (sy + .5)*transform[5],
                original=raw[y, x].tolist(), compressed=compressed[y, x].tolist(), error=int(errors[y, x])))
            if any(c['x'] <= x < c['x']+c['width'] and c['y'] <= y < c['y']+c['height'] for c in level['crops']):
                continue
            size = 96
            x0, y0 = min(max(0, x-size//2), width-size), min(max(0, y-size//2), height-size)
            index = len(level['crops'])
            crop = dict(x=x0, y=y0, width=size, height=size, index=index)
            level['crops'].append(crop)
            for name, values in [('original', raw), ('compressed', compressed), ('flags', mask)]:
                Image.fromarray(values[y0:y0+size, x0:x0+size]).save(folder / f'res{res}-crop{index}-{name}.png')
        frame['levels'].append(level)
        print(state, 'res', res, 'flagged', len(flagged), 'max', level['quality']['palette_error_max'], flush=True)
    total = sum(l['quality']['poor_color_match_count'] for l in frame['levels'])
    maximum = max(l['quality']['palette_error_max'] for l in frame['levels'])
    assert all(s['poor_count'] == total and s['max_error'] == maximum for s in frame['samples']), frame['samples']
    frame.update(reconstructed_count=total, reconstructed_max_error=maximum, verified=True)
    write_json(folder / 'analysis.json', frame)
    return frame


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--evidence', type=Path, required=True,
                        help='Capture directory containing frames.json, sources/, and deployed tiler/palette')
    parser.add_argument('--output', type=Path, required=True, help='Public comparison site directory')
    parser.add_argument('--rebuild', action='store_true', help='Recompute existing frame analyses')
    args = parser.parse_args()
    evidence, site = args.evidence.resolve(), args.output.resolve()
    if site == evidence or site in evidence.parents:
        parser.error('output must not contain private evidence; serve only the output directory')
    site.mkdir(parents=True, exist_ok=True)
    frames = json.loads((evidence / 'frames.json').read_text())
    sample_times = []
    for path in sorted(evidence.glob('pipeline_health-*.jsonl')):
        with path.open() as stream:
            for line in stream:
                sample_times.append(json.loads(line)['sampled_at_utc'])
    for frame in frames:
        if 'cache_metadata' not in frame:
            continue
        cached = site / 'frames' / frame['state_id'] / 'analysis.json'
        if cached.exists() and not args.rebuild:
            analysis = json.loads(cached.read_text())
            assert analysis['state_id'] == frame['state_id']
            assert sha(evidence / 'sources' / frame['source_file']) == frame['cache_metadata']['sha256'] == analysis['cache_metadata']['sha256']
            assert sha(evidence / (frame['commit'] + '-tiler.py')) == analysis['tiler_sha256'], 'tiler changed; use --rebuild'
            assert sha(evidence / (frame['commit'] + '-palette.json')) == analysis['palette_sha256'], 'palette changed; use --rebuild'
            assert all(s['poor_count'] == analysis['reconstructed_count'] and s['max_error'] == analysis['reconstructed_max_error'] for s in frame['samples'])
            frame.update(analysis)
        else:
            frame.update(reconstruct(frame, evidence, site))
    for path in ASSETS.iterdir():
        if path.is_file():
            shutil.copyfile(path, site / path.name)
    write_json(site / 'review.json', dict(
        title='NEXRAD palette review', threshold=8, frames=frames,
        period_start=min(sample_times), period_end=max(sample_times),
        recovered=sum(bool(f.get('verified')) for f in frames),
        total=len(frames)))
    print('Review ready:', site / 'review.json', flush=True)


if __name__ == '__main__':
    main()
