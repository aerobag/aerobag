#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
NAV_KV_CONSUMERS = (
    "product/preprocessor/live-feeds-daemon/src/main.rs",
    "product/preprocessor/nms-notams-fetch/src/rendezvous_audit.rs",
    "product/preprocessor/preprocessor-cli/src/bin/had_query.rs",
    "product/preprocessor/preprocessor-cli/src/bin/had_validate.rs",
    "product/preprocessor/preprocessor-live-feeds/src/engine.rs",
    "ui/core-rust/crates/app-fixtures/src/lib.rs",
)


class NavKvDirectoryBoundaryTest(unittest.TestCase):
    def test_consumers_use_the_shared_directory_reader(self) -> None:
        for relative in NAV_KV_CONSUMERS:
            source = (ROOT / relative).read_text(encoding="utf-8")
            self.assertIn("NavKvDirectoryReader", source, relative)
            self.assertNotIn("fn read_nav_kv_page_from_dir", source, relative)
            self.assertNotIn("fn read_nav_page(", source, relative)

    def test_no_direct_inline_page_file_reads_are_introduced(self) -> None:
        direct_read = re.compile(r'fs::read\([^\n]*format!\("page_')
        offenders = []
        for source_root in (ROOT / "product/preprocessor", ROOT / "ui/core-rust/crates"):
            for path in source_root.rglob("*.rs"):
                for line_number, line in enumerate(
                    path.read_text(encoding="utf-8").splitlines(), start=1
                ):
                    if direct_read.search(line):
                        offenders.append(f"{path.relative_to(ROOT)}:{line_number}")
        self.assertEqual([], offenders)


if __name__ == "__main__":
    unittest.main()
