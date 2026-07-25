# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from tools.ci.junit_summary import parse_report, render_summary, report_paths


class JunitSummaryTests(unittest.TestCase):
    def test_parses_pass_failure_and_skip(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            report = Path(temp) / "junit.xml"
            report.write_text(
                """
                <testsuites>
                  <testsuite name="sample">
                    <testcase classname="alpha" name="passes" time="0.1"/>
                    <testcase classname="alpha" name="fails" time="0.2">
                      <failure message="expected true">trace</failure>
                    </testcase>
                    <testcase classname="alpha" name="skips">
                      <skipped message="fixture unavailable"/>
                    </testcase>
                  </testsuite>
                </testsuites>
                """,
                encoding="utf-8",
            )

            cases = parse_report(report)

            self.assertEqual(
                [(case.name, case.status) for case in cases],
                [("passes", "passed"), ("fails", "failed"), ("skips", "skipped")],
            )
            summary = render_summary("Sample", cases, [report])
            self.assertIn("1 passed, 1 failed, 1 skipped", summary)
            self.assertIn("expected true", summary)

    def test_report_paths_expands_recursive_globs(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            report = Path(temp) / "nested" / "junit.xml"
            report.parent.mkdir()
            report.write_text("<testsuite/>", encoding="utf-8")

            self.assertEqual(report_paths([f"{temp}/**/*.xml"]), [report])


if __name__ == "__main__":
    unittest.main()
