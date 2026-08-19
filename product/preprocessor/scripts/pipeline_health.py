#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import argparse
import base64
import hashlib
import hmac
import json
import math
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
from urllib.request import Request, urlopen


SCHEMA_VERSION = 1
HISTORY_SCHEMA_VERSION = 2
DEFAULT_LISTEN = "127.0.0.1:8098"
DEFAULT_POLL_SECONDS = 60
HISTORY_RECORD_LIMIT = 2 * 24 * 60 + 10
HISTORY_LOOKBACK_DAYS = 14
HISTORY_RETENTION_DAYS = HISTORY_LOOKBACK_DAYS
HISTORY_TAIL_CHUNK_BYTES = 64 * 1024
HISTORY_TAIL_MAX_BYTES = 32 * 1024 * 1024
DASHBOARD_WINDOW_SECONDS = 24 * 60 * 60
DASHBOARD_BUCKET_SECONDS = 5 * 60
DASHBOARD_BUCKET_LIMIT = DASHBOARD_WINDOW_SECONDS // DASHBOARD_BUCKET_SECONDS
LIVE_FEED_FAILURE_WINDOW_SECONDS = 2 * 60 * 60
EXPECTED_NOTAM_PROCEDURE_WITHOUT_UI_ANCHOR = 1
ACS_OPERATOR_STATUS_KDF_LABEL = b"aerobag-cloud-operator-status-v1"

_history_maintenance_dates: dict[Path, date] = {}
_history_maintenance_lock = threading.Lock()

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


def fetch_json_url(
    url: str,
    timeout: float = 5.0,
    headers: dict[str, str] | None = None,
) -> tuple[Any | None, str | None]:
    try:
        with urlopen(Request(url, headers=headers or {}), timeout=timeout) as response:
            return json.loads(response.read().decode("utf-8")), None
    except (OSError, URLError, json.JSONDecodeError) as exc:
        return None, f"{url}: {exc}"


