# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import math
import sys
from collections import Counter

import pypdf


def bucket_angle(angle: float) -> int:
    return (round((angle % 360) / 90) * 90) % 360


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: detect_landscape_rotation.py <pdf-path>", file=sys.stderr)
        return 2

    reader = pypdf.PdfReader(sys.argv[1])
    if not reader.pages:
        print("0")
        return 0

    page = reader.pages[0]
    counts: Counter[int] = Counter()

    def visitor(text, cm, tm, font_dict, font_size):
        a, b, c, d, _, _ = tm
        clean = (text or "").strip()
        if not clean:
            return
        angle = bucket_angle(math.degrees(math.atan2(b, a)))
        counts[angle] += len(clean)

    try:
        page.extract_text(visitor_text=visitor)
    except TypeError:
        page.extract_text(visitor)

    if not counts:
        print("0")
        return 0

    dominant_angle, dominant_weight = counts.most_common(1)[0]
    total_weight = sum(counts.values())
    ratio = dominant_weight / total_weight if total_weight else 0.0

    if dominant_angle in (90, 270) and ratio >= 0.7:
        print(dominant_angle)
    else:
        print("0")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
