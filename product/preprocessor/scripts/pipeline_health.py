#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import threading
import time
from dataclasses import dataclass
from datetime import date, datetime, timedelta, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.error import URLError
from urllib.parse import parse_qs, urlparse
from urllib.request import urlopen


SCHEMA_VERSION = 1
DEFAULT_LISTEN = "127.0.0.1:8098"
DEFAULT_POLL_SECONDS = 60
HISTORY_RECORD_LIMIT = 24 * 60
DASHBOARD_SERIES_LIMIT = HISTORY_RECORD_LIMIT
HISTORY_LOOKBACK_DAYS = 14
HISTORY_TAIL_CHUNK_BYTES = 64 * 1024
HISTORY_TAIL_MAX_BYTES = 16 * 1024 * 1024

LIVE_FEED_STALE_THRESHOLDS: dict[str, tuple[int, int]] = {
    "tafs": (60 * 60, 3 * 60 * 60),
    "metars": (7 * 60, 30 * 60),
    "obstacles": (2 * 24 * 60 * 60, 7 * 24 * 60 * 60),
    "tfrs": (3 * 60 * 60, 6 * 60 * 60),
    "nexrad": (700, 15 * 60),
}

LIVE_FEED_DISPLAY_NAMES = {
    "tafs": "TAFs",
    "metars": "METARs",
    "obstacles": "Obstacles",
    "tfrs": "TFRs",
    "nexrad": "NEXRAD",
}

SECONDS_PER_DAY = 24 * 60 * 60
CYCLE_PUBLICATION_WARNING_SECONDS = 20 * SECONDS_PER_DAY
CYCLE_PUBLICATION_CRITICAL_SECONDS = 15 * SECONDS_PER_DAY
SEVERITY_RANK = {"ok": 0, "warning": 1, "critical": 2}


def utc_now() -> datetime:
    return datetime.now(timezone.utc)


def iso_utc(value: datetime) -> str:
    return value.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")


def parse_time(value: object) -> datetime | None:
    if not isinstance(value, str) or not value:
        return None
    text = value.strip()
    if text.endswith("Z"):
        text = text[:-1] + "+00:00"
    try:
        parsed = datetime.fromisoformat(text)
    except ValueError:
        return None
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc)


def parse_date(value: object) -> date | None:
    if not isinstance(value, str) or not value:
        return None
    try:
        return date.fromisoformat(value)
    except ValueError:
        return None


def utc_midnight(value: date) -> datetime:
    return datetime(value.year, value.month, value.day, tzinfo=timezone.utc)


def human_duration(seconds: int) -> str:
    seconds = max(0, seconds)
    days, remainder = divmod(seconds, SECONDS_PER_DAY)
    hours, remainder = divmod(remainder, 60 * 60)
    minutes, _ = divmod(remainder, 60)
    if days > 0:
        if hours > 0:
            return f"{days}d {hours}h"
        return f"{days}d"
    if hours > 0:
        if minutes > 0:
            return f"{hours}h {minutes}m"
        return f"{hours}h"
    return f"{minutes}m"


def read_env_file(path: Path = Path("/etc/aerobag/env")) -> dict[str, str]:
    values: dict[str, str] = {}
    if not path.exists():
        return values
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        if len(value) >= 2 and value[0] == "'" and value[-1] == "'":
            value = value[1:-1].replace("'\"'\"'", "'")
        values[key] = value
    return values


def read_json_file(path: Path) -> tuple[Any | None, str | None]:
    try:
        return json.loads(path.read_text(encoding="utf-8")), None
    except FileNotFoundError:
        return None, f"missing file {path}"
    except Exception as exc:  # noqa: BLE001 - monitoring reports input failures.
        return None, f"{path}: {exc}"


def fetch_json_url(url: str, timeout: float = 5.0) -> tuple[Any | None, str | None]:
    try:
        with urlopen(url, timeout=timeout) as response:
            return json.loads(response.read().decode("utf-8")), None
    except (OSError, URLError, json.JSONDecodeError) as exc:
        return None, f"{url}: {exc}"


def safe_join(root: Path, relative: str) -> Path | None:
    path = Path(relative)
    if path.is_absolute() or ".." in path.parts:
        return None
    return root / path


@dataclass
class MonitorConfig:
    artifact_root: Path
    data_root: Path
    health_root: Path
    current_artifacts_path: Path
    deploy_health_path: Path
    live_feeds_status_url: str
    build_watch_url: str
    calendar_path: Path
    listen: str
    poll_seconds: float


def default_config_from_env() -> MonitorConfig:
    env = read_env_file()
    artifact_root = Path(
        os.environ.get(
            "ARTIFACT_ROOT",
            env.get("ARTIFACT_ROOT", "/mnt/aerobag-data/artifacts"),
        )
    )
    data_root = Path(os.environ.get("DATA_ROOT", env.get("DATA_ROOT", "/mnt/aerobag-data")))
    health_root = data_root / "health"
    live_listen = os.environ.get(
        "AEROBAG_LIVE_FEEDS_LISTEN",
        env.get("AEROBAG_LIVE_FEEDS_LISTEN", "127.0.0.1:8095"),
    )
    build_watch_listen = os.environ.get(
        "AEROBAG_BUILD_WATCH_LISTEN",
        env.get("AEROBAG_BUILD_WATCH_LISTEN", "127.0.0.1:8097"),
    )
    return MonitorConfig(
        artifact_root=artifact_root,
        data_root=data_root,
        health_root=health_root,
        current_artifacts_path=artifact_root / "published" / "current_artifacts.json",
        deploy_health_path=data_root / "health" / "status.json",
        live_feeds_status_url=f"http://{live_listen}/live-feeds/status.json",
        build_watch_url=f"http://{build_watch_listen}/api/state",
        calendar_path=Path("/etc/aerobag/faa-cycle-calendar.json"),
        listen=os.environ.get(
            "AEROBAG_PIPELINE_HEALTH_LISTEN",
            env.get("AEROBAG_PIPELINE_HEALTH_LISTEN", DEFAULT_LISTEN),
        ),
        poll_seconds=float(
            os.environ.get(
                "AEROBAG_PIPELINE_HEALTH_POLL_SECONDS",
                env.get("AEROBAG_PIPELINE_HEALTH_POLL_SECONDS", str(DEFAULT_POLL_SECONDS)),
            )
        ),
    )


