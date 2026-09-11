# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

from io import BytesIO
import unittest

import numpy as np
from PIL import Image

from compare_nexrad_encodings import color_errors, png_bytes, repair_tile, transparency_bytes


def indexed(indices, palette, transparent=False):
    image = Image.fromarray(np.asarray(indices, dtype=np.uint8), 'P')
    image.putpalette(np.asarray(palette, dtype=np.uint8).reshape(-1).tolist())
    kwargs = {'transparency': bytes([0] + [255]*(len(palette)-1))} if transparent else {}
    data = png_bytes(image, **kwargs)
    decoded = Image.open(BytesIO(data))
    decoded.load()
    return decoded, data


class PaletteRepairExperimentTests(unittest.TestCase):
    def test_repairs_only_pixels_above_threshold_and_preserves_alpha(self):
        image, data = indexed([[0, 1, 1, 1]], [[0, 0, 0], [100, 100, 100]], True)
        original = np.asarray(image.convert('RGBA')).copy()
        original[0, 1, :3] = 108
        original[0, 2:, :3] = 109
        result, stats = repair_tile(original, image, 8)
        actual = np.asarray(Image.open(BytesIO(result)).convert('RGBA'))
        self.assertEqual(stats['encoding'], 'indexed')
        self.assertEqual(stats['exception_colors'], 1)
        self.assertEqual(stats['max_error_after'], 8)
        self.assertEqual(actual[0, 1].tolist(), [100, 100, 100, 255])
        np.testing.assert_array_equal(actual[0, 2:], original[0, 2:])
        np.testing.assert_array_equal(actual[:, :, 3], original[:, :, 3])
        self.assertEqual(png_bytes(image, **transparency_bytes(image, 2)), data)

    def test_full_palette_uses_lossless_rgba_without_relaxing_error_bound(self):
        image, _ = indexed([list(range(256))], [[i, i, i] for i in range(256)])
        original = np.asarray(image.convert('RGBA')).copy()
        original[0, 0] = [255, 0, 255, 255]
        result, stats = repair_tile(original, image, 8)
        actual = Image.open(BytesIO(result))
        self.assertEqual(actual.mode, 'RGBA')
        self.assertEqual(stats['encoding'], 'rgba')
        np.testing.assert_array_equal(np.asarray(actual), original)

    def test_palette_bit_depth_transition_is_still_lossless(self):
        image, baseline = indexed([list(range(16))], [[i*10, i*10, i*10] for i in range(16)])
        original = np.asarray(image.convert('RGBA')).copy()
        original[0, 0] = [255, 0, 255, 255]
        result, stats = repair_tile(original, image, 8)
        self.assertEqual(baseline[24], 4)
        self.assertEqual(result[24], 8)
        self.assertEqual(stats['encoding'], 'indexed')
        np.testing.assert_array_equal(np.asarray(Image.open(BytesIO(result)).convert('RGBA')), original)

    def test_transparent_rgb_does_not_count_as_color_error(self):
        original = np.array([[[255, 255, 255, 0]]], dtype=np.uint8)
        decoded = np.array([[[0, 0, 0, 0]]], dtype=np.uint8)
        self.assertEqual(int(color_errors(original, decoded).max()), 0)


if __name__ == '__main__':
    unittest.main()
