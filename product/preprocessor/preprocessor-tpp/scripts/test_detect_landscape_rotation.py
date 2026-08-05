#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import unittest
from collections import Counter

from detect_landscape_rotation import (
    cardinal_angle,
    is_inside_diagram_frame,
    summarize_orientation,
)


class DetectLandscapeRotationTests(unittest.TestCase):
    def test_only_near_cardinal_text_informs_orientation(self):
        self.assertEqual(cardinal_angle(4), 0)
        self.assertEqual(cardinal_angle(86), 90)
        self.assertEqual(cardinal_angle(355), 0)
        self.assertIsNone(cardinal_angle(6))
        self.assertIsNone(cardinal_angle(45))

    def test_nonzero_rotation_requires_seventy_percent_cardinal_majority(self):
        selected = summarize_orientation(Counter({90: 70, 0: 30}), 500)
        rejected = summarize_orientation(Counter({90: 69, 0: 31}), 0)

        self.assertEqual(selected["rotation_deg"], 90)
        self.assertEqual(rejected["rotation_deg"], 0)

    def test_all_cardinal_rotations_are_supported(self):
        self.assertEqual(
            summarize_orientation(Counter({180: 20}), 0)["rotation_deg"], 180
        )
        self.assertEqual(
            summarize_orientation(Counter({270: 20}), 0)["rotation_deg"], 270
        )

    def test_standard_title_strips_are_outside_diagram_frame(self):
        page = (0.0, 0.0, 387.36, 594.0)

        self.assertTrue(is_inside_diagram_frame(18.0, 45.0, *page))
        self.assertTrue(is_inside_diagram_frame(369.0, 550.0, *page))
        self.assertFalse(is_inside_diagram_frame(18.0, 553.0, *page))
        self.assertFalse(is_inside_diagram_frame(18.0, 32.0, *page))


if __name__ == "__main__":
    unittest.main()