def collect_facts(config: MonitorConfig, now: datetime) -> dict[str, Any]:
    current_artifacts, current_error = read_json_file(config.current_artifacts_path)
    product_facts = collect_product_facts(config.artifact_root, current_artifacts)
    deploy_health, deploy_health_error = read_json_file(config.deploy_health_path)
    live_status, live_error = fetch_json_url(config.live_feeds_status_url)
    build_watch, build_watch_error = fetch_json_url(config.build_watch_url)
    calendar, calendar_error = read_json_file(config.calendar_path)
    return {
        "schema_version": SCHEMA_VERSION,
        "sampled_at_utc": iso_utc(now),
        "inputs": {
            "current_artifacts": {
                "path": str(config.current_artifacts_path),
                "payload": current_artifacts,
                "error": current_error,
            },
            "product_facts": product_facts,
            "deploy_health": {
                "path": str(config.deploy_health_path),
                "payload": deploy_health,
                "error": deploy_health_error,
            },
            "live_feeds_status": {
                "url": config.live_feeds_status_url,
                "payload": live_status,
                "error": live_error,
            },
            "build_watch": {
                "url": config.build_watch_url,
                "payload": build_watch,
                "error": build_watch_error,
            },
            "faa_cycle_calendar": {
                "path": str(config.calendar_path),
                "payload": calendar,
                "error": calendar_error,
            },
        },
    }


def collect_product_facts(artifact_root: Path, current_artifacts: Any | None) -> list[dict[str, Any]]:
    publication_root = artifact_root / "published"
    manifests = current_artifacts if isinstance(current_artifacts, list) else []
    facts: list[dict[str, Any]] = []
    for index, manifest in enumerate(manifests):
        if not isinstance(manifest, dict):
            facts.append({"manifest_index": index, "error": "manifest is not an object"})
            continue
        artifact_roots = manifest.get("artifact_roots")
        packaged = artifact_roots.get("packaged") if isinstance(artifact_roots, dict) else None
        if not isinstance(packaged, str):
            facts.append({"manifest_index": index, "error": "missing artifact_roots.packaged"})
            continue
        packaged_root = safe_join(publication_root, packaged.rstrip("/"))
        if packaged_root is None:
            facts.append({"manifest_index": index, "error": "invalid artifact_roots.packaged"})
            continue
        path = packaged_root / "product-facts.json"
        payload, error = read_json_file(path)
        facts.append(
            {
                "manifest_index": index,
                "path": str(path),
                "payload": payload,
                "error": error,
            }
        )
    return facts


def evaluate_health(
    facts: dict[str, Any],
    previous_records: list[dict[str, Any]],
    now: datetime,
) -> dict[str, Any]:
    metrics: list[dict[str, Any]] = []
    alerts: list[dict[str, Any]] = []

    add_input_metrics(metrics, facts)
    add_build_watch_metrics(metrics, facts)
    add_live_feed_metrics(metrics, facts, now)
    add_product_fact_metrics(metrics, facts, previous_records)
    add_cycle_calendar_metrics(metrics, facts, now)

    for metric in metrics:
        severity = metric.get("severity", "ok")
        if severity != "ok":
            alerts.append(
                {
                    "severity": severity,
                    "metric_id": metric["id"],
                    "message": metric["message"],
                }
            )
    top_line_status = max(
        (metric.get("severity", "ok") for metric in metrics),
        key=lambda severity: SEVERITY_RANK.get(severity, 0),
        default="ok",
    )
    return {
        "schema_version": SCHEMA_VERSION,
        "generated_at_utc": iso_utc(now),
        "top_line_status": top_line_status,
        "metrics": metrics,
        "alerts": alerts,
    }


def add_metric(
    metrics: list[dict[str, Any]],
    *,
    metric_id: str,
    label: str,
    value: Any,
    unit: str | None = None,
    severity: str = "ok",
    message: str | None = None,
    warning_threshold: Any | None = None,
    critical_threshold: Any | None = None,
    details: Any | None = None,
) -> None:
    metric = {
        "id": metric_id,
        "label": label,
        "value": value,
        "severity": severity,
        "message": message or f"{label}: {value}",
    }
    if unit is not None:
        metric["unit"] = unit
    if warning_threshold is not None:
        metric["warning_threshold"] = warning_threshold
    if critical_threshold is not None:
        metric["critical_threshold"] = critical_threshold
    if details is not None:
        metric["details"] = details
    metrics.append(metric)


def threshold_severity(value: float, warning: float, critical: float) -> str:
    if value >= critical:
        return "critical"
    if value >= warning:
        return "warning"
    return "ok"


def add_input_metrics(metrics: list[dict[str, Any]], facts: dict[str, Any]) -> None:
    inputs = facts.get("inputs", {})
    for key in [
        "current_artifacts",
        "deploy_health",
        "live_feeds_status",
        "build_watch",
        "faa_cycle_calendar",
    ]:
        source = inputs.get(key, {})
        error = source.get("error") if isinstance(source, dict) else "missing input source"
        add_metric(
            metrics,
            metric_id=f"input.{key}.available",
            label=f"{key} input available",
            value=error is None,
            severity="ok" if error is None else "critical",
            message=f"{key} input ok" if error is None else str(error),
        )
    product_facts = inputs.get("product_facts")
    if isinstance(product_facts, list):
        missing = [entry for entry in product_facts if entry.get("error")]
        add_metric(
            metrics,
            metric_id="input.product_facts.available",
            label="product facts available",
            value=len(product_facts) - len(missing),
            severity="ok" if not missing else "warning",
            message=(
                "product facts available"
                if not missing
                else f"{len(missing)} current publication(s) missing product-facts.json"
            ),
        )


