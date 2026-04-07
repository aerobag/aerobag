import re
import sys

import pypdf


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: find_plate_pages.py <pdf-path> <apt-id>", file=sys.stderr)
        return 2

    pdf_path = sys.argv[1]
    apt_id = sys.argv[2]
    reader = pypdf.PdfReader(pdf_path)
    pattern = re.compile(rf"\({re.escape(apt_id)}\)|\(K{re.escape(apt_id)}\)")

    for index, page in enumerate(reader.pages):
        text = page.extract_text() or ""
        if pattern.search(text):
            print(index)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
