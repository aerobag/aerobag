#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import argparse
import math
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


WORK_TAG = re.compile(r"AerobagSessionWork(?:\([^)]*\))?:\s+(.*)$")
SCENARIO_TAG = re.compile(r"AerobagPerfScenario(?:\([^)]*\))?:\s+(.*)$")


def parse_fields(message: str) -> dict[str, str]:
    fields: dict[str, str] = {}
    for token in message.split():
        if "=" not in token:
            continue
        name, value = token.split("=", 1)
        fields[name] = value
    return fields


def integer(record: dict[str, str], name: str) -> int | None:
    try:
        return int(record[name])
    except (KeyError, ValueError):
        return None


def percentile(values: list[int], fraction: float) -> int:
    ordered = sorted(values)
    if not ordered:
        return 0
    index = max(0, min(len(ordered) - 1, math.ceil(len(ordered) * fraction) - 1))
    return ordered[index]


@dataclass(frozen=True)
class Analysis:
    work: tuple[dict[str, str], ...]
    scenario_messages: tuple[str, ...]
    failures: tuple[str, ...]

    @property
    def passed(self) -> bool:
        return not self.failures


def analyze_lines(
    lines: Iterable[str],
    scenario: str,
    max_selection_queue_ms: int = 100,
    max_selection_landing_ms: int = 50,
    max_active_work: int = 2,
) -> Analysis:
    work: list[dict[str, str]] = []
    scenario_messages: list[str] = []
    fatal_lines: list[str] = []
    instrumentation_enabled = False
    scenario_started = False
    scenario_finished = False
    for line in lines:
        scenario_match = SCENARIO_TAG.search(line)
        if scenario_match:
            message = scenario_match.group(1)
            if message.startswith(f"start scenario={scenario}"):
                scenario_started = True
            if scenario_started and not scenario_finished:
                scenario_messages.append(message)
                if message.startswith(f"frame_summary scenario={scenario}"):
                    scenario_finished = True

        work_match = WORK_TAG.search(line)
        if work_match:
            record = parse_fields(work_match.group(1))
            if record.get("event") == "instrumentation_enabled":
                instrumentation_enabled = True
                work.append(record)
            elif scenario_started and not scenario_finished:
                work.append(record)
        if "FATAL EXCEPTION" in line:
            fatal_lines.append(line.strip())

    failures: list[str] = []
    if fatal_lines:
        failures.append("AndroidRuntime reported a fatal exception")
    if not instrumentation_enabled:
        failures.append("session-work instrumentation was not enabled")
    if not scenario_started:
        failures.append(f"scenario {scenario} did not start")
    if not any(message.startswith(f"done scenario={scenario}") for message in scenario_messages):
        failures.append(f"scenario {scenario} did not complete")
    for message in scenario_messages:
        if "threshold_violation" in message:
            failures.append(message)

    frame_summaries = [
        parse_fields(message)
        for message in scenario_messages
        if message.startswith(f"frame_summary scenario={scenario}")
    ]
    if not frame_summaries:
        failures.append("frame-gap summary is missing")
    elif (integer(frame_summaries[-1], "frames") or 0) == 0:
        failures.append("frame-gap monitor observed no frames")

    started = [record for record in work if record.get("event") == "started"]
    finished = [record for record in work if record.get("event") == "finished"]
    requested_overlays = [
        record
        for record in work
        if record.get("event") == "requested" and record.get("kind") == "map_overlay"
    ]
    if len(requested_overlays) < 64:
        failures.append(f"dense overlay burst submitted only {len(requested_overlays)} requests")

    selection_finished = [record for record in finished if record.get("kind") == "map_selection"]
    successful_selections = [
        record
        for record in selection_finished
        if record.get("action") == "land" and record.get("outcome") == "success"
    ]
    if len(successful_selections) < 2:
        failures.append(f"expected two successful map selections, observed {len(successful_selections)}")

    main_thread_work = [record for record in finished if record.get("main_thread") == "true"]
    if main_thread_work:
        failures.append(f"{len(main_thread_work)} scheduled operations executed on the main thread")

    active_counts = [value for record in started if (value := integer(record, "active_count")) is not None]
    if active_counts and max(active_counts) > max_active_work:
        failures.append(
            f"scheduled work concurrency reached {max(active_counts)}; limit is {max_active_work}"
        )

    selection_queue = [
        value
        for record in successful_selections
        if (value := integer(record, "queue_ms")) is not None
    ]
    if selection_queue and max(selection_queue) > max_selection_queue_ms:
        failures.append(
            f"map-selection queue delay reached {max(selection_queue)} ms; "
            f"limit is {max_selection_queue_ms} ms"
        )

    selection_landing = [
        value
        for record in successful_selections
        if (value := integer(record, "landing_ms")) is not None
    ]
    if selection_landing and max(selection_landing) > max_selection_landing_ms:
        failures.append(
            f"map-selection landing reached {max(selection_landing)} ms; "
            f"limit is {max_selection_landing_ms} ms"
        )

    return Analysis(tuple(work), tuple(scenario_messages), tuple(dict.fromkeys(failures)))