def add_build_watch_metrics(metrics: list[dict[str, Any]], facts: dict[str, Any]) -> None:
    build_watch = facts["inputs"]["build_watch"].get("payload")
    if not isinstance(build_watch, dict):
        return
    result = build_watch.get("result")
    result_status = result.get("status") if isinstance(result, dict) else None
    severity = "critical" if result_status == "fail" else "ok"
    add_metric(
        metrics,
        metric_id="cycle_build.latest_result",
        label="latest cycle build result",
        value=result_status,
        severity=severity,
        message=(
            "latest cycle build failed"
            if result_status == "fail"
            else f"latest cycle build result is {result_status}"
        ),
    )
    process = build_watch.get("process")
    progress = build_watch.get("progress")
    alive = process.get("alive") if isinstance(process, dict) else None
    active = progress.get("active") if isinstance(progress, dict) else None
    if result_status == "in_progress" and alive is False:
        add_metric(
            metrics,
            metric_id="cycle_build.process_alive",
            label="cycle build process alive",
            value=False,
            severity="critical",
            message="cycle build is marked in progress but its process is dead",
        )
    elif result_status == "in_progress":
        add_metric(
            metrics,
            metric_id="cycle_build.active_tasks",
            label="cycle build active tasks",
            value=active,
            severity="ok",
            message=f"cycle build active tasks: {active}",
        )


def add_live_feed_metrics(
    metrics: list[dict[str, Any]], facts: dict[str, Any], now: datetime
) -> None:
    payload = facts["inputs"]["live_feeds_status"].get("payload")
    products = payload.get("products") if isinstance(payload, dict) else None
    if not isinstance(products, dict):
        return
    for product, (warning_seconds, critical_seconds) in LIVE_FEED_STALE_THRESHOLDS.items():
        status = products.get(product)
        display = LIVE_FEED_DISPLAY_NAMES.get(product, product)
        if not isinstance(status, dict):
            add_metric(
                metrics,
                metric_id=f"live_feed.{product}.present",
                label=f"{display} status present",
                value=False,
                severity="critical",
                message=f"{display} is missing from live-feed status",
            )
            continue
        source_time = parse_time(status.get("last_source_timestamp_utc")) or parse_time(
            status.get("last_success_at_utc")
        )
        if source_time is None:
            add_metric(
                metrics,
                metric_id=f"live_feed.{product}.stale_seconds",
                label=f"{display} stale age",
                value=None,
                unit="seconds",
                severity="critical",
                warning_threshold=warning_seconds,
                critical_threshold=critical_seconds,
                message=f"{display} has no successful source timestamp",
            )
        else:
            stale_seconds = max(0, int((now - source_time).total_seconds()))
            severity = threshold_severity(stale_seconds, warning_seconds, critical_seconds)
            add_metric(
                metrics,
                metric_id=f"live_feed.{product}.stale_seconds",
                label=f"{display} stale age",
                value=stale_seconds,
                unit="seconds",
                severity=severity,
                warning_threshold=warning_seconds,
                critical_threshold=critical_seconds,
                message=f"{display} data is {stale_seconds} seconds old",
            )
        failures = int(status.get("consecutive_failure_count") or 0)
        add_metric(
            metrics,
            metric_id=f"live_feed.{product}.consecutive_failures",
            label=f"{display} consecutive failures",
            value=failures,
            severity=threshold_severity(failures, 1, 3),
            warning_threshold=1,
            critical_threshold=3,
            message=f"{display} consecutive failures: {failures}",
        )
        attempts = status.get("attempts")
        if isinstance(attempts, list) and attempts:
            attempt_count = len(attempts)
            failed_attempts = [
                attempt
                for attempt in attempts
                if isinstance(attempt, dict) and attempt.get("result") == "failure"
            ]
            failure_count = len(failed_attempts)
            failure_rate = failure_count / attempt_count
            last_failure_time = parse_time(status.get("last_failure_at_utc"))
            last_failure_age_seconds = (
                max(0, int((now - last_failure_time).total_seconds()))
                if last_failure_time is not None
                else None
            )
            severity = "critical" if attempt_count >= 3 and failure_rate >= 0.5 else (
                "warning" if failure_count > 0 else "ok"
            )
            detail_failures = [
                {
                    "attempted_at_utc": attempt.get("attempted_at_utc"),
                    "phase": attempt.get("phase"),
                    "error": attempt.get("error"),
                }
                for attempt in failed_attempts[-10:]
                if isinstance(attempt, dict)
            ]
            if last_failure_age_seconds is None:
                failure_message = f"{display} recent failures: {failure_count}/{attempt_count} attempts"
            else:
                failure_message = (
                    f"{display} recent failures: {failure_count}/{attempt_count} attempts; "
                    f"last failure {last_failure_age_seconds} seconds ago"
                )
            add_metric(
                metrics,
                metric_id=f"live_feed.{product}.recent_failure_rate",
                label=f"{display} recent failure rate",
                value=round(failure_rate, 6),
                unit="ratio",
                severity=severity,
                warning_threshold=0.0,
                critical_threshold=0.5,
                message=failure_message,
                details={
                    "attempt_count": attempt_count,
                    "failure_count": failure_count,
                    "last_failure_at_utc": status.get("last_failure_at_utc"),
                    "last_failure_phase": status.get("last_failure_phase"),
                    "last_error": status.get("last_error"),
                    "last_failure_age_seconds": last_failure_age_seconds,
                    "failures": detail_failures,
                },
            )
        if product == "nexrad":
            quality = status.get("quality")
            if isinstance(quality, dict):
                poor_count = int(quality.get("poor_color_match_count") or 0)
                add_metric(
                    metrics,
                    metric_id="live_feed.nexrad.poor_color_match_count",
                    label="NEXRAD poor color matches",
                    value=poor_count,
                    severity="warning" if poor_count > 0 else "ok",
                    warning_threshold=1,
                    message=f"NEXRAD poor color matches: {poor_count}",
                )
                if "palette_error_max" in quality:
                    add_metric(
                        metrics,
                        metric_id="live_feed.nexrad.palette_error_max",
                        label="NEXRAD max palette error",
                        value=quality.get("palette_error_max"),
                        severity="ok",
                        message=f"NEXRAD max palette error: {quality.get('palette_error_max')}",
                    )


