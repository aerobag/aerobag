# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import json
import math
import sys
from collections import Counter
from pathlib import Path

import pypdf

CARDINAL_TOLERANCE_DEG = 5.0
MIN_CARDINAL_TEXT_WEIGHT = 20
MIN_DOMINANCE_RATIO = 0.7
FAA_DIAGRAM_FRAME_LEFT_INSET_PT = 18.0
FAA_DIAGRAM_FRAME_RIGHT_INSET_PT = 18.0
FAA_DIAGRAM_FRAME_BOTTOM_INSET_PT = 45.0
FAA_DIAGRAM_FRAME_TOP_INSET_PT = 44.0


def cardinal_angle(angle: float) -> int | None:
    normalized = angle % 360
    nearest = (round(normalized / 90) * 90) % 360
    distance = abs((normalized - nearest + 180) % 360 - 180)
    return nearest if distance <= CARDINAL_TOLERANCE_DEG else None


def multiply_matrices(left: list[float], right: list[float]) -> list[float]:
    return [
        left[0] * right[0] + left[1] * right[2],
        left[0] * right[1] + left[1] * right[3],
        left[2] * right[0] + left[3] * right[2],
        left[2] * right[1] + left[3] * right[3],
        left[4] * right[0] + left[5] * right[2] + right[4],
        left[4] * right[1] + left[5] * right[3] + right[5],
    ]


def is_inside_diagram_frame(
    x: float,
    y: float,
    page_left: float,
    page_bottom: float,
    page_right: float,
    page_top: float,
) -> bool:
    return (
        page_left + FAA_DIAGRAM_FRAME_LEFT_INSET_PT <= x
        <= page_right - FAA_DIAGRAM_FRAME_RIGHT_INSET_PT
        and page_bottom + FAA_DIAGRAM_FRAME_BOTTOM_INSET_PT <= y
        <= page_top - FAA_DIAGRAM_FRAME_TOP_INSET_PT
    )


def summarize_orientation(
    counts: Counter[int], non_cardinal_weight: int
) -> dict[str, object]:
    cardinal_weight = sum(counts.values())
    if cardinal_weight:
        dominant_angle, dominant_weight = counts.most_common(1)[0]
        dominance_ratio = dominant_weight / cardinal_weight
    else:
        dominant_angle = 0
        dominant_weight = 0
        dominance_ratio = 0.0

    rotation_deg = 0
    if (
        dominant_angle != 0
        and cardinal_weight >= MIN_CARDINAL_TEXT_WEIGHT
        and dominance_ratio >= MIN_DOMINANCE_RATIO
    ):
        rotation_deg = dominant_angle

    return {
        "rotation_deg": rotation_deg,
        "dominant_cardinal_deg": dominant_angle,
        "dominant_char_weight": dominant_weight,
        "cardinal_char_weight": cardinal_weight,
        "non_cardinal_char_weight": non_cardinal_weight,
        "dominance_per_mille": round(dominance_ratio * 1000),
        "cardinal_char_weights": [counts[angle] for angle in (0, 90, 180, 270)],
    }


def analyze_pdf(pdf_path: str) -> dict[str, object]:
    reader = pypdf.PdfReader(pdf_path)
    counts: Counter[int] = Counter()
    non_cardinal_weight = 0
    outside_frame_weight = 0

    for page in reader.pages:
        page_left = float(page.cropbox.left)
        page_bottom = float(page.cropbox.bottom)
        page_right = float(page.cropbox.right)
        page_top = float(page.cropbox.top)

        def visitor(text, cm, tm, font_dict, font_size):
            nonlocal non_cardinal_weight, outside_frame_weight
            clean = (text or "").strip()
            if not clean:
                return
            weight = len(clean)
            a, b, _, _, x, y = multiply_matrices(tm, cm)
            if not is_inside_diagram_frame(
                x, y, page_left, page_bottom, page_right, page_top
            ):
                outside_frame_weight += weight
                return
            angle = cardinal_angle(math.degrees(math.atan2(b, a)))
            if angle is None:
                non_cardinal_weight += weight
            else:
                counts[angle] += weight

        try:
            page.extract_text(visitor_text=visitor)
        except TypeError:
            page.extract_text(visitor)

    return {
        "path": str(Path(pdf_path)),
        "outside_frame_char_weight": outside_frame_weight,
        **summarize_orientation(counts, non_cardinal_weight),
    }


def main() -> int:
    if len(sys.argv) == 2 and sys.argv[1] == "--batch-json":
        for line in sys.stdin:
            pdf_path = line.rstrip("\n")
            if pdf_path:
                print(json.dumps(analyze_pdf(pdf_path), separators=(",", ":")))
        return 0

    json_output = len(sys.argv) == 3 and sys.argv[1] == "--json"
    if not (len(sys.argv) == 2 or json_output):
        print(
            "usage: detect_landscape_rotation.py [--json] <pdf-path> | --batch-json",
            file=sys.stderr,
        )
        return 2

    result = analyze_pdf(sys.argv[-1])
    if json_output:
        print(json.dumps(result, separators=(",", ":")))
    else:
        print(result["rotation_deg"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