def summary_lines(analysis: Analysis) -> list[str]:
    finished = [record for record in analysis.work if record.get("event") == "finished"]
    lines: list[str] = []
    frame_summary = next(
        (
            parse_fields(message)
            for message in reversed(analysis.scenario_messages)
            if message.startswith("frame_summary ")
        ),
        None,
    )
    if frame_summary:
        lines.append(
            "FRAME "
            f"frames={frame_summary.get('frames', '?')} "
            f"p95_ms={frame_summary.get('p95Ms', '?')} "
            f"max_ms={frame_summary.get('maxMs', '?')} "
            f"threshold_ms={frame_summary.get('thresholdMs', '?')}"
        )
    for kind in sorted({record.get("kind", "unknown") for record in finished}):
        records = [record for record in finished if record.get("kind") == kind]
        executed = [record for record in records if integer(record, "work_ms") is not None]
        values_by_name: dict[str, list[int]] = {}
        for name in ("queue_ms", "dispatcher_wait_ms", "work_ms", "delivery_ms", "landing_ms", "total_ms"):
            values_by_name[name] = [
                value for record in executed if (value := integer(record, name)) is not None
            ]
        for output_name, source_name in (
            ("core_ms", "core_us"),
            ("resource_fetch_ms", "resource_fetch_us"),
            ("resource_fetch_wall_ms", "resource_fetch_wall_us"),
            ("resource_ingest_ms", "resource_ingest_us"),
        ):
            values_by_name[output_name] = [
                (value + 500) // 1000
                for record in executed
                if (value := integer(record, source_name)) is not None
            ]
        metrics = []
        for name, values in values_by_name.items():
            if values:
                metrics.append(
                    f"{name}=p50:{percentile(values, 0.50)},p95:{percentile(values, 0.95)},max:{max(values)}"
                )
        resource_bytes = sum(integer(record, "resource_bytes") or 0 for record in executed)
        resource_rounds = sum(integer(record, "resource_rounds") or 0 for record in executed)
        lines.append(
            f"WORK kind={kind} finished={len(records)} executed={len(executed)} "
            f"cancelled={len(records) - len(executed)} resource_rounds={resource_rounds} "
            f"resource_bytes={resource_bytes} {' '.join(metrics)}"
        )
    for record in analysis.work:
        if record.get("event") != "resource_frontier":
            continue
        lines.append(
            "FRONTIER "
            f"kind={record.get('kind', 'unknown')} "
            f"request_id={record.get('request_id', '?')} "
            f"round={record.get('round', '?')} "
            f"width={record.get('width', '?')} "
            f"sources={record.get('source_kinds', '?')} "
            f"fetch_wall_ms={(integer(record, 'fetch_wall_us') or 0) // 1000} "
            f"fetch_work_ms={(integer(record, 'fetch_work_us') or 0) // 1000} "
            f"max_concurrency={record.get('max_concurrency', '?')} "
            f"ingest_ms={(integer(record, 'ingest_us') or 0) // 1000} "
            f"resources={record.get('resource_ids', '?')}"
        )
    return lines


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--scenario", default="map_selection_freeze")
    parser.add_argument("--log", type=Path)
    parser.add_argument("--max-selection-queue-ms", type=int, default=100)
    parser.add_argument("--max-selection-landing-ms", type=int, default=50)
    parser.add_argument("--max-active-work", type=int, default=2)
    args = parser.parse_args()
    lines = args.log.read_text(encoding="utf-8").splitlines() if args.log else sys.stdin
    analysis = analyze_lines(
        lines,
        scenario=args.scenario,
        max_selection_queue_ms=args.max_selection_queue_ms,
        max_selection_landing_ms=args.max_selection_landing_ms,
        max_active_work=args.max_active_work,
    )
    for line in summary_lines(analysis):
        print(line)
    if analysis.passed:
        print(f"RESULT: scenario {args.scenario} passed")
        return 0
    for failure in analysis.failures:
        print(f"FAIL: {failure}", file=sys.stderr)
    print(f"RESULT: scenario {args.scenario} failed", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