def add_product_fact_metrics(
    metrics: list[dict[str, Any]],
    facts: dict[str, Any],
    previous_records: list[dict[str, Any]],
) -> None:
    summary = product_count_summary(facts)
    counts = summary["counts"]
    previous_counts = latest_distinct_product_counts(previous_records, facts)
    for count_name, label in [
        ("error_count", "Cycle product errors per cycle"),
        ("warning_count", "Cycle product warnings per cycle"),
    ]:
        value = counts[count_name]
        previous = previous_counts.get(count_name) if previous_counts is not None else None
        increased = previous is not None and value > previous
        cycle_summary = format_cycle_count_summary(summary["cycles"], count_name)
        message = f"{label}: max {value}"
        if cycle_summary:
            message = f"{message} ({cycle_summary})"
        if previous is not None:
            message = f"{message}; previous distinct publication: {previous}"
        add_metric(
            metrics,
            metric_id=f"cycle_product.{count_name}",
            label=label,
            value=value,
            unit="count",
            severity="warning" if increased else "ok",
            warning_threshold=(previous + 1) if previous is not None else None,
            message=message,
            details={"cycle_counts": summary["cycles"]},
        )


def iter_current_product_facts(facts: dict[str, Any]) -> list[dict[str, Any]]:
    products: list[dict[str, Any]] = []
    for entry in facts.get("inputs", {}).get("product_facts", []):
        payload = entry.get("payload") if isinstance(entry, dict) else None
        payload_products = payload.get("products") if isinstance(payload, dict) else None
        if isinstance(payload_products, list):
            products.extend(product for product in payload_products if isinstance(product, dict))
    return products


def product_count_summary(facts: dict[str, Any]) -> dict[str, Any]:
    cycle_counts: dict[str, dict[str, int]] = {}
    for product in iter_current_product_facts(facts):
        cycle = str(product.get("cycle") or "uncycled")
        counts = cycle_counts.setdefault(cycle, {"error_count": 0, "warning_count": 0})
        counts["error_count"] += int(product.get("error_count") or 0)
        counts["warning_count"] += int(product.get("warning_count") or 0)
    return {
        "counts": max_cycle_counts(cycle_counts),
        "cycles": cycle_counts,
    }


def aggregate_product_counts(facts: dict[str, Any]) -> dict[str, int]:
    return product_count_summary(facts)["counts"]


def max_cycle_counts(cycle_counts: dict[str, dict[str, int]]) -> dict[str, int]:
    return {
        "error_count": max(
            (counts.get("error_count", 0) for counts in cycle_counts.values()),
            default=0,
        ),
        "warning_count": max(
            (counts.get("warning_count", 0) for counts in cycle_counts.values()),
            default=0,
        ),
    }


def format_cycle_count_summary(
    cycle_counts: dict[str, dict[str, int]], count_name: str
) -> str:
    parts = [
        f"{cycle}: {counts.get(count_name, 0)}"
        for cycle, counts in sorted(cycle_counts.items())
        if counts.get(count_name, 0)
    ]
    return ", ".join(parts)


def product_facts_publication_key(facts: dict[str, Any]) -> tuple[str, ...]:
    keys: list[str] = []
    for entry in facts.get("inputs", {}).get("product_facts", []):
        if not isinstance(entry, dict):
            continue
        path = entry.get("path")
        if isinstance(path, str):
            keys.append(path)
            continue
        payload = entry.get("payload")
        generated_at = payload.get("generated_at_utc") if isinstance(payload, dict) else None
        if isinstance(generated_at, str):
            keys.append(generated_at)
    return tuple(sorted(keys))


def record_product_facts_key(record: dict[str, Any]) -> tuple[str, ...]:
    key = record.get("product_facts_key")
    if isinstance(key, list) and all(isinstance(item, str) for item in key):
        return tuple(sorted(key))
    facts = record.get("facts")
    if isinstance(facts, dict):
        return product_facts_publication_key(facts)
    return ()


def record_product_counts(record: dict[str, Any]) -> dict[str, int] | None:
    counts = record.get("product_counts")
    if isinstance(counts, dict):
        return {
            "error_count": int(counts.get("error_count") or 0),
            "warning_count": int(counts.get("warning_count") or 0),
        }
    facts = record.get("facts")
    if isinstance(facts, dict):
        return aggregate_product_counts(facts)
    return None


def latest_distinct_product_counts(
    previous_records: list[dict[str, Any]], current_facts: dict[str, Any]
) -> dict[str, int] | None:
    current_key = product_facts_publication_key(current_facts)
    for record in reversed(previous_records):
        previous_key = record_product_facts_key(record)
        if current_key and previous_key == current_key:
            continue
        counts = record_product_counts(record)
        if counts is None:
            continue
        if counts["error_count"] or counts["warning_count"] or previous_key:
            return counts
    return None


