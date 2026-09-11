#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Measure tile-local PNG alternatives against a reconstructed pixel review.

This is an experiment, not the production encoder. No publication is modified.
"""

import argparse
from io import BytesIO
import json
from pathlib import Path

import numpy as np
from PIL import Image


def png_bytes(image, **kwargs):
    output = BytesIO()
    image.save(output, 'PNG', optimize=True, **kwargs)
    return output.getvalue()


def color_errors(original, decoded):
    error = np.max(np.abs(original[:, :, :3].astype(np.int16)
                          - decoded[:, :, :3].astype(np.int16)), axis=2)
    error[original[:, :, 3] == 0] = 0
    return error


def transparency_bytes(encoded, entries):
    """Match the deployed writer's full-length tRNS, not Pillow's compact read form."""
    value = encoded.info.get('transparency')
    if value is None:
        return {}
    alpha = bytearray([255] * entries)
    if isinstance(value, int):
        alpha[value] = 0
    else:
        alpha[:len(value)] = value
    return dict(transparency=bytes(alpha))


def repair_tile(original, encoded, threshold):
    """Keep existing indices and add exact exceptions when the palette fits."""
    assert encoded.mode == 'P'
    decoded = np.asarray(encoded.convert('RGBA'))
    assert np.array_equal(original[:, :, 3], decoded[:, :, 3])
    errors = color_errors(original, decoded)
    bad = errors > threshold
    expected = decoded.copy()
    expected[bad] = original[bad]
    palette = np.asarray(encoded.getpalette(), dtype=np.uint8).reshape(-1, 3)
    exceptions, inverse = np.unique(original[bad, :3], axis=0, return_inverse=True)
    if len(palette) + len(exceptions) > 256:
        result = png_bytes(Image.fromarray(original))
        expected = original
        encoding = 'rgba'
    else:
        indices = np.asarray(encoded).copy()
        indices[bad] = (len(palette) + inverse).astype(np.uint8)
        repaired = Image.fromarray(indices, 'P')
        repaired.putpalette(np.concatenate([palette, exceptions]).reshape(-1).tolist())
        kwargs = transparency_bytes(encoded, len(palette) + len(exceptions))
        result = png_bytes(repaired, **kwargs)
        encoding = 'indexed'
    with Image.open(BytesIO(result)) as image:
        actual = np.asarray(image.convert('RGBA'))
    assert np.array_equal(actual, expected), 'encoded PNG differs from intended pixels'
    assert color_errors(original, actual).max() <= threshold
    return result, dict(encoding=encoding, palette_entries_before=len(palette),
                       exception_colors=len(exceptions), max_error_after=int(color_errors(original, actual).max()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--review', type=Path, required=True, help='Reconstructed review.json')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    review = json.loads(args.review.read_text())
    root = args.review.parent
    frames = []
    for frame in review['frames']:
        if not frame.get('verified'):
            continue
        directory = root / 'frames' / frame['state_id']
        baseline = sum(path.stat().st_size for path in (directory / 'tiles').rglob('*.png'))
        result = dict(state_id=frame['state_id'], baseline_png_bytes=baseline,
                      affected_tile_rgba_bytes=baseline, exception_palette_bytes=baseline,
                      tile_count=sum(l['tile_cols']*l['tile_rows'] for l in frame['levels']),
                      affected_tiles=[])
        for level in frame['levels']:
            res = level['res']
            affected = sorted({(p['x']//512, p['y']//512) for p in level['flagged_pixels']})
            if not affected:
                continue
            with Image.open(directory / f'res{res}-original.png') as source:
                for x, y in affected:
                    tile_path = directory / 'tiles' / f'res{res}' / str(x) / f'{y}.png'
                    with Image.open(tile_path) as encoded:
                        assert png_bytes(encoded, **transparency_bytes(encoded, len(encoded.getpalette())//3)) == tile_path.read_bytes(), 'baseline writer must match before comparing sizes'
                        original = np.asarray(source.crop((x*512, y*512, x*512+encoded.width, y*512+encoded.height)))
                        rgba_bytes = png_bytes(Image.fromarray(original))
                        with Image.open(BytesIO(rgba_bytes)) as image:
                            assert np.array_equal(original, np.asarray(image.convert('RGBA')))
                        repaired, repair = repair_tile(original, encoded, review['threshold'])
                    before = tile_path.stat().st_size
                    result['affected_tile_rgba_bytes'] += len(rgba_bytes) - before
                    result['exception_palette_bytes'] += len(repaired) - before
                    result['affected_tiles'].append(dict(
                        res=res, x=x, y=y, baseline_png_bytes=before,
                        lossless_rgba_bytes=len(rgba_bytes), repaired_bytes=len(repaired), **repair))
        frames.append(result)
        print(frame['observed_at_utc'], 'tiles', len(result['affected_tiles']),
              'RGBA extra', result['affected_tile_rgba_bytes']-baseline,
              'palette extra', result['exception_palette_bytes']-baseline, flush=True)
    totals = {key: sum(f[key] for f in frames) for key in [
        'baseline_png_bytes', 'affected_tile_rgba_bytes', 'exception_palette_bytes', 'tile_count']}
    totals['affected_tiles'] = sum(len(f['affected_tiles']) for f in frames)
    report = dict(threshold=review['threshold'], frame_count=len(frames), totals=totals, frames=frames,
                  scope='PNG payload bytes, all four resolutions. Only flagged tiles replaced. '
                        'No HTTP, manifest, package-container overhead, or frame-to-frame delta modeled. '
                        'These frames were selected for warnings, not as representative traffic.')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(totals, indent=2))


if __name__ == '__main__':
    main()