def cloud_status_authorization(secret_path: Path) -> tuple[str, str | None]:
    try:
        secret = secret_path.read_bytes()
    except OSError as exc:
        return "", f"ACS operator credential {secret_path}: {exc}"
    if len(secret) != 32:
        return "", f"ACS operator credential {secret_path} is not 32 bytes"
    digest = hmac.new(secret, ACS_OPERATOR_STATUS_KDF_LABEL, hashlib.sha256).digest()
    token = base64.urlsafe_b64encode(digest).decode("ascii").rstrip("=")
    return f"Bearer {token}", None


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
    cloud_status_url: str
    cloud_status_secret_path: Path
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
    cloud_listen = os.environ.get(
        "AEROBAG_CLOUD_SERVER_LISTEN",
        env.get("AEROBAG_CLOUD_SERVER_LISTEN", "127.0.0.1:8096"),
    )
    cloud_secret = Path(
        os.environ.get(
            "AEROBAG_CLOUD_SERVER_SECRET",
            env.get(
                "AEROBAG_CLOUD_SERVER_SECRET",
                "/etc/aerobag/secrets/aerobag-cloud-server.bin",
            ),
        )
    )
    return MonitorConfig(
        artifact_root=artifact_root,
        data_root=data_root,
        health_root=health_root,
        current_artifacts_path=artifact_root / "published" / "current_artifacts.json",
        deploy_health_path=data_root / "health" / "status.json",
        live_feeds_status_url=f"http://{live_listen}/live-feeds/status.json",
        cloud_status_url=f"http://{cloud_listen}/cloud/v1/status",
        cloud_status_secret_path=cloud_secret,
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
    cloud_authorization, cloud_auth_error = cloud_status_authorization(
        config.cloud_status_secret_path
    )
    if cloud_auth_error is None:
        cloud_status, cloud_error = fetch_json_url(
            config.cloud_status_url,
            headers={"Authorization": cloud_authorization},
        )
    else:
        cloud_status, cloud_error = None, cloud_auth_error
    if isinstance(cloud_status, dict):
        # Pipeline-health is broadly visible; opaque account/network rankings are operator-only.
        cloud_status = dict(cloud_status)
        cloud_status.pop("top_contributors", None)
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
            "aerobag_cloud_status": {
                "url": config.cloud_status_url,
                "payload": cloud_status,
                "error": cloud_error,
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
    add_aerobag_cloud_metrics(metrics, facts)
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
    keys = [
        "current_artifacts",
        "deploy_health",
        "live_feeds_status",
        "build_watch",
        "faa_cycle_calendar",
    ]
    if "aerobag_cloud_status" in inputs:
        keys.append("aerobag_cloud_status")
    for key in keys:
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
    severity = {
        "pass": "ok",
        "in_progress": "ok",
        "fail": "critical",
    }.get(result_status, "warning")
    add_metric(
        metrics,
        metric_id="cycle_build.latest_result",
        label="latest cycle build result",
        value=result_status,
        severity=severity,
        message=(
            "latest cycle build failed"
            if result_status == "fail"
            else (
                f"latest cycle build result is {result_status}"
                if result_status in {"pass", "in_progress"}
                else f"latest cycle build has unknown result status {result_status!r}"
            )
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


def add_aerobag_cloud_metrics(metrics: list[dict[str, Any]], facts: dict[str, Any]) -> None:
    source = facts.get("inputs", {}).get("aerobag_cloud_status")
    if not isinstance(source, dict):
        return
    payload = source.get("payload")
    if not isinstance(payload, dict):
        return

    mode = payload.get("mode")
    mode_severity = "ok"
    if mode == "read_only":
        mode_severity = "warning"
    elif mode != "normal":
        mode_severity = "critical"
    add_metric(
        metrics,
        metric_id="aerobag_cloud.mode",
        label="Aerobag Cloud mode",
        value=mode,
        severity=mode_severity,
        message=f"Aerobag Cloud mode is {mode}",
    )

    database_healthy = payload.get("database_healthy") is True
    add_metric(
        metrics,
        metric_id="aerobag_cloud.database_healthy",
        label="Aerobag Cloud database healthy",
        value=database_healthy,
        severity="ok" if database_healthy else "critical",
        message=(
            "Aerobag Cloud database is healthy"
            if database_healthy
            else "Aerobag Cloud reports an unhealthy database"
        ),
    )

    for status_metric in payload.get("metrics", []):
        if not isinstance(status_metric, dict) or not isinstance(status_metric.get("id"), str):
            continue
        current = status_metric.get("current")
        if not isinstance(current, (int, float)):
            continue
        warning = status_metric.get("warning_at")
        critical = status_metric.get("critical_at")
        hard_limit = status_metric.get("hard_limit")
        lower_is_worse = status_metric.get("lower_is_worse") is True
        severity = "ok"
        if lower_is_worse:
            if isinstance(hard_limit, (int, float)) and current <= hard_limit:
                severity = "critical"
            elif isinstance(critical, (int, float)) and current <= critical:
                severity = "critical"
            elif isinstance(warning, (int, float)) and current <= warning:
                severity = "warning"
        else:
            if isinstance(hard_limit, (int, float)) and current >= hard_limit:
                severity = "critical"
            elif isinstance(critical, (int, float)) and current >= critical:
                severity = "critical"
            elif isinstance(warning, (int, float)) and current >= warning:
                severity = "warning"
        metric_id = status_metric["id"]
        add_metric(
            metrics,
            metric_id=f"aerobag_cloud.{metric_id}",
            label=f"Aerobag Cloud {metric_id.replace('_', ' ')}",
            value=current,
            severity=severity,
            warning_threshold=warning,
            critical_threshold=critical if critical is not None else hard_limit,
            message=f"Aerobag Cloud {metric_id.replace('_', ' ')}: {current}",
            details={
                "peak": status_metric.get("peak"),
                "hard_limit": hard_limit,
                "window_seconds": status_metric.get("window_seconds"),
                "rejected_in_window": status_metric.get("rejected_in_window"),
                "lower_is_worse": lower_is_worse,
            },
        )
def add_live_feed_metrics(
    metrics: list[dict[str, Any]], facts: dict[str, Any], now: datetime
) -> None:
    payload = facts["inputs"]["live_feeds_status"].get("payload")
    products = payload.get("products") if isinstance(payload, dict) else None
    if not isinstance(products, dict):
        return
    policies = payload.get("product_policies") if isinstance(payload, dict) else None
    if not isinstance(policies, list):
        add_metric(
            metrics,
            metric_id="live_feed.product_policy.present",
            label="Live-feed product policy present",
            value=False,
            severity="critical",
            message="Live-feed status omitted its authoritative product policy",
        )
        return
    add_metric(
        metrics,
        metric_id="live_feed.product_policy.present",
        label="Live-feed product policy present",
        value=True,
        severity="ok",
        message="Live-feed product policy is present",
    )
    for policy in policies:
        if not isinstance(policy, dict):
            continue
        product = policy.get("product_id")
        display = policy.get("display_name")
        health = policy.get("operator_health")
        if (
            not isinstance(product, str)
            or not product
            or not isinstance(display, str)
            or not isinstance(health, dict)
            or not isinstance(health.get("warning_after_seconds"), int)
            or not isinstance(health.get("critical_after_seconds"), int)
        ):
            add_metric(
                metrics,
                metric_id="live_feed.product_policy.valid",
                label="Live-feed product policy valid",
                value=False,
                severity="critical",
                message="Live-feed status contains a malformed product policy",
            )
            continue
        warning_seconds = health["warning_after_seconds"]
        critical_seconds = health["critical_after_seconds"]
        status = products.get(product)
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
            failure_window_start = now - timedelta(
                seconds=LIVE_FEED_FAILURE_WINDOW_SECONDS
            )
            window_attempts = [
                attempt
                for attempt in attempts
                if isinstance(attempt, dict)
                and (
                    attempt_time := parse_time(attempt.get("attempted_at_utc"))
                )
                is not None
                and failure_window_start <= attempt_time <= now
            ]
            attempt_count = len(window_attempts)
            failed_attempts = [
                attempt
                for attempt in window_attempts
                if attempt.get("result") == "failure"
            ]
            failure_count = len(failed_attempts)
            failure_rate = failure_count / attempt_count if attempt_count else 0.0
            last_failure = failed_attempts[-1] if failed_attempts else None
            last_failure_time = (
                parse_time(last_failure.get("attempted_at_utc"))
                if last_failure is not None
                else None
            )
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
                failure_message = (
                    f"{display} failures in the last 2h: "
                    f"{failure_count}/{attempt_count} attempts"
                )
            else:
                failure_message = (
                    f"{display} failures in the last 2h: "
                    f"{failure_count}/{attempt_count} attempts; "
                    f"last failure {last_failure_age_seconds} seconds ago"
                )
            add_metric(
                metrics,
                metric_id=f"live_feed.{product}.failure_rate_2h",
                label=f"{display} 2h failure rate",
                value=round(failure_rate, 6),
                unit="ratio",
                severity=severity,
                warning_threshold=0.0,
                critical_threshold=0.5,
                message=failure_message,
                details={
                    "window_seconds": LIVE_FEED_FAILURE_WINDOW_SECONDS,
                    "attempt_count": attempt_count,
                    "failure_count": failure_count,
                    "last_failure_at_utc": (
                        last_failure.get("attempted_at_utc")
                        if last_failure is not None
                        else None
                    ),
                    "last_failure_phase": (
                        last_failure.get("phase") if last_failure is not None else None
                    ),
                    "last_error": (
                        last_failure.get("error") if last_failure is not None else None
                    ),
                    "last_failure_age_seconds": last_failure_age_seconds,
                    "failures": detail_failures,
                },
            )
        if product == "notams":
            source_samples = status.get("source_samples")
            recent_source_rejections: list[dict[str, Any]] = []
            if isinstance(source_samples, list):
                rejection_window_start = now - timedelta(
                    seconds=LIVE_FEED_FAILURE_WINDOW_SECONDS
                )
                recent_source_rejections = [
                    {
                        "observed_at_utc": sample.get("observed_at_utc"),
                        "cursor_utc": sample.get("cursor_utc"),
                        "rejected_count": int(sample.get("rejected_count") or 0),
                    }
                    for sample in source_samples
                    if isinstance(sample, dict)
                    and int(sample.get("rejected_count") or 0) > 0
                    and (
                        sample_time := parse_time(sample.get("observed_at_utc"))
                    )
                    is not None
                    and rejection_window_start <= sample_time <= now
                ]
            rejected_updates = sum(
                sample["rejected_count"] for sample in recent_source_rejections
            )
            add_metric(
                metrics,
                metric_id="live_feed.notams.rejected_api_updates_2h",
                label="NOTAM rejected API updates (2h)",
                value=rejected_updates,
                unit="records",
                severity="warning" if rejected_updates > 0 else "ok",
                warning_threshold=1,
                message=f"NOTAM rejected API updates in the last 2h: {rejected_updates}",
                details={"samples": recent_source_rejections[-10:]},
            )
            quality = status.get("quality")
            if isinstance(quality, dict):
                unanchored_count = quality.get(
                    "procedure_notams_without_ui_anchor"
                )
                if isinstance(unanchored_count, int):
                    expected = EXPECTED_NOTAM_PROCEDURE_WITHOUT_UI_ANCHOR
                    add_metric(
                        metrics,
                        metric_id=(
                            "live_feed.notams."
                            "procedure_notams_without_ui_anchor"
                        ),
                        label="Procedure NOTAMs without a UI anchor",
                        value=unanchored_count,
                        unit="records",
                        severity="warning" if unanchored_count > expected else "ok",
                        warning_threshold=expected + 1,
                        message=(
                            "Procedure NOTAMs without an airport or rendezvous key: "
                            f"{unanchored_count}; expected at most {expected}"
                        ),
                    )
                rejected_count = int(quality.get("rejected_row_count") or 0)
                add_metric(
                    metrics,
                    metric_id="live_feed.notams.rejected_row_count",
                    label="NOTAM rejected source rows",
                    value=rejected_count,
                    unit="rows",
                    severity="warning" if rejected_count > 0 else "ok",
                    warning_threshold=1,
                    message=f"NOTAM rejected source rows: {rejected_count}",
                    details={
                        "oldest_rejected_ingest_seq": quality.get(
                            "oldest_rejected_ingest_seq"
                        ),
                        "latest_rejected_ingest_seq": quality.get(
                            "latest_rejected_ingest_seq"
                        ),
                        "last_rejection_error": quality.get("last_rejection_error"),
                        "recent_rejections": quality.get("recent_rejections") or [],
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
    parsed_cycles: list[tuple[str, datetime]] = []
    for entry in cycles:
        if not isinstance(entry, dict):
            continue
        cycle = entry.get("cycle")
        effective = parse_date(entry.get("effective_date"))
        if not isinstance(cycle, str) or effective is None:
            continue
        effective_at = utc_midnight(effective)
        parsed_cycles.append((cycle, effective_at))

    newest_published_effective = max(
        (
            effective_at
            for cycle, effective_at in parsed_cycles
            if cycle in published_cycles
        ),
        default=None,
    )

    for cycle, effective_at in parsed_cycles:
        if (
            newest_published_effective is not None
            and effective_at < newest_published_effective
        ):
            continue
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


def graphable_scalar(value: object) -> bool | int | float | None:
    if isinstance(value, bool):
        return value
    if isinstance(value, int):
        return value
    if isinstance(value, float) and math.isfinite(value):
        return value
    return None


def compact_evaluation_metrics(evaluation: dict[str, Any]) -> dict[str, Any]:
    compact: dict[str, Any] = {}
    metrics = evaluation.get("metrics")
    if not isinstance(metrics, list):
        return compact
    for metric in metrics:
        if not isinstance(metric, dict) or not isinstance(metric.get("id"), str):
            continue
        value = graphable_scalar(metric.get("value"))
        if value is None:
            continue
        severity = metric.get("severity", "ok")
        compact[metric["id"]] = (
            value
            if severity == "ok"
            else {
                "value": value,
                "severity": severity,
            }
        )
    return compact


def history_metric_values(record: dict[str, Any]) -> dict[str, Any]:
    metrics = record.get("metrics")
    if isinstance(metrics, dict):
        return metrics
    evaluation = record.get("evaluation")
    if isinstance(evaluation, dict):
        return compact_evaluation_metrics(evaluation)
    return {}


def compact_existing_history_record(record: dict[str, Any]) -> dict[str, Any] | None:
    sampled_at = record.get("sampled_at_utc")
    if not isinstance(sampled_at, str):
        return None
    compact = {
        "schema_version": SCHEMA_VERSION,
        "history_schema_version": HISTORY_SCHEMA_VERSION,
        "sampled_at_utc": sampled_at,
        "metrics": history_metric_values(record),
    }
    product_key = record.get("product_facts_key")
    if isinstance(product_key, list):
        compact["product_facts_key"] = product_key
    product_counts = record.get("product_counts")
    if isinstance(product_counts, dict):
        compact["product_counts"] = product_counts
    return compact


def history_date_from_path(path: Path) -> date | None:
    prefix = "pipeline_health-"
    if not path.name.startswith(prefix) or path.suffix != ".jsonl":
        return None
    return parse_date(path.name[len(prefix) : -len(path.suffix)])


def prune_history_files(
    health_root: Path,
    now: datetime,
    retention_days: int = HISTORY_RETENTION_DAYS,
) -> list[Path]:
    if retention_days <= 0:
        return []
    cutoff = now.date() - timedelta(days=retention_days - 1)
    removed: list[Path] = []
    for path in health_root.glob("pipeline_health-*.jsonl"):
        file_date = history_date_from_path(path)
        if file_date is not None and file_date < cutoff:
            path.unlink()
            removed.append(path)
    return removed


def history_file_is_compact(path: Path) -> bool:
    with path.open("rb") as stream:
        for line in stream:
            record = parse_history_line(line)
            if record is not None:
                return record.get("history_schema_version") == HISTORY_SCHEMA_VERSION
    return True


def migrate_history_file(path: Path) -> bool:
    if history_file_is_compact(path):
        return False
    temporary = path.with_name(f".{path.name}.compact.tmp")
    temporary.unlink(missing_ok=True)
    try:
        with path.open("rb") as source, temporary.open("w", encoding="utf-8") as output:
            for line in source:
                record = parse_history_line(line)
                if record is None:
                    continue
                compact = compact_existing_history_record(record)
                if compact is None:
                    continue
                output.write(
                    json.dumps(compact, sort_keys=True, separators=(",", ":")) + "\n"
                )
        temporary.replace(path)
    except Exception:
        temporary.unlink(missing_ok=True)
        raise
    return True


def maintain_history(health_root: Path, now: datetime) -> None:
    key = health_root.resolve()
    with _history_maintenance_lock:
        if _history_maintenance_dates.get(key) == now.date():
            return
        health_root.mkdir(parents=True, exist_ok=True)
        prune_history_files(health_root, now)
        for path in history_paths_newest_first(health_root, now):
            migrate_history_file(path)
        _history_maintenance_dates[key] = now.date()


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
        stream.write(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")


def compact_history_record(
    facts: dict[str, Any],
    evaluation: dict[str, Any],
) -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "history_schema_version": HISTORY_SCHEMA_VERSION,
        "sampled_at_utc": facts["sampled_at_utc"],
        "metrics": compact_evaluation_metrics(evaluation),
        "product_facts_key": list(product_facts_publication_key(facts)),
        "product_counts": aggregate_product_counts(facts),
    }


def current_health_record(
    facts: dict[str, Any], evaluation: dict[str, Any]
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
    maintain_history(config.health_root, now)
    history_path = history_path_for_date(config.health_root, now.date())
    previous = read_history(config.health_root, now=now).records
    facts = collect_facts(config, now)
    evaluation = evaluate_health(facts, previous, now)
    history_record = compact_history_record(facts, evaluation)
    current_record = current_health_record(facts, evaluation)
    append_history(history_path, history_record)
    write_current(config.health_root / "pipeline-health-current.json", current_record)
    return current_record


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
    response["monitor_poll_seconds"] = config.poll_seconds
    return response


def stored_metric_value(metric: Any) -> tuple[bool | int | float | None, str]:
    if isinstance(metric, dict):
        value = graphable_scalar(metric.get("value"))
        severity = metric.get("severity", "ok")
        return value, severity if isinstance(severity, str) else "ok"
    return graphable_scalar(metric), "ok"


def compact_metric_series(
    records: list[dict[str, Any]],
    *,
    now: datetime | None = None,
    window_seconds: int = DASHBOARD_WINDOW_SECONDS,
    bucket_seconds: int = DASHBOARD_BUCKET_SECONDS,
) -> dict[str, Any]:
    now = now or utc_now()
    bucket_limit = max(1, math.ceil(window_seconds / bucket_seconds))
    window_start = now - timedelta(seconds=window_seconds)
    buckets: dict[int, dict[str, dict[str, Any]]] = {}
    source_records = 0
    latest_sampled_at: datetime | None = None
    for record in records:
        sampled_at = parse_time(record.get("sampled_at_utc"))
        if sampled_at is None or sampled_at < window_start or sampled_at > now:
            continue
        source_records += 1
        if latest_sampled_at is None or sampled_at > latest_sampled_at:
            latest_sampled_at = sampled_at
        bucket_epoch = int(sampled_at.timestamp()) // bucket_seconds * bucket_seconds
        bucket = buckets.setdefault(bucket_epoch, {})
        for metric_id, stored in history_metric_values(record).items():
            if not isinstance(metric_id, str):
                continue
            value, severity = stored_metric_value(stored)
            if value is None:
                continue
            numeric = int(value) if isinstance(value, bool) else value
            aggregate = bucket.get(metric_id)
            if aggregate is None:
                bucket[metric_id] = {
                    "first": numeric,
                    "last": numeric,
                    "min": numeric,
                    "max": numeric,
                    "severity": SEVERITY_RANK.get(severity, 0),
                    "first_at": sampled_at,
                    "last_at": sampled_at,
                }
                continue
            if sampled_at < aggregate["first_at"]:
                aggregate["first"] = numeric
                aggregate["first_at"] = sampled_at
            if sampled_at >= aggregate["last_at"]:
                aggregate["last"] = numeric
                aggregate["last_at"] = sampled_at
            aggregate["min"] = min(aggregate["min"], numeric)
            aggregate["max"] = max(aggregate["max"], numeric)
            aggregate["severity"] = max(
                aggregate["severity"], SEVERITY_RANK.get(severity, 0)
            )

    bucket_epochs = sorted(buckets)[-bucket_limit:]
    metric_ids = sorted(
        {
            metric_id
            for bucket_epoch in bucket_epochs
            for metric_id in buckets[bucket_epoch]
        }
    )
    series: dict[str, dict[str, list[Any]]] = {}
    for metric_id in metric_ids:
        columns = {
            "first": [],
            "last": [],
            "min": [],
            "max": [],
            "severity": [],
        }
        for bucket_epoch in bucket_epochs:
            aggregate = buckets[bucket_epoch].get(metric_id)
            for name in columns:
                columns[name].append(aggregate.get(name) if aggregate is not None else None)
        series[metric_id] = columns

    return {
        "schema_version": SCHEMA_VERSION,
        "generated_at_utc": iso_utc(now),
        "window_seconds": window_seconds,
        "bucket_seconds": bucket_seconds,
        "bucket_limit": bucket_limit,
        "source_records": source_records,
        "latest_sampled_at_utc": (
            iso_utc(latest_sampled_at) if latest_sampled_at is not None else None
        ),
        "times": [iso_utc(datetime.fromtimestamp(value, timezone.utc)) for value in bucket_epochs],
        "series": series,
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
                history = read_history(config.health_root, HISTORY_RECORD_LIMIT)
                response = compact_metric_series(history.records)
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
    .metric-details-host { grid-column:1 / -1; }
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
const severityNames = ["ok", "warning", "critical"];
const esc = (value) => String(value ?? "").replace(/[&<>"']/g, (ch) => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[ch]));
const dashboard = {
  current: null,
  series: null,
  rows: new Map(),
  plots: new Map(),
  observer: null,
  rowOrder: [],
  refreshTimer: null,
  refreshInFlight: false,
  forceSeriesReload: false,
};
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
function severityTrace(points, severity) {
  const x = [], y = [];
  for (let index = 1; index < points.length; index += 1) {
    const previous = points[index - 1];
    const current = points[index];
    if (!previous || !current) continue;
    const segmentSeverity = (severityRank[current.severity] || 0) >= (severityRank[previous.severity] || 0)
      ? current.severity
      : previous.severity;
    if (segmentSeverity !== severity) continue;
    x.push(previous.x, current.x, null);
    y.push(previous.y, current.y, null);
  }
  if (!x.length) return null;
  const colors = { ok:"#50d890", warning:"#f0c85a", critical:"#ff6b6b" };
  return {
    type:"scatter",
    mode:"lines",
    name:severity,
    x,
    y,
    line:{color:colors[severity], width:2},
    connectgaps:false,
  };
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
function graphableMetrics(metrics) {
  return (metrics || []).filter((metric) => graphValue(metric.value) !== null);
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
function updateMetricRow(row, metric) {
  row.metric = metric;
  row.title.textContent = metric.label || metric.id;
  row.pill.textContent = metric.severity || "ok";
  row.pill.className = `pill ${cls(metric.severity)}`;
  row.value.innerHTML = `${esc(formatValue(metric))}${metric.unit ? ` <span class="muted">${esc(metric.unit)}</span>` : ""}`;
  row.message.textContent = metric.message || "";
  row.details.innerHTML = renderMetricDetails(metric);
}
function deactivatePlot(metricId) {
  const row = dashboard.rows.get(metricId);
  if (!row || !dashboard.plots.has(metricId)) return;
  if (window.Plotly) Plotly.purge(row.plot);
  row.plot.replaceChildren();
  dashboard.plots.delete(metricId);
}
function purgeAllPlots() {
  for (const metricId of [...dashboard.plots.keys()]) {
    deactivatePlot(metricId);
  }
}
function buildMetricRows(metrics) {
  const container = document.getElementById("metricRows");
  if (dashboard.observer) dashboard.observer.disconnect();
  purgeAllPlots();
  dashboard.rows.clear();
  container.innerHTML = metrics.length
    ? metrics.map((metric, index) => `<section class="metric-row">
        <div>
          <div class="metric-title"><h3></h3><span class="pill"></span></div>
          <div class="metric-id">${esc(metric.id)}</div>
          <div class="metric-value"></div>
          <div class="metric-message"></div>
        </div>
        <div id="metricPlot${index}" class="plot"></div>
        <div class="metric-details-host"></div>
      </section>`).join("")
    : `<section><div class="muted">No graphable metrics yet.</div></section>`;
  dashboard.observer = new IntersectionObserver((entries) => {
    for (const entry of entries) {
      const metricId = entry.target.dataset.metricId;
      if (!metricId) continue;
      if (entry.isIntersecting) activatePlot(metricId);
      else deactivatePlot(metricId);
    }
  }, { rootMargin:"500px 0px" });
  metrics.forEach((metric, index) => {
    const element = container.children[index];
    element.dataset.metricId = metric.id;
    const row = {
      element,
      plot: element.querySelector(".plot"),
      title: element.querySelector("h3"),
      pill: element.querySelector(".pill"),
      value: element.querySelector(".metric-value"),
      message: element.querySelector(".metric-message"),
      details: element.querySelector(".metric-details-host"),
      metric,
    };
    dashboard.rows.set(metric.id, row);
    updateMetricRow(row, metric);
    dashboard.observer.observe(element);
  });
  dashboard.rowOrder = metrics.map((metric) => metric.id);
}
function ensureMetricRows(current) {
  const metrics = graphableMetrics(current.evaluation?.metrics || []);
  const ids = metrics.map((metric) => metric.id).sort();
  const existingIds = [...dashboard.rowOrder].sort();
  const changed = ids.length !== existingIds.length || ids.some((id, index) => id !== existingIds[index]);
  if (changed) {
    const ordered = [...metrics].sort((a, b) => {
      const severity = (severityRank[b.severity || "ok"] || 0) - (severityRank[a.severity || "ok"] || 0);
      return severity || String(a.id).localeCompare(String(b.id));
    });
    buildMetricRows(ordered);
  }
  for (const metric of metrics) {
    const row = dashboard.rows.get(metric.id);
    if (row) updateMetricRow(row, metric);
  }
}
function metricPlotData(metricId) {
  const columns = dashboard.series?.series?.[metricId];
  const times = dashboard.series?.times || [];
  if (!columns) return { points:[], envelopeX:[], minimums:[], maximums:[] };
  const points = [], envelopeX = [], minimums = [], maximums = [];
  let previousBucketStart = null;
  const bucketMilliseconds = Number(dashboard.series.bucket_seconds || 300) * 1000;
  for (let index = 0; index < times.length; index += 1) {
    const first = graphValue(columns.first?.[index]);
    const last = graphValue(columns.last?.[index]);
    const minimum = graphValue(columns.min?.[index]);
    const maximum = graphValue(columns.max?.[index]);
    if (first === null || last === null || minimum === null || maximum === null) continue;
    const bucketStart = Date.parse(times[index]);
    if (!Number.isFinite(bucketStart)) continue;
    if (previousBucketStart !== null && bucketStart - previousBucketStart > bucketMilliseconds * 1.5) points.push(null);
    const severity = severityNames[columns.severity?.[index] || 0] || "ok";
    points.push(
      { x:new Date(bucketStart).toISOString(), y:first, severity },
      { x:new Date(bucketStart + bucketMilliseconds).toISOString(), y:last, severity },
    );
    envelopeX.push(new Date(bucketStart + bucketMilliseconds / 2).toISOString());
    minimums.push(minimum);
    maximums.push(maximum);
    previousBucketStart = bucketStart;
  }
  return { points, envelopeX, minimums, maximums };
}
function renderPlot(metricId) {
  if (!window.Plotly || !dashboard.plots.has(metricId)) return;
  const row = dashboard.rows.get(metricId);
  if (!row) return;
  const metric = row.metric;
  const { points, envelopeX, minimums, maximums } = metricPlotData(metricId);
  const traces = [];
  if (envelopeX.length) {
    traces.push(
      { type:"scatter", mode:"lines", name:"range maximum", x:envelopeX, y:maximums, line:{width:0}, hoverinfo:"skip", showlegend:false },
      { type:"scatter", mode:"lines", name:"observed range", x:envelopeX, y:minimums, line:{width:0}, fill:"tonexty", fillcolor:"rgba(169,181,173,0.14)", hoverinfo:"skip", showlegend:false },
    );
  }
  traces.push(...["ok", "warning", "critical"]
    .map((severity) => severityTrace(points, severity))
    .filter((trace) => trace !== null));
  const nonNullPoints = points.filter(Boolean);
  if (typeof metric.warning_threshold === "number" && nonNullPoints.length) {
    traces.push({ type:"scatter", mode:"lines", name:"warning threshold", x:[nonNullPoints[0].x, nonNullPoints[nonNullPoints.length - 1].x], y:[metric.warning_threshold, metric.warning_threshold], line:{color:"#f0c85a", width:1, dash:"dot"} });
  }
  if (typeof metric.critical_threshold === "number" && nonNullPoints.length) {
    traces.push({ type:"scatter", mode:"lines", name:"critical threshold", x:[nonNullPoints[0].x, nonNullPoints[nonNullPoints.length - 1].x], y:[metric.critical_threshold, metric.critical_threshold], line:{color:"#ff6b6b", width:1, dash:"dot"} });
  }
  Plotly.react(row.plot, traces, {
      paper_bgcolor:"#171b19",
      plot_bgcolor:"#171b19",
      font:{color:"#edf3ee", size:11},
      margin:{l:48,r:12,t:8,b:32},
      xaxis:{type:"date", gridcolor:"#303833"},
      yaxis:{title:metric.unit || "", gridcolor:"#303833", rangemode:"tozero"},
      showlegend:traces.length > 1,
      legend:{orientation:"h", x:0, y:1.18},
      uirevision:metricId,
    }, { responsive:true, displaylogo:false });
}
function activatePlot(metricId) {
  const row = dashboard.rows.get(metricId);
  if (!row || dashboard.plots.has(metricId)) return;
  dashboard.plots.set(metricId, row.plot);
  renderPlot(metricId);
}
function renderActivePlots() {
  for (const metricId of dashboard.plots.keys()) renderPlot(metricId);
}
function emptyColumns(length) {
  return {
    first:Array(length).fill(null),
    last:Array(length).fill(null),
    min:Array(length).fill(null),
    max:Array(length).fill(null),
    severity:Array(length).fill(null),
  };
}
function ensureSeriesColumns(metricId) {
  const series = dashboard.series;
  if (!series.series[metricId]) series.series[metricId] = emptyColumns(series.times.length);
  return series.series[metricId];
}
function appendSeriesBucket(bucketTime) {
  dashboard.series.times.push(bucketTime);
  for (const columns of Object.values(dashboard.series.series)) {
    for (const values of Object.values(columns)) values.push(null);
  }
  const limit = Number(dashboard.series.bucket_limit || 288);
  while (dashboard.series.times.length > limit) {
    dashboard.series.times.shift();
    for (const columns of Object.values(dashboard.series.series)) {
      for (const values of Object.values(columns)) values.shift();
    }
  }
}
function mergeCurrentSample(current) {
  const sampledAt = Date.parse(current.sampled_at_utc);
  if (!Number.isFinite(sampledAt) || !dashboard.series) return false;
  const latest = Date.parse(dashboard.series.latest_sampled_at_utc || "");
  if (Number.isFinite(latest) && sampledAt <= latest) return false;
  const bucketSeconds = Number(dashboard.series.bucket_seconds || 300);
  const bucketMilliseconds = bucketSeconds * 1000;
  const bucketTime = new Date(Math.floor(sampledAt / bucketMilliseconds) * bucketMilliseconds).toISOString();
  let index = dashboard.series.times.indexOf(bucketTime);
  if (index < 0) {
    const lastTime = dashboard.series.times.at(-1);
    if (lastTime && bucketTime < lastTime) return false;
    appendSeriesBucket(bucketTime);
    index = dashboard.series.times.length - 1;
  }
  for (const metric of current.evaluation?.metrics || []) {
    const value = graphValue(metric.value);
    if (value === null) continue;
    const columns = ensureSeriesColumns(metric.id);
    if (columns.first[index] === null || columns.first[index] === undefined) {
      columns.first[index] = value;
      columns.min[index] = value;
      columns.max[index] = value;
      columns.severity[index] = severityRank[metric.severity || "ok"] || 0;
    } else {
      columns.min[index] = Math.min(columns.min[index], value);
      columns.max[index] = Math.max(columns.max[index], value);
      columns.severity[index] = Math.max(columns.severity[index] || 0, severityRank[metric.severity || "ok"] || 0);
    }
    columns.last[index] = value;
  }
  dashboard.series.latest_sampled_at_utc = current.sampled_at_utc;
  return true;
}
function shouldReloadSeries(current) {
  if (!dashboard.series) return true;
  const sampledAt = Date.parse(current.sampled_at_utc);
  const latest = Date.parse(dashboard.series.latest_sampled_at_utc || "");
  if (!Number.isFinite(sampledAt)) return false;
  if (!Number.isFinite(latest)) return true;
  if (sampledAt <= latest) return false;
  const pollMilliseconds = Number(current.monitor_poll_seconds || 60) * 1000;
  return sampledAt - latest > pollMilliseconds * 1.5 + 5000;
}
async function refresh(forceSeriesReload = false) {
  const current = await loadJson("/pipeline-health/current.json");
  if (forceSeriesReload || shouldReloadSeries(current)) {
    dashboard.series = await loadJson("/pipeline-health/series.json");
  }
  dashboard.current = current;
  mergeCurrentSample(current);
  renderCurrent(current);
  ensureMetricRows(current);
  renderActivePlots();
}
async function refreshLoop(forceSeriesReload = false) {
  if (dashboard.refreshInFlight) {
    dashboard.forceSeriesReload ||= forceSeriesReload;
    return;
  }
  dashboard.refreshInFlight = true;
  try {
    await refresh(forceSeriesReload || dashboard.forceSeriesReload);
    dashboard.forceSeriesReload = false;
  } catch (error) {
    document.getElementById("sampleAge").textContent = String(error);
  } finally {
    dashboard.refreshInFlight = false;
    clearTimeout(dashboard.refreshTimer);
    dashboard.refreshTimer = setTimeout(refreshLoop, 30000);
  }
}
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState !== "visible") return;
  clearTimeout(dashboard.refreshTimer);
  void refreshLoop(true);
});
void refreshLoop(true);
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
    parser.add_argument("--cloud-status-url", default=defaults.cloud_status_url)
    parser.add_argument(
        "--cloud-status-secret",
        type=Path,
        default=defaults.cloud_status_secret_path,
    )
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
        cloud_status_url=args.cloud_status_url,
        cloud_status_secret_path=args.cloud_status_secret,
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