def add_cycle_calendar_metrics(
    metrics: list[dict[str, Any]], facts: dict[str, Any], now: datetime
) -> None:
    calendar = facts["inputs"]["faa_cycle_calendar"].get("payload")
    cycles = calendar.get("cycles") if isinstance(calendar, dict) else None
    if not isinstance(cycles, list):
        return
    published_cycles = {
        str(product.get("cycle"))
        for product in iter_current_product_facts(facts)
        if product.get("cycle")
    }
    for entry in cycles:
        if not isinstance(entry, dict):
            continue
        cycle = entry.get("cycle")
        effective = parse_date(entry.get("effective_date"))
        if not isinstance(cycle, str) or effective is None:
            continue
        effective_at = utc_midnight(effective)
        seconds_until_effective = int((effective_at - now).total_seconds())
        if seconds_until_effective > CYCLE_PUBLICATION_WARNING_SECONDS:
            continue
        if effective_at < now - timedelta(days=35):
            continue
        metric_id = f"cycle_calendar.{cycle}.seconds_until_effective"
        if cycle in published_cycles:
            add_metric(
                metrics,
                metric_id=metric_id,
                label=f"FAA cycle {cycle} publication countdown",
                value=0,
                unit="seconds",
                severity="ok",
                message=f"FAA cycle {cycle} is published; no looming deadline",
            )
            continue
        value = max(0, seconds_until_effective)
        severity = (
            "critical"
            if value <= CYCLE_PUBLICATION_CRITICAL_SECONDS
            else "warning"
        )
        add_metric(
            metrics,
            metric_id=metric_id,
            label=f"FAA cycle {cycle} publication countdown",
            value=value,
            unit="seconds",
            severity=severity,
            warning_threshold=CYCLE_PUBLICATION_WARNING_SECONDS,
            critical_threshold=CYCLE_PUBLICATION_CRITICAL_SECONDS,
            message=(
                f"FAA cycle {cycle} is not published; "
                f"effective in {human_duration(value)}"
            ),
        )


@dataclass
class HistoryRead:
    records: list[dict[str, Any]]
    files: list[str]
    truncated: bool = False


def history_path_for_date(health_root: Path, day: date) -> Path:
    return health_root / f"pipeline_health-{day.isoformat()}.jsonl"


def history_paths_newest_first(health_root: Path, now: datetime) -> list[Path]:
    paths: list[Path] = []
    for day_offset in range(HISTORY_LOOKBACK_DAYS):
        path = history_path_for_date(health_root, (now - timedelta(days=day_offset)).date())
        if path.exists():
            paths.append(path)
    return paths


def parse_history_line(line: bytes) -> dict[str, Any] | None:
    line = line.strip()
    if not line:
        return None
    try:
        value = json.loads(line.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError):
        return None
    return value if isinstance(value, dict) else None


def read_history_file_tail(
    path: Path,
    limit: int,
    *,
    max_bytes: int = HISTORY_TAIL_MAX_BYTES,
) -> HistoryRead:
    if limit <= 0 or not path.exists():
        return HistoryRead(records=[], files=[])
    size = path.stat().st_size
    position = size
    bytes_read = 0
    pending = b""
    lines: list[bytes] = []
    while position > 0 and len(lines) < limit and bytes_read < max_bytes:
        chunk_size = min(HISTORY_TAIL_CHUNK_BYTES, position, max_bytes - bytes_read)
        if chunk_size <= 0:
            break
        position -= chunk_size
        bytes_read += chunk_size
        with path.open("rb") as stream:
            stream.seek(position)
            chunk = stream.read(chunk_size)
        parts = (chunk + pending).splitlines()
        if position > 0 and parts:
            pending = parts[0]
            parts = parts[1:]
        else:
            pending = b""
        lines = parts + lines
    selected = lines[-limit:]
    records = [
        record
        for line in selected
        if (record := parse_history_line(line)) is not None
    ]
    truncated = position > 0 and len(records) < limit
    return HistoryRead(records=records, files=[str(path)], truncated=truncated)


def read_history(
    health_root: Path,
    limit: int = HISTORY_RECORD_LIMIT,
    *,
    now: datetime | None = None,
) -> HistoryRead:
    limit = max(0, min(limit, HISTORY_RECORD_LIMIT))
    if limit == 0:
        return HistoryRead(records=[], files=[])
    now = now or utc_now()
    chunks: list[HistoryRead] = []
    remaining = limit
    for path in history_paths_newest_first(health_root, now):
        chunk = read_history_file_tail(path, remaining)
        if chunk.records:
            chunks.append(chunk)
            remaining -= len(chunk.records)
        if remaining <= 0:
            break
    records: list[dict[str, Any]] = []
    files: list[str] = []
    truncated = False
    for chunk in reversed(chunks):
        records.extend(chunk.records)
        files.extend(chunk.files)
        truncated = truncated or chunk.truncated
    return HistoryRead(records=records[-limit:], files=files, truncated=truncated)


def append_history(path: Path, record: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as stream:
        stream.write(json.dumps(record, sort_keys=True) + "\n")


def compact_history_record(
    facts: dict[str, Any],
    evaluation: dict[str, Any],
) -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "sampled_at_utc": facts["sampled_at_utc"],
        "evaluation": evaluation,
        "product_facts_key": list(product_facts_publication_key(facts)),
        "product_counts": aggregate_product_counts(facts),
    }


def write_current(path: Path, record: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".json.tmp")
    tmp.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    tmp.replace(path)


def run_sample(config: MonitorConfig) -> dict[str, Any]:
    now = utc_now()
    history_path = history_path_for_date(config.health_root, now.date())
    previous = read_history(config.health_root, now=now).records
    facts = collect_facts(config, now)
    evaluation = evaluate_health(facts, previous, now)
    record = compact_history_record(facts, evaluation)
    append_history(history_path, record)
    write_current(config.health_root / "pipeline-health-current.json", record)
    return record


def sample_age_seconds(record: dict[str, Any], now: datetime) -> int | None:
    sampled_at = parse_time(record.get("sampled_at_utc"))
    if sampled_at is None:
        return None
    return max(0, int((now - sampled_at).total_seconds()))


def current_record_for_response(config: MonitorConfig) -> dict[str, Any]:
    current_path = config.health_root / "pipeline-health-current.json"
    if not current_path.exists():
        record = run_sample(config)
    else:
        payload, error = read_json_file(current_path)
        record = payload if isinstance(payload, dict) and error is None else run_sample(config)
    served_at = utc_now()
    response = dict(record)
    response["served_at_utc"] = iso_utc(served_at)
    response["sample_age_seconds"] = sample_age_seconds(record, served_at)
    return response


