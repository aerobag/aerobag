#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import threading
import time
from collections import deque
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
HISTORY_LIMIT = 10_000

LIVE_FEED_STALE_THRESHOLDS: dict[str, tuple[int, int]] = {
    "tafs": (60 * 60, 3 * 60 * 60),
    "metars": (5 * 60, 30 * 60),
    "obstacles": (2 * 24 * 60 * 60, 7 * 24 * 60 * 60),
    "tfrs": (3 * 60 * 60, 6 * 60 * 60),
    "nexrad": (5 * 60, 15 * 60),
}

LIVE_FEED_DISPLAY_NAMES = {
    "tafs": "TAFs",
    "metars": "METARs",
    "obstacles": "Obstacles",
    "tfrs": "TFRs",
    "nexrad": "NEXRAD",
}

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
            failure_count = sum(
                1
                for attempt in attempts
                if isinstance(attempt, dict) and attempt.get("result") == "failure"
            )
            failure_rate = failure_count / attempt_count
            severity = "critical" if attempt_count >= 3 and failure_rate >= 0.5 else (
                "warning" if failure_count > 0 else "ok"
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
                message=(
                    f"{display} recent failures: {failure_count}/{attempt_count} attempts"
                ),
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
    previous_counts = latest_product_counts(previous_records)
    for product in iter_current_product_facts(facts):
        product_id = product.get("product_id")
        family = product.get("family")
        region = product.get("region_id") or ""
        key = f"{family}:{region}"
        for count_name in ["error_count", "warning_count"]:
            value = int(product.get(count_name) or 0)
            previous = previous_counts.get(key, {}).get(count_name)
            increased = previous is not None and value > previous
            severity = "warning" if value > 0 and (previous is None or increased) else "ok"
            add_metric(
                metrics,
                metric_id=f"cycle_product.{product_id}.{count_name}",
                label=f"{product_id} {count_name}",
                value=value,
                severity=severity,
                warning_threshold=(previous + 1) if previous is not None else 1,
                message=(
                    f"{product_id} {count_name} increased from {previous} to {value}"
                    if increased
                    else f"{product_id} {count_name}: {value}"
                ),
            )


def iter_current_product_facts(facts: dict[str, Any]) -> list[dict[str, Any]]:
    products: list[dict[str, Any]] = []
    for entry in facts.get("inputs", {}).get("product_facts", []):
        payload = entry.get("payload") if isinstance(entry, dict) else None
        payload_products = payload.get("products") if isinstance(payload, dict) else None
        if isinstance(payload_products, list):
            products.extend(product for product in payload_products if isinstance(product, dict))
    return products


def latest_product_counts(previous_records: list[dict[str, Any]]) -> dict[str, dict[str, int]]:
    for record in reversed(previous_records):
        facts = record.get("facts")
        if not isinstance(facts, dict):
            continue
        counts: dict[str, dict[str, int]] = {}
        for product in iter_current_product_facts(facts):
            family = product.get("family")
            region = product.get("region_id") or ""
            if isinstance(family, str):
                counts[f"{family}:{region}"] = {
                    "error_count": int(product.get("error_count") or 0),
                    "warning_count": int(product.get("warning_count") or 0),
                }
        if counts:
            return counts
    return {}


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
    now_date = now.date()
    for entry in cycles:
        if not isinstance(entry, dict):
            continue
        cycle = entry.get("cycle")
        effective = parse_date(entry.get("effective_date"))
        if not isinstance(cycle, str) or effective is None:
            continue
        due_date = effective - timedelta(days=20)
        if due_date > now_date:
            continue
        if effective < now_date - timedelta(days=35):
            continue
        if cycle in published_cycles:
            add_metric(
                metrics,
                metric_id=f"cycle_calendar.{cycle}.published",
                label=f"FAA cycle {cycle} published",
                value=True,
                severity="ok",
                message=f"FAA cycle {cycle} is published",
            )
            continue
        late_seconds = max(0, int((now_date - due_date).days * 24 * 60 * 60))
        severity = threshold_severity(late_seconds, 24 * 60 * 60, 3 * 24 * 60 * 60)
        add_metric(
            metrics,
            metric_id=f"cycle_calendar.{cycle}.missing_seconds",
            label=f"FAA cycle {cycle} missing",
            value=late_seconds,
            unit="seconds",
            severity=severity,
            warning_threshold=24 * 60 * 60,
            critical_threshold=3 * 24 * 60 * 60,
            message=f"FAA cycle {cycle} is not published {late_seconds} seconds after due date",
        )


def read_history(path: Path, limit: int = HISTORY_LIMIT) -> list[dict[str, Any]]:
    if not path.exists():
        return []
    records: deque[dict[str, Any]] = deque(maxlen=limit)
    with path.open("r", encoding="utf-8") as stream:
        for line in stream:
            line = line.strip()
            if not line:
                continue
            try:
                value = json.loads(line)
            except json.JSONDecodeError:
                continue
            if isinstance(value, dict):
                records.append(value)
    return list(records)


def append_history(path: Path, record: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as stream:
        stream.write(json.dumps(record, sort_keys=True) + "\n")


def write_current(path: Path, record: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".json.tmp")
    tmp.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    tmp.replace(path)


def run_sample(config: MonitorConfig) -> dict[str, Any]:
    now = utc_now()
    history_path = config.health_root / "pipeline-health.jsonl"
    previous = read_history(history_path)
    facts = collect_facts(config, now)
    evaluation = evaluate_health(facts, previous, now)
    record = {
        "schema_version": SCHEMA_VERSION,
        "sampled_at_utc": facts["sampled_at_utc"],
        "facts": facts,
        "evaluation": evaluation,
    }
    append_history(history_path, record)
    write_current(config.health_root / "pipeline-health-current.json", record)
    return record


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
                current_path = config.health_root / "pipeline-health-current.json"
                if not current_path.exists():
                    run_sample(config)
                self._send_file(current_path, "application/json", send_body)
                return
            if path == "/pipeline-health/history.json":
                query = parse_qs(urlparse(self.path).query)
                limit = int(query.get("limit", ["200"])[0])
                records = read_history(config.health_root / "pipeline-health.jsonl", limit)
                self._send(
                    200,
                    json.dumps(records[-limit:], indent=2, sort_keys=True) + "\n",
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
                self.wfile.write(payload)

    print(f"pipeline health serving on http://{host}:{port}/pipeline-health/", flush=True)
    ThreadingHTTPServer((host, port), Handler).serve_forever()


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
    body { margin:0; min-height:100vh; background:var(--bg); color:var(--text); font:14px/1.45 ui-sans-serif, system-ui, sans-serif; letter-spacing:0; }
    main { padding:20px; max-width:1500px; margin:0 auto; }
    header { display:flex; justify-content:space-between; gap:16px; align-items:flex-start; margin-bottom:16px; }
    h1 { margin:0; font-size:24px; }
    h2 { margin:0 0 10px; font-size:16px; }
    .muted { color:var(--muted); }
    .pill { display:inline-flex; align-items:center; border:1px solid var(--line); border-radius:999px; padding:4px 10px; font-weight:700; text-transform:uppercase; }
    .ok { color:var(--ok); border-color:color-mix(in srgb, var(--ok) 55%, var(--line)); }
    .warning { color:var(--warn); border-color:color-mix(in srgb, var(--warn) 55%, var(--line)); }
    .critical { color:var(--crit); border-color:color-mix(in srgb, var(--crit) 55%, var(--line)); }
    .grid { display:grid; grid-template-columns:repeat(auto-fit, minmax(360px, 1fr)); gap:12px; }
    section { background:var(--panel); border:1px solid var(--line); border-radius:8px; padding:14px; }
    table { width:100%; border-collapse:collapse; }
    th, td { border-bottom:1px solid var(--line); padding:6px 8px; text-align:left; vertical-align:top; }
    th { color:var(--muted); font-weight:600; }
    .plot { height:300px; }
  </style>
</head>
<body>
<main>
  <header>
    <div>
      <h1>Pipeline Health</h1>
      <div id="generated" class="muted">Loading...</div>
    </div>
    <div id="topline" class="pill">...</div>
  </header>
  <div class="grid">
    <section>
      <h2>Alerts</h2>
      <div id="alerts"></div>
    </section>
    <section>
      <h2>Metrics</h2>
      <div id="metrics"></div>
    </section>
  </div>
  <section style="margin-top:12px">
    <h2>Live Feed Staleness</h2>
    <div id="stalePlot" class="plot"></div>
  </section>
</main>
<script>
const cls = (severity) => severity === "critical" ? "critical" : severity === "warning" ? "warning" : "ok";
const esc = (value) => String(value ?? "").replace(/[&<>"']/g, (ch) => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[ch]));
async function loadJson(path) {
  const response = await fetch(path, { cache: "no-store" });
  if (!response.ok) throw new Error(`${path}: ${response.status}`);
  return response.json();
}
function renderCurrent(record) {
  const evaln = record.evaluation;
  document.getElementById("generated").textContent = evaln.generated_at_utc;
  const top = document.getElementById("topline");
  top.textContent = evaln.top_line_status;
  top.className = `pill ${cls(evaln.top_line_status)}`;
  const alerts = evaln.alerts || [];
  document.getElementById("alerts").innerHTML = alerts.length
    ? `<table><thead><tr><th>Severity</th><th>Metric</th><th>Message</th></tr></thead><tbody>${alerts.map((a) => `<tr><td class="${cls(a.severity)}">${esc(a.severity)}</td><td>${esc(a.metric_id)}</td><td>${esc(a.message)}</td></tr>`).join("")}</tbody></table>`
    : `<div class="muted">No alerts.</div>`;
  const rows = (evaln.metrics || []).map((m) => `<tr><td class="${cls(m.severity)}">${esc(m.severity)}</td><td>${esc(m.id)}</td><td>${esc(m.value)}</td><td>${esc(m.unit || "")}</td><td>${esc(m.message)}</td></tr>`).join("");
  document.getElementById("metrics").innerHTML = `<table><thead><tr><th>Status</th><th>Metric</th><th>Value</th><th>Unit</th><th>Message</th></tr></thead><tbody>${rows}</tbody></table>`;
}
function renderHistory(history) {
  if (!window.Plotly) return;
  const products = ["metars", "tafs", "nexrad", "tfrs", "obstacles"];
  const traces = products.map((product) => {
    const x = [], y = [];
    for (const record of history) {
      const metric = (record.evaluation?.metrics || []).find((m) => m.id === `live_feed.${product}.stale_seconds`);
      if (metric && typeof metric.value === "number") {
        x.push(record.sampled_at_utc);
        y.push(metric.value / 60);
      }
    }
    return { type:"scatter", mode:"lines", name:product, x, y };
  }).filter((trace) => trace.x.length);
  Plotly.react("stalePlot", traces, {
    paper_bgcolor:"#171b19", plot_bgcolor:"#171b19", font:{color:"#edf3ee"},
    margin:{l:55,r:20,t:12,b:45}, xaxis:{type:"date", gridcolor:"#303833"}, yaxis:{title:"minutes stale", gridcolor:"#303833", rangemode:"tozero"},
    legend:{orientation:"h"}
  }, { responsive:true, displaylogo:false });
}
async function refresh() {
  const [current, history] = await Promise.all([
    loadJson("/pipeline-health/current.json"),
    loadJson("/pipeline-health/history.json?limit=240"),
  ]);
  renderCurrent(current);
  renderHistory(history);
}
refresh().catch((error) => {
  document.getElementById("generated").textContent = String(error);
});
setInterval(() => refresh().catch((error) => {
  document.getElementById("generated").textContent = String(error);
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
