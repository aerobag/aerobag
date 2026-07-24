# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import re
import sys

import pypdf


def main() -> int:
    if len(sys.argv) < 3:
        print("usage: find_plate_pages.py <pdf-path> <apt-id> [<apt-id> ...]", file=sys.stderr)
        return 2

    pdf_path = sys.argv[1]
    apt_ids = sorted(set(sys.argv[2:]))
    reader = pypdf.PdfReader(pdf_path)
    patterns = {
        apt_id: re.compile(rf"\({re.escape(apt_id)}\)|\(K{re.escape(apt_id)}\)")
        for apt_id in apt_ids
    }
    matches = {apt_id: [] for apt_id in apt_ids}

    for index, page in enumerate(reader.pages):
        text = page.extract_text() or ""
        for apt_id, pattern in patterns.items():
            if pattern.search(text):
                matches[apt_id].append(index)

    import json

    print(json.dumps(matches, sort_keys=True, separators=(",", ":")))

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