def compact_metric_series(records: list[dict[str, Any]]) -> dict[str, Any]:
    samples: list[dict[str, Any]] = []
    for record in records:
        evaluation = record.get("evaluation")
        if not isinstance(evaluation, dict):
            continue
        metrics = evaluation.get("metrics")
        if not isinstance(metrics, list):
            continue
        metric_values: dict[str, dict[str, Any]] = {}
        for metric in metrics:
            if not isinstance(metric, dict) or not isinstance(metric.get("id"), str):
                continue
            metric_values[metric["id"]] = metric.get("value")
        samples.append(
            {
                "sampled_at_utc": record.get("sampled_at_utc"),
                "metrics": metric_values,
            }
        )
    return {
        "schema_version": SCHEMA_VERSION,
        "generated_at_utc": iso_utc(utc_now()),
        "samples": samples,
    }


def parse_record_limit(value: str | None, default: int) -> int:
    if value is None:
        return default
    try:
        parsed = int(value)
    except ValueError:
        return default
    return max(0, min(parsed, HISTORY_RECORD_LIMIT))


def parse_listen(value: str) -> tuple[str, int]:
    if ":" not in value:
        return value or "127.0.0.1", 8098
    host, port = value.rsplit(":", 1)
    return host or "127.0.0.1", int(port)


def serve(config: MonitorConfig) -> None:
    stop = threading.Event()

    def sampler() -> None:
        while not stop.is_set():
            try:
                run_sample(config)
            except Exception as exc:  # noqa: BLE001 - keep monitor alive.
                print(f"pipeline health sample failed: {exc}", flush=True)
            stop.wait(config.poll_seconds)

    thread = threading.Thread(target=sampler, daemon=True)
    thread.start()
    host, port = parse_listen(config.listen)

    class Handler(BaseHTTPRequestHandler):
        server_version = "AerobagPipelineHealth/1"

        def log_message(self, format: str, *args: object) -> None:
            return

        def do_HEAD(self) -> None:
            self._handle(send_body=False)

        def do_GET(self) -> None:
            self._handle(send_body=True)

        def _handle(self, send_body: bool) -> None:
            path = urlparse(self.path).path
            if path in {"/", "/pipeline-health/", "/pipeline-health/status.html"}:
                self._send(200, dashboard_html(), "text/html; charset=utf-8", send_body)
                return
            if path == "/pipeline-health/current.json":
                record = current_record_for_response(config)
                self._send(
                    200,
                    json.dumps(record, indent=2, sort_keys=True) + "\n",
                    "application/json",
                    send_body,
                )
                return
            if path == "/pipeline-health/history.json":
                query = parse_qs(urlparse(self.path).query)
                limit = parse_record_limit(query.get("limit", ["200"])[0], 200)
                history = read_history(config.health_root, limit)
                self._send(
                    200,
                    json.dumps(
                        {
                            "schema_version": SCHEMA_VERSION,
                            "generated_at_utc": iso_utc(utc_now()),
                            "record_limit": limit,
                            "records_returned": len(history.records),
                            "history_files": history.files,
                            "truncated": history.truncated,
                            "records": history.records,
                        },
                        indent=2,
                        sort_keys=True,
                    )
                    + "\n",
                    "application/json",
                    send_body,
                )
                return
            if path == "/pipeline-health/series.json":
                query = parse_qs(urlparse(self.path).query)
                limit = parse_record_limit(
                    query.get("limit", [str(DASHBOARD_SERIES_LIMIT)])[0],
                    DASHBOARD_SERIES_LIMIT,
                )
                history = read_history(config.health_root, limit)
                response = compact_metric_series(history.records)
                response["record_limit"] = limit
                response["records_returned"] = len(history.records)
                response["history_files"] = history.files
                response["truncated"] = history.truncated
                self._send(
                    200,
                    json.dumps(response, sort_keys=True, separators=(",", ":")) + "\n",
                    "application/json",
                    send_body,
                )
                return
            if path == "/pipeline-health/health.json":
                self._send(200, '{"ok":true}\n', "application/json", send_body)
                return
            self._send(404, "not found\n", "text/plain; charset=utf-8", send_body)

        def _send_file(self, path: Path, content_type: str, send_body: bool) -> None:
            try:
                body = path.read_text(encoding="utf-8")
            except FileNotFoundError:
                self._send(404, "not found\n", "text/plain; charset=utf-8", send_body)
                return
            self._send(200, body, content_type, send_body)

        def _send(self, status: int, body: str, content_type: str, send_body: bool) -> None:
            payload = body.encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(payload)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            if send_body:
                try:
                    self.wfile.write(payload)
                except (BrokenPipeError, ConnectionResetError):
                    return

    class PipelineHealthServer(ThreadingHTTPServer):
        allow_reuse_address = True
        daemon_threads = True

    print(f"pipeline health serving on http://{host}:{port}/pipeline-health/", flush=True)
    PipelineHealthServer((host, port), Handler).serve_forever()


