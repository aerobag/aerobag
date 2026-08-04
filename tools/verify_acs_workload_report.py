#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR.parent / "product" / "preprocessor" / "scripts"))

import pipeline_health  # noqa: E402


def pipeline_facts(status: dict[str, Any], now: datetime) -> dict[str, Any]:
    return {
        "sampled_at_utc": pipeline_health.iso_utc(now),
        "inputs": {
            "current_artifacts": {"error": None, "payload": []},
            "deploy_health": {"error": None, "payload": {}},
            "live_feeds_status": {"error": None, "payload": {"products": {}}},
            "aerobag_cloud_status": {"error": None, "payload": status},
            "build_watch": {"error": None, "payload": {}},
            "faa_cycle_calendar": {"error": None, "payload": {"cycles": []}},
            "product_facts": [],
        },
    }


def verify_report(report: dict[str, Any]) -> dict[str, Any]:
    results: list[dict[str, Any]] = []
    failures: list[str] = []
    for scenario in report.get("status_scenarios", []):
        status = scenario["status"]
        now = datetime.fromtimestamp(
            status["server_time_epoch_ms"] / 1_000,
            tz=timezone.utc,
        )
        evaluation = pipeline_health.evaluate_health(
            pipeline_facts(status, now),
            [],
            now,
        )
        actual = {
            metric["id"]: metric["severity"]
            for metric in evaluation["metrics"]
        }
        expectations = scenario.get("expected_pipeline_health", [])
        checks = []
        for expectation in expectations:
            metric_id = expectation["metric_id"]
            expected = expectation["severity"]
            observed = actual.get(metric_id)
            passed = observed == expected
            checks.append(
                {
                    "metric_id": metric_id,
                    "expected": expected,
                    "observed": observed,
                    "passed": passed,
                }
            )
            if not passed:
                failures.append(
                    f"{scenario['name']}: {metric_id} expected {expected}, got {observed}"
                )
        results.append(
            {
                "scenario": scenario["name"],
                "top_line_status": evaluation["top_line_status"],
                "checks": checks,
            }
        )
    if failures:
        raise RuntimeError("pipeline-health workload verification failed:\n" + "\n".join(failures))
    return {
        "schema_version": 1,
        "workload_profile": report.get("profile"),
        "scenarios": results,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Verify real ACS workload status snapshots through pipeline-health",
    )
    parser.add_argument("report", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    report = json.loads(args.report.read_text(encoding="utf-8"))
    result = verify_report(report)
    text = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.output is not None:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text, encoding="utf-8")
        print(f"ACS pipeline-health workload verification passed: {args.output}")
    else:
        print(text, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
