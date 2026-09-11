# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Extract six historical regression cases from a verified pixel-review capture."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil

import numpy as np
from osgeo import gdal
from PIL import Image


FRAME_TIMES = (
    '20260908T214639Z', '20260910T005241Z', '20260910T011240Z',
    '20260910T052442Z', '20260911T042443Z', '20260911T111239Z',
)


def extract(capture, output):
    review = json.loads((capture / 'site/review.json').read_text())
    output.mkdir(parents=True, exist_ok=False)
    cases = []
    for timestamp in FRAME_TIMES:
        frame, = [frame for frame in review['frames'] if frame['state_id'].startswith(timestamp)]
        assert frame['verified']
        source = capture / 'evidence/sources' / frame['source_file']
        source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
        assert source_hash == frame['cache_metadata']['sha256']
        level = frame['levels'][0]
        worst = max(level['flagged_pixels'], key=lambda pixel: pixel['error'])
        x, y = worst['x'] // 512, worst['y'] // 512
        x0, y0 = x * 512, y * 512
        width, height = min(512, level['width'] - x0), min(512, level['height'] - y0)
        dataset = gdal.Open('/vsigzip/' + str(source.resolve()))
        assert dataset.RasterCount == 4
        rgba = np.moveaxis(dataset.ReadAsArray(x0, y0, width, height), 0, -1)
        assert rgba.dtype == np.uint8
        dataset = None
        filename = f'{timestamp}-res0-{x}-{y}.png'
        Image.fromarray(rgba, 'RGBA').save(output / filename, optimize=True)
        baseline = capture / 'site/frames' / frame['state_id'] / f'tiles/res0/{x}/{y}.png'
        assert hashlib.sha256(baseline.read_bytes()).hexdigest() == level['tile_sha256'][f'tiles/res0/{x}/{y}.png']
        baseline_name = f'{timestamp}-res0-{x}-{y}-before.png'
        shutil.copyfile(baseline, output / baseline_name)
        with Image.open(baseline) as image:
            decoded = np.asarray(image.convert('RGBA'))
            entries = len(image.getpalette()) // 3
        assert np.array_equal(rgba[:, :, 3], decoded[:, :, 3])
        errors = np.abs(rgba[:, :, :3].astype(np.int16) - decoded[:, :, :3]).max(axis=2)
        bad = (errors > review['threshold']) & (rgba[:, :, 3] > 0)
        cases.append({
            'file': filename, 'sha256': hashlib.sha256((output / filename).read_bytes()).hexdigest(),
            'before_file': baseline_name, 'before_sha256': hashlib.sha256(baseline.read_bytes()).hexdigest(),
            'source_url': frame['cache_metadata']['url'], 'source_sha256': source_hash,
            'observed_at_utc': frame['observed_at_utc'], 'state_id': frame['state_id'],
            'source_grid': frame['source_grid'], 'crop': [x0, y0, width, height],
            'palette_sha256': frame['palette_sha256'], 'tiler_sha256': frame['tiler_sha256'],
            'poor_color_match_count_before': int(bad.sum()), 'error_max_before': int(errors[bad].max()),
            'palette_entries_before': entries,
            'exception_colors': int(len(np.unique(rgba[:, :, :3][bad], axis=0))),
        })
    (output / 'manifest.json').write_text(json.dumps({'schema_version': 1, 'cases': cases}, indent=2) + '\n')
    print(f'Extracted {len(cases)} cases, {sum(path.stat().st_size for path in output.iterdir())} bytes')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('capture', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    extract(args.capture, args.output)