def dashboard_html() -> str:
    return """<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Aerobag Pipeline Health</title>
  <script src="https://cdn.plot.ly/plotly-2.35.2.min.js"></script>
  <style>
    :root { color-scheme: dark; --bg:#101312; --panel:#171b19; --line:#303833; --text:#edf3ee; --muted:#a9b5ad; --ok:#50d890; --warn:#f0c85a; --crit:#ff6b6b; }
    * { box-sizing: border-box; }
    body { margin:0; min-height:100vh; background:var(--bg); color:var(--text); font:14px/1.45 ui-sans-serif, system-ui, sans-serif; letter-spacing:0; user-select:text; }
    main { padding:20px; max-width:1500px; margin:0 auto; }
    header { display:flex; justify-content:space-between; gap:16px; align-items:flex-start; margin-bottom:16px; flex-wrap:wrap; }
    h1 { margin:0; font-size:24px; }
    h2 { margin:0 0 10px; font-size:16px; }
    .muted { color:var(--muted); }
    .meta { display:flex; gap:10px; flex-wrap:wrap; margin-top:6px; }
    .pill { display:inline-flex; align-items:center; border:1px solid var(--line); border-radius:999px; padding:4px 10px; font-weight:700; text-transform:uppercase; }
    .tag { display:inline-flex; align-items:center; border:1px solid var(--line); border-radius:999px; padding:4px 10px; color:var(--muted); }
    .ok { color:var(--ok); border-color:color-mix(in srgb, var(--ok) 55%, var(--line)); }
    .warning { color:var(--warn); border-color:color-mix(in srgb, var(--warn) 55%, var(--line)); }
    .critical { color:var(--crit); border-color:color-mix(in srgb, var(--crit) 55%, var(--line)); }
    section { background:var(--panel); border:1px solid var(--line); border-radius:8px; padding:14px; }
    table { width:100%; border-collapse:collapse; }
    th, td { border-bottom:1px solid var(--line); padding:6px 8px; text-align:left; vertical-align:top; }
    th { color:var(--muted); font-weight:600; }
    .alerts { margin-bottom:12px; }
    .metric-list { display:flex; flex-direction:column; gap:10px; }
    .metric-row { display:grid; grid-template-columns:minmax(280px, 360px) minmax(0, 1fr); gap:12px; align-items:stretch; background:var(--panel); border:1px solid var(--line); border-radius:8px; padding:12px; }
    .metric-title { display:flex; align-items:center; justify-content:space-between; gap:10px; margin-bottom:8px; }
    .metric-title h3 { margin:0; font-size:15px; }
    .metric-id { color:var(--muted); overflow-wrap:anywhere; font-size:12px; margin-bottom:10px; }
    .metric-value { font-size:24px; font-weight:700; margin-bottom:6px; }
    .metric-message { color:var(--muted); overflow-wrap:anywhere; }
    .metric-details { grid-column:1 / -1; margin-top:2px; color:var(--muted); user-select:text; }
    .metric-details summary { cursor:pointer; color:var(--text); }
    .metric-details table { margin-top:8px; font-size:12px; }
    .metric-details td:last-child { overflow-wrap:anywhere; }
    .plot { min-height:150px; height:150px; user-select:none; }
    .plot * { user-select:none; }
    @media (max-width: 820px) {
      main { padding:12px; }
      .metric-row { grid-template-columns:1fr; }
      .plot { height:190px; }
    }
  </style>
</head>
<body>
<main>
  <header>
    <div>
      <h1>Pipeline Health</h1>
      <div class="meta">
        <span id="sampleAge" class="tag">sample age ...</span>
        <span id="sampledAt" class="tag">sampled ...</span>
        <span id="servedAt" class="tag">served ...</span>
      </div>
    </div>
    <div id="topline" class="pill">...</div>
  </header>
  <section class="alerts">
    <h2>Alerts</h2>
    <div id="alerts"></div>
  </section>
  <div id="metricRows" class="metric-list"></div>
</main>
<script>
const cls = (severity) => severity === "critical" ? "critical" : severity === "warning" ? "warning" : "ok";
const severityRank = { ok: 0, warning: 1, critical: 2 };
const esc = (value) => String(value ?? "").replace(/[&<>"']/g, (ch) => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[ch]));
async function loadJson(path) {
  const response = await fetch(path, { cache: "no-store" });
  if (!response.ok) throw new Error(`${path}: ${response.status}`);
  return response.json();
}
function graphValue(value) {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "boolean") return value ? 1 : 0;
  return null;
}
function seriesValue(metric) {
  if (metric && typeof metric === "object" && Object.hasOwn(metric, "value")) return metric.value;
  return metric;
}
function formatValue(metric) {
  const value = metric?.value;
  if (value === null || value === undefined) return "missing";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "number") {
    if (metric?.unit === "seconds") return formatAge(Math.floor(value));
    if (Math.abs(value) >= 1000) return value.toLocaleString();
    return String(value);
  }
  return String(value);
}
function formatAge(seconds) {
  if (typeof seconds !== "number") return "unknown";
  if (seconds >= 86400) return `${(seconds / 86400).toFixed(1)} days`;
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return `${hours}h ${minutes}m`;
}
function renderMetricDetails(metric) {
  const details = metric?.details;
  const failures = Array.isArray(details?.failures) ? details.failures : [];
  if (!failures.length && !details?.last_error) return "";
  const rows = failures.map((failure) => `<tr>
    <td>${esc(failure.attempted_at_utc || "")}</td>
    <td>${esc(failure.phase || "")}</td>
    <td>${esc(failure.error || "")}</td>
  </tr>`).join("");
  const last = details?.last_error
    ? `<div class="metric-message">Last failure: ${esc(details.last_failure_at_utc || "")} ${details.last_failure_age_seconds === null || details.last_failure_age_seconds === undefined ? "" : `(${esc(formatAge(details.last_failure_age_seconds))} ago)`}</div>
       <div class="metric-message">${esc(details.last_error)}</div>`
    : "";
  const table = rows
    ? `<table><thead><tr><th>Attempted</th><th>Phase</th><th>Error</th></tr></thead><tbody>${rows}</tbody></table>`
    : "";
  return `<details class="metric-details"><summary>Failure details</summary>${last}${table}</details>`;
}
function graphableMetrics(metrics, series) {
  const idsWithSeries = new Set();
  for (const sample of series.samples || []) {
    for (const [id, metric] of Object.entries(sample.metrics || {})) {
      if (graphValue(seriesValue(metric)) !== null) idsWithSeries.add(id);
    }
  }
  return (metrics || [])
    .filter((metric) => graphValue(metric.value) !== null || idsWithSeries.has(metric.id))
    .sort((a, b) => {
      const severity = (severityRank[b.severity || "ok"] || 0) - (severityRank[a.severity || "ok"] || 0);
      return severity || String(a.id).localeCompare(String(b.id));
    });
}
function renderCurrent(record) {
  const evaln = record.evaluation;
  document.getElementById("sampleAge").textContent = `data age ${formatAge(record.sample_age_seconds)}`;
  document.getElementById("sampledAt").textContent = `sampled ${record.sampled_at_utc || evaln.generated_at_utc}`;
  document.getElementById("servedAt").textContent = `served ${record.served_at_utc || "unknown"}`;
  const top = document.getElementById("topline");
  top.textContent = evaln.top_line_status;
  top.className = `pill ${cls(evaln.top_line_status)}`;
  const alerts = evaln.alerts || [];
  document.getElementById("alerts").innerHTML = alerts.length
    ? `<table><thead><tr><th>Severity</th><th>Metric</th><th>Message</th></tr></thead><tbody>${alerts.map((a) => `<tr><td class="${cls(a.severity)}">${esc(a.severity)}</td><td>${esc(a.metric_id)}</td><td>${esc(a.message)}</td></tr>`).join("")}</tbody></table>`
    : `<div class="muted">No alerts.</div>`;
}
function renderMetricRows(current, series) {
  const metrics = graphableMetrics(current.evaluation?.metrics || [], series);
  const container = document.getElementById("metricRows");
  container.innerHTML = metrics.length
    ? metrics.map((metric, index) => `<section class="metric-row">
        <div>
          <div class="metric-title"><h3>${esc(metric.label || metric.id)}</h3><span class="pill ${cls(metric.severity)}">${esc(metric.severity || "ok")}</span></div>
          <div class="metric-id">${esc(metric.id)}</div>
          <div class="metric-value">${esc(formatValue(metric))}${metric.unit ? ` <span class="muted">${esc(metric.unit)}</span>` : ""}</div>
          <div class="metric-message">${esc(metric.message || "")}</div>
        </div>
        <div id="metricPlot${index}" class="plot"></div>
        ${renderMetricDetails(metric)}
      </section>`).join("")
    : `<section><div class="muted">No graphable metrics yet.</div></section>`;
  if (!window.Plotly) return;
  metrics.forEach((metric, index) => {
    const x = [], y = [];
    for (const sample of series.samples || []) {
      const value = graphValue(seriesValue(sample.metrics?.[metric.id]));
      if (value !== null && sample.sampled_at_utc) {
        x.push(sample.sampled_at_utc);
        y.push(value);
      }
    }
    const traces = [{ type:"scatter", mode:"lines", name:metric.id, x, y, line:{color:"#50d890", width:2} }];
    if (typeof metric.warning_threshold === "number" && x.length) {
      traces.push({ type:"scatter", mode:"lines", name:"warning", x:[x[0], x[x.length - 1]], y:[metric.warning_threshold, metric.warning_threshold], line:{color:"#f0c85a", width:1, dash:"dot"} });
    }
    if (typeof metric.critical_threshold === "number" && x.length) {
      traces.push({ type:"scatter", mode:"lines", name:"critical", x:[x[0], x[x.length - 1]], y:[metric.critical_threshold, metric.critical_threshold], line:{color:"#ff6b6b", width:1, dash:"dot"} });
    }
    Plotly.react(`metricPlot${index}`, traces, {
      paper_bgcolor:"#171b19",
      plot_bgcolor:"#171b19",
      font:{color:"#edf3ee", size:11},
      margin:{l:48,r:12,t:8,b:32},
      xaxis:{type:"date", gridcolor:"#303833"},
      yaxis:{title:metric.unit || "", gridcolor:"#303833", rangemode:"tozero"},
      showlegend:traces.length > 1,
      legend:{orientation:"h", x:0, y:1.18}
    }, { responsive:true, displaylogo:false });
  });
}
async function refresh() {
  const [current, series] = await Promise.all([
    loadJson("/pipeline-health/current.json"),
    loadJson("/pipeline-health/series.json?limit=1440"),
  ]);
  renderCurrent(current);
  renderMetricRows(current, series);
}
refresh().catch((error) => {
  document.getElementById("sampleAge").textContent = String(error);
});
setInterval(() => refresh().catch((error) => {
  document.getElementById("sampleAge").textContent = String(error);
}), 30000);
</script>
</body>
</html>
"""


