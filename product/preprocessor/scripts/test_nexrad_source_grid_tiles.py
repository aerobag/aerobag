# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Pixel-level regression tests using historical NOAA tiles, without network access."""

import hashlib
import gzip
import importlib.util
import json
from pathlib import Path
import sys

import numpy as np
from PIL import Image
import pytest


ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'product/preprocessor/preprocessor-live-feeds/src/nexrad_source_grid_tiles.py'
FIXTURES = SOURCE.parent.parent / 'tests/fixtures/nexrad-palette'
PALETTE = ROOT / 'docs/nexrad/analysis/whole-day-greedy-255-palette.json'
CASES = json.loads((FIXTURES / 'manifest.json').read_text())['cases']
spec = importlib.util.spec_from_file_location('nexrad_tiler', SOURCE)
tiler = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tiler)


def read_png(path):
    with Image.open(path) as image:
        return np.asarray(image.convert('RGBA'))


def errors(original, decoded):
    assert np.array_equal(original[:, :, 3], decoded[:, :, 3])
    result = np.abs(original[:, :, :3].astype(np.int16) - decoded[:, :, :3]).max(axis=2)
    result[original[:, :, 3] == 0] = 0
    return result


def encode(rgba, output, palette=None, res=0, tile_size=512):
    if palette is None:
        palette = tiler.load_fixed_palette(PALETTE)[0]
    level = tiler.write_tiles(rgba, output, res, tile_size, [-130, .01, 0, 55, 0, -.01], False, palette)
    decoded = np.zeros((level['height'], level['width'], 4), dtype=np.uint8)
    for y in range(level['tile_rows']):
        for x in range(level['tile_cols']):
            tile = read_png(output / f'tiles/res{res}/{x}/{y}.png')
            h, w = tile.shape[:2]
            decoded[y * tile_size:y * tile_size + h, x * tile_size:x * tile_size + w] = tile
    actual_errors = errors(rgba[::1 << res, ::1 << res], decoded)
    # Assert the delivered pixels BEFORE trusting the encoder's quality metadata.
    assert actual_errors.max() <= 8
    opaque_errors = actual_errors[rgba[::1 << res, ::1 << res, 3] > 0]
    quality = level['quality']
    assert quality['palette_error_max'] == actual_errors.max()
    assert quality['palette_error_p95'] == (np.percentile(opaque_errors, 95) if opaque_errors.size else 0)
    assert quality['poor_color_match_count'] == 0
    return decoded, level


@pytest.mark.parametrize('case', CASES, ids=lambda case: case['file'])
def test_historical_warning_tile_has_bounded_decoded_error(case, tmp_path):
    for filename, digest in [('file', 'sha256'), ('before_file', 'before_sha256')]:
        assert hashlib.sha256((FIXTURES / case[filename]).read_bytes()).hexdigest() == case[digest]
    assert hashlib.sha256(PALETTE.read_bytes()).hexdigest() == case['palette_sha256']
    original = read_png(FIXTURES / case['file'])
    before = read_png(FIXTURES / case['before_file'])
    before_errors = errors(original, before)
    assert before_errors.max() == case['error_max_before']
    bad = before_errors > 8
    assert bad.sum() == case['poor_color_match_count_before']
    decoded, level = encode(original, tmp_path)
    assert np.array_equal(decoded[bad], original[bad])
    assert np.array_equal(decoded[~bad], before[~bad])
    with Image.open(tmp_path / 'tiles/res0/0/0.png') as image:
        assert image.mode == 'P'
        assert len(image.getpalette()) // 3 == case['palette_entries_before'] + case['exception_colors']
    assert level['quality']['repaired_pixel_count'] == bad.sum()
    assert level['quality']['rgba_tile_count'] == 0
    # Catch accidentally making entire warning tiles lossless RGBA or using a huge palette.
    assert (tmp_path / 'tiles/res0/0/0.png').stat().st_size <= (FIXTURES / case['before_file']).stat().st_size + 100


def red_palette():
    palette = np.zeros((256, 3), dtype=np.uint8)
    palette[1:, 0] = np.arange(255)
    return palette


@pytest.mark.parametrize('base_colors,transparent,expected_mode', [
    (15, True, 'P'), (254, True, 'P'), (255, False, 'P'), (255, True, 'RGBA'),
])
def test_palette_capacity_including_transparency(base_colors, transparent, expected_mode, tmp_path):
    palette = red_palette()
    pixels = [[*rgb, 255] for rgb in palette[1:base_colors + 1]]
    if transparent:
        pixels.append([0, 0, 0, 0])
    pixels.append([0, 0, 200, 255])
    original = np.array([pixels], dtype=np.uint8)
    decoded, level = encode(original, tmp_path, palette)
    assert np.array_equal(decoded, original)
    with Image.open(tmp_path / 'tiles/res0/0/0.png') as image:
        assert image.mode == expected_mode
        if expected_mode == 'P':
            assert len(image.getpalette()) // 3 == len(pixels)
    assert level['quality']['rgba_tile_count'] == int(expected_mode == 'RGBA')


def test_exact_threshold_and_only_exception_pixels_change(tmp_path):
    original = np.array([[[100, 8, 0, 255], [100, 9, 0, 255], [0, 0, 0, 0]]], dtype=np.uint8)
    decoded, level = encode(original, tmp_path, red_palette())
    # L-infinity ties use the first base-palette entry, red=92 in this case.
    assert decoded.tolist() == [[[92, 0, 0, 255], [100, 9, 0, 255], [0, 0, 0, 0]]]
    assert level['quality']['repaired_pixel_count'] == 1


