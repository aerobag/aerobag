#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import argparse
import glob
import os
import sys
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class TestCase:
    suite: str
    name: str
    classname: str
    seconds: float
    status: str
    detail: str


def local_name(tag: str) -> str:
    return tag.rsplit("}", 1)[-1]


def parse_report(path: Path) -> list[TestCase]:
    root = ET.parse(path).getroot()
    suites = [root] if local_name(root.tag) == "testsuite" else [
        node for node in root.iter() if local_name(node.tag) == "testsuite"
    ]
    cases: list[TestCase] = []
    for suite in suites:
        suite_name = suite.attrib.get("name", path.stem)
        for node in suite:
            if local_name(node.tag) != "testcase":
                continue
            failure = next(
                (
                    child
                    for child in node
                    if local_name(child.tag) in {"failure", "error"}
                ),
                None,
            )
            skipped = next(
                (child for child in node if local_name(child.tag) == "skipped"),
                None,
            )
            if failure is not None:
                status = "failed"
                detail = failure.attrib.get("message") or (failure.text or "").strip()
            elif skipped is not None:
                status = "skipped"
                detail = skipped.attrib.get("message") or (skipped.text or "").strip()
            else:
                status = "passed"
                detail = ""
            try:
                seconds = float(node.attrib.get("time", "0"))
            except ValueError:
                seconds = 0.0
            cases.append(
                TestCase(
                    suite=suite_name,
                    name=node.attrib.get("name", "<unnamed>"),
                    classname=node.attrib.get("classname", ""),
                    seconds=seconds,
                    status=status,
                    detail=detail,
                )
            )
    return cases


def markdown_escape(value: str) -> str:
    return value.replace("\\", "\\\\").replace("|", "\\|").replace("\n", " ")


def annotation_escape(value: str) -> str:
    return (
        value.replace("%", "%25")
        .replace("\r", "%0D")
        .replace("\n", "%0A")
        .replace(",", "%2C")
    )


def render_summary(title: str, cases: list[TestCase], paths: list[Path]) -> str:
    counts = {
        status: sum(case.status == status for case in cases)
        for status in ("passed", "failed", "skipped")
    }
    lines = [
        f"## {title}",
        "",
        (
            f"**{len(cases)} tests:** {counts['passed']} passed, "
            f"{counts['failed']} failed, {counts['skipped']} skipped."
        ),
        "",
    ]
    failures = [case for case in cases if case.status == "failed"]
    if failures:
        lines.extend(["### Failures", "", "| Test | Time | Detail |", "|---|---:|---|"])
        for case in failures:
            label = f"{case.classname}.{case.name}" if case.classname else case.name
            detail = case.detail.splitlines()[0] if case.detail else "No failure detail"
            lines.append(
                f"| `{markdown_escape(label)}` | {case.seconds:.3f}s | "
                f"{markdown_escape(detail)} |"
            )
        lines.append("")
    lines.extend(
        [
            "<details>",
            "<summary>All test cases</summary>",
            "",
            "| Result | Suite | Test | Time |",
            "|---|---|---|---:|",
        ]
    )
    symbols = {"passed": "PASS", "failed": "FAIL", "skipped": "SKIP"}
    for case in sorted(cases, key=lambda value: (value.suite, value.classname, value.name)):
        label = f"{case.classname}.{case.name}" if case.classname else case.name
        lines.append(
            f"| {symbols[case.status]} | {markdown_escape(case.suite)} | "
            f"`{markdown_escape(label)}` | {case.seconds:.3f}s |"
        )
    lines.extend(
        [
            "",
            "</details>",
            "",
            "Reports: " + ", ".join(f"`{path}`" for path in paths),
            "",
        ]
    )
    return "\n".join(lines)


def report_paths(patterns: list[str]) -> list[Path]:
    paths = {
        Path(match)
        for pattern in patterns
        for match in glob.glob(pattern, recursive=True)
        if Path(match).is_file()
    }
    return sorted(paths)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--title", required=True)
    parser.add_argument("reports", nargs="+")
    args = parser.parse_args()

    paths = report_paths(args.reports)
    if not paths:
        message = f"No JUnit reports were produced for {args.title}."
        print(f"::warning::{annotation_escape(message)}")
        summary = f"## {args.title}\n\n{message}\n"
    else:
        cases = [case for path in paths for case in parse_report(path)]
        for case in cases:
            if case.status != "failed":
                continue
            label = f"{case.classname}.{case.name}" if case.classname else case.name
            detail = case.detail or "No failure detail"
            print(
                f"::error title={annotation_escape(label)}::"
                f"{annotation_escape(detail)}"
            )
        summary = render_summary(args.title, cases, paths)

    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary_path:
        with Path(summary_path).open("a", encoding="utf-8") as output:
            output.write(summary)
            output.write("\n")
    else:
        sys.stdout.write(summary)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
