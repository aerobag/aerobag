#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import unittest

from analyze_perf_scenario import analyze_lines, percentile, summary_lines


def work(message: str) -> str:
    return f"08-16 12:00:00.000 I AerobagSessionWork: {message}"


def scenario(message: str) -> str:
    return f"08-16 12:00:00.000 I AerobagPerfScenario: {message}"


def passing_log() -> list[str]:
    lines = [
        work("event=instrumentation_enabled"),
        scenario("start scenario=map_selection_freeze"),
    ]
    lines.extend(
        work(f"event=requested request_id={request_id} kind=map_overlay coalesce_key=map_overlay")
        for request_id in range(1, 65)
    )
    lines.extend(
        [
            work("event=started request_id=1 kind=map_overlay active_count=1 queue_ms=0"),
            work(
                "event=finished request_id=1 kind=map_overlay action=land outcome=success "
                "queue_ms=0 dispatcher_wait_ms=1 work_ms=40 delivery_ms=1 landing_ms=2 total_ms=44 "
                "main_thread=false resource_rounds=1 resource_bytes=1024"
            ),
            work("event=started request_id=65 kind=map_selection active_count=2 queue_ms=3"),
            work(
                "event=resource_frontier request_id=65 kind=map_selection round=1 width=2 "
                "source_kinds=public_url:2 resource_ids=one+two fetch_wall_us=110000 "
                "fetch_work_us=200000 max_concurrency=2 ingest_us=10000"
            ),
            work(
                "event=finished request_id=65 kind=map_selection action=land outcome=success "
                "queue_ms=3 dispatcher_wait_ms=1 work_ms=20 delivery_ms=1 landing_ms=2 total_ms=27 "
                "main_thread=false resource_rounds=0 resource_bytes=0"
            ),
            work("event=started request_id=66 kind=map_selection active_count=1 queue_ms=4"),
            work(
                "event=finished request_id=66 kind=map_selection action=land outcome=success "
                "queue_ms=4 dispatcher_wait_ms=1 work_ms=22 delivery_ms=1 landing_ms=3 total_ms=31 "
                "main_thread=false resource_rounds=0 resource_bytes=0"
            ),
            scenario("done scenario=map_selection_freeze elapsedMs=6000"),
            scenario("frame_summary scenario=map_selection_freeze frames=300 p95Ms=17 maxMs=40 thresholdMs=250"),
        ]
    )
    return lines


class AnalyzePerfScenarioTest(unittest.TestCase):
    def test_accepts_complete_responsive_scenario_and_summarizes_work(self) -> None:
        analysis = analyze_lines(passing_log(), "map_selection_freeze")
        self.assertTrue(analysis.passed, analysis.failures)
        summaries = summary_lines(analysis)
        self.assertIn("FRAME frames=300 p95_ms=17 max_ms=40 threshold_ms=250", summaries)
        self.assertTrue(any("kind=map_selection" in line and "queue_ms=p50:3" in line for line in summaries))
        self.assertTrue(
            any(
                "FRONTIER kind=map_selection" in line
                and "width=2" in line
                and "max_concurrency=2" in line
                for line in summaries
            )
        )

    def test_rejects_main_thread_work_and_scenario_thresholds(self) -> None:
        lines = passing_log()
        frame_index = next(index for index, line in enumerate(lines) if "frame_summary" in line)
        lines.insert(
            frame_index,
            work(
                "event=finished request_id=70 kind=map_overlay action=land outcome=success "
                "main_thread=true"
            ),
        )
        lines.insert(
            frame_index + 1,
            scenario(
                "threshold_violation scenario=map_selection_freeze kind=frame_gap gapMs=300 thresholdMs=250"
            ),
        )
        analysis = analyze_lines(lines, "map_selection_freeze")
        self.assertFalse(analysis.passed)
        self.assertTrue(any("main thread" in failure for failure in analysis.failures))
        self.assertTrue(any("frame_gap" in failure for failure in analysis.failures))

    def test_ignores_work_after_scenario_frame_summary(self) -> None:
        lines = passing_log()
        lines.append(
            work(
                "event=finished request_id=70 kind=map_overlay action=land outcome=success "
                "queue_ms=9000 work_ms=9000 main_thread=true"
            )
        )
        analysis = analyze_lines(lines, "map_selection_freeze")
        self.assertTrue(analysis.passed, analysis.failures)
        self.assertNotIn("9000", "\n".join(summary_lines(analysis)))

    def test_nearest_rank_percentile_is_deterministic(self) -> None:
        self.assertEqual(percentile([1, 2, 3, 4, 5], 0.50), 3)
        self.assertEqual(percentile([1, 2, 3, 4, 5], 0.95), 5)


if __name__ == "__main__":
    unittest.main()