@pytest.mark.parametrize('alpha', [0, 1, 128, 254, 255])
def test_alpha_is_preserved(alpha, tmp_path):
    original = np.full((3, 5, 4), [100, 0, 0, alpha], dtype=np.uint8)
    decoded, _ = encode(original, tmp_path, red_palette())
    assert np.all(decoded[:, :, 3] == alpha)


def test_every_resolution_and_partial_edge_tiles(tmp_path):
    original = np.full((519, 521, 4), [100, 0, 0, 255], dtype=np.uint8)
    original[::8, ::8] = [0, 0, 200, 255]
    for res in range(4):
        decoded, _ = encode(original, tmp_path, red_palette(), res=res)
        assert np.array_equal(decoded, original[::1 << res, ::1 << res])


def test_mixed_indexed_and_rgba_tiles_in_one_level(tmp_path):
    palette = red_palette()
    original = np.zeros((2, 1024, 4), dtype=np.uint8)
    original[0, :255, :3] = palette[1:]
    original[0, :255, 3] = 255
    original[0, 255] = [0, 0, 200, 255]
    decoded, level = encode(original, tmp_path, palette)
    assert np.array_equal(decoded, original)
    assert level['quality']['rgba_tile_count'] == 1
    with Image.open(tmp_path / 'tiles/res0/0/0.png') as image:
        assert image.mode == 'RGBA'
    with Image.open(tmp_path / 'tiles/res0/1/0.png') as image:
        assert image.mode == 'P'


def test_written_png_is_verified_not_just_palette_math(tmp_path, monkeypatch):
    save = Image.Image.save

    def corrupted_save(image, path, *args, **kwargs):
        wrong = Image.new('RGBA', image.size, (0, 200, 0, 255))
        save(wrong, path, 'PNG')

    monkeypatch.setattr(Image.Image, 'save', corrupted_save)
    original = np.array([[[100, 0, 0, 255]]], dtype=np.uint8)
    with pytest.raises(ValueError, match='NEXRAD.*color error'):
        encode(original, tmp_path, red_palette())


def test_manifest_describes_the_delivered_encoding(tmp_path, monkeypatch):
    original = read_png(FIXTURES / CASES[1]['file'])
    source = tmp_path / 'source.tif'
    dataset = tiler.gdal.GetDriverByName('GTiff').Create(str(source), original.shape[1], original.shape[0], 4)
    dataset.SetGeoTransform([-130, .01, 0, 55, 0, -.01])
    for band in range(4):
        dataset.GetRasterBand(band + 1).WriteArray(original[:, :, band])
    dataset = None
    source_gz = tmp_path / 'source.tif.gz'
    with gzip.open(source_gz, 'wb') as target:
        target.write(source.read_bytes())
    output = tmp_path / 'output'
    output.mkdir()
    source_hash = hashlib.sha256(source_gz.read_bytes()).hexdigest()
    encoder_hash = hashlib.sha256(SOURCE.read_bytes()).hexdigest()
    monkeypatch.setattr(sys, 'argv', [
        str(SOURCE), '--palette', str(PALETTE), '--source-gz', str(source_gz),
        '--output-dir', str(output), '--state-id', 'test-state',
        '--observed-at-utc', CASES[1]['observed_at_utc'], '--source-file', source_gz.name,
        '--source-sha256', source_hash, '--encoder-sha256', encoder_hash,
        '--tile-size', '512', '--res-level', '0', '--res-level', '1',
        '--res-level', '2', '--res-level', '3',
    ])
    tiler.main()
    manifest = json.loads((output / 'manifest.json').read_text())
    assert manifest['schema_version'] == 2
    assert manifest['tile_encoding'] == 'png-bounded-palette-v1'
    assert manifest['encoder_sha256'] == encoder_hash
    assert manifest['quantization'] == {
        'base_palette_sha256': CASES[1]['palette_sha256'],
        'max_rgb_channel_error': 8, 'overflow_encoding': 'rgba8',
    }
    assert 'palette' not in manifest
    assert manifest['source_sha256'] == source_hash
    assert manifest['quality']['poor_color_match_count'] == 0
    assert manifest['quality']['palette_error_max'] <= 8
    assert manifest['quality']['repaired_pixel_count'] >= 2
    assert manifest['quality']['rgba_tile_count'] == 0
    assert manifest['res-levels'] == [0, 1, 2, 3]
    assert errors(original, read_png(output / 'tiles/res0/0/0.png')).max() <= 8


@pytest.mark.parametrize('count', [1, 2, 19, 20, 21, 1000])
def test_quality_histogram_matches_numpy_percentile(count):
    values = np.random.default_rng(123).integers(0, 9, count)
    quality = tiler.quality_from_histogram(np.bincount(values, minlength=256))
    assert quality['palette_error_p95'] == np.percentile(values, 95)


def test_healthy_tile_keeps_existing_png_bytes(tmp_path):
    # Re-encoding a delivered indexed tile has no exceptions to add.
    case = CASES[1]
    encode(read_png(FIXTURES / case['before_file']), tmp_path)
    assert (tmp_path / 'tiles/res0/0/0.png').read_bytes() == (FIXTURES / case['before_file']).read_bytes()