def parse_args() -> argparse.Namespace:
    defaults = default_config_from_env()
    parser = argparse.ArgumentParser(description="Evaluate Aerobag preprocessing pipeline health.")
    parser.add_argument("--artifact-root", type=Path, default=defaults.artifact_root)
    parser.add_argument("--data-root", type=Path, default=defaults.data_root)
    parser.add_argument("--health-root", type=Path, default=defaults.health_root)
    parser.add_argument("--current-artifacts", type=Path, default=defaults.current_artifacts_path)
    parser.add_argument("--deploy-health", type=Path, default=defaults.deploy_health_path)
    parser.add_argument("--live-feeds-status-url", default=defaults.live_feeds_status_url)
    parser.add_argument("--build-watch-url", default=defaults.build_watch_url)
    parser.add_argument("--calendar", type=Path, default=defaults.calendar_path)
    parser.add_argument("--listen", default=defaults.listen)
    parser.add_argument("--poll-seconds", type=float, default=defaults.poll_seconds)
    parser.add_argument("--once", action="store_true")
    return parser.parse_args()


def config_from_args(args: argparse.Namespace) -> MonitorConfig:
    return MonitorConfig(
        artifact_root=args.artifact_root,
        data_root=args.data_root,
        health_root=args.health_root,
        current_artifacts_path=args.current_artifacts,
        deploy_health_path=args.deploy_health,
        live_feeds_status_url=args.live_feeds_status_url,
        build_watch_url=args.build_watch_url,
        calendar_path=args.calendar,
        listen=args.listen,
        poll_seconds=args.poll_seconds,
    )


def main() -> int:
    args = parse_args()
    config = config_from_args(args)
    if args.once:
        print(json.dumps(run_sample(config), indent=2, sort_keys=True))
        return 0
    serve(config)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
