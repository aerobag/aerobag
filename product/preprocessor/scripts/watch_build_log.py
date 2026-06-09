#!/usr/bin/env python3

import argparse
import curses
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
import re
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlparse


LAUNCH_RE = re.compile(
    r"^(?:(?P<wall>\S+)\s+)?(?P<ts>\+\d+:\d+(?::\d+)?)\s+(?:product-scheduler-)?launch\s+(?P<task>\S+)\s+"
    r"launched=(?P<launched>\d+)/(?P<total>\d+)\s+"
    r"completed=(?P<completed>\d+)/(?P=total)\s+"
    r"weight=(?P<weight>\d+)\s+running_units=(?P<running>\d+)/(?P<budget>\d+)"
)

COMPLETE_RE = re.compile(
    r"^(?:(?P<wall>\S+)\s+)?(?P<ts>\+\d+(?::\d+){1,2})\s+(?:product-scheduler-)?complete\s+(?P<task>\S+)\s+"
    r"completed=(?P<completed>\d+)/(?P<total>\d+)\s+"
    r"running_units=(?P<running>\d+)/(?P<budget>\d+)(?P<rest>.*)$"
)

FINAL_RE = re.compile(
    r"^(?:(?P<wall>\S+)\s+)?(?P<ts>\+\d+:\d+(?::\d+)?)\s+complete\s+(?P<result>PASS|FAIL)(?P<rest>.*)$"
)

READY_RE = re.compile(
    r"^.*scheduler-ready\s+tasks=(?P<total>\d+)\s+work_unit_budget=(?P<budget>\d+).*$"
)

BEGIN_RE = re.compile(
    r"^(?:(?P<wall>\S+)\s+)?(?P<ts>\+\d+:\d+(?::\d+)?)\s+begin\s+(?P<rest>.*)$"
)

CYCLE_RE = re.compile(
    r"^(?:(?P<wall>\S+)\s+)?(?P<ts>\+\d+:\d+(?::\d+)?)\s+cycle\s+bundle=(?P<bundle>\S+)\s+(?P<rest>.*)$"
)


@dataclass
class TaskState:
    task: str
    launched_at: str
    launched_wall: str
    weight: int
    status: str
    details: str = ""
    completed_at: str | None = None
    completed_wall: str | None = None


@dataclass
class DiagnosticsState:
    status: str
    text: str
    color: str


class BuildState:
    def __init__(self) -> None:
        self.total_tasks = 0
        self.work_unit_budget = 0
        self.launched = 0
        self.completed = 0
        self.running_units = 0
        self.header = ""
        self.publish_label = ""
        self.cycle_summary = ""
        self.bundle_cycle = "?"
        self.pid: int | None = None
        self.build_root: Path | None = None
        self.publish_dir: Path | None = None
        self.diagnostic_error_count: int | None = None
        self.final_result: str | None = None
        self.final_details = ""
        self.final_at: str | None = None
        self.tasks: dict[str, TaskState] = {}
        self.completion_order: list[str] = []
        self.last_line = ""
        self.last_timestamp = ""

    def apply_line(self, line: str) -> None:
        self.last_line = line

        match = BEGIN_RE.match(line)
        if match:
            self._reset_for_new_run()
            self.header = f"{match.group('ts')} {match.group('rest')}"
            self.pid = parse_pid(match.group("rest"))
            self.build_root = parse_build_root(match.group("rest"))
            self.publish_dir = parse_publish_dir(match.group("rest"))
            self.publish_label = parse_publish_label(match.group("rest"))
            if not self.publish_label:
                self.publish_label = infer_publish_label_from_publish_dir(self.publish_dir)
            self.last_timestamp = match.group("ts")
            return

        match = CYCLE_RE.match(line)
        if match:
            self.last_timestamp = match.group("ts")
            self.bundle_cycle = match.group("bundle")
            self.cycle_summary = match.group("rest")
            return

        match = READY_RE.match(line)
        if match:
            self.total_tasks = int(match.group("total"))
            self.work_unit_budget = int(match.group("budget"))
            return

        match = LAUNCH_RE.match(line)
        if match:
            self.last_timestamp = match.group("ts")
            task = match.group("task")
            self.total_tasks = int(match.group("total"))
            self.launched = int(match.group("launched"))
            self.completed = int(match.group("completed"))
            self.running_units = int(match.group("running"))
            self.work_unit_budget = int(match.group("budget"))
            self.tasks[task] = TaskState(
                task=task,
                launched_at=match.group("ts"),
                launched_wall=match.group("wall"),
                weight=int(match.group("weight")),
                status="active",
            )
            return

        match = COMPLETE_RE.match(line)
        if match:
            self.last_timestamp = match.group("ts")
            task = match.group("task")
            self.total_tasks = int(match.group("total"))
            self.completed = int(match.group("completed"))
            self.running_units = int(match.group("running"))
            self.work_unit_budget = int(match.group("budget"))
            task_state = self.tasks.get(task)
            if task_state is None:
                task_state = TaskState(
                    task=task,
                    launched_at="?",
                    launched_wall=match.group("wall"),
                    weight=0,
                    status="done",
                )
                self.tasks[task] = task_state
            task_state.status = "done"
            task_state.completed_at = match.group("ts")
            task_state.completed_wall = match.group("wall")
            task_state.details = match.group("rest").strip()
            if task == "current-artifacts":
                self.diagnostic_error_count = parse_diagnostic_error_count(
                    task_state.details
                )
            if task not in self.completion_order:
                self.completion_order.append(task)
            return

        match = FINAL_RE.match(line)
        if match:
            self.last_timestamp = match.group("ts")
            self.final_result = match.group("result")
            self.final_details = match.group("rest").strip()
            self.final_at = match.group("ts")
            return

    def _reset_for_new_run(self) -> None:
        self.total_tasks = 0
        self.work_unit_budget = 0
        self.launched = 0
        self.completed = 0
        self.running_units = 0
        self.header = ""
        self.publish_label = ""
        self.cycle_summary = ""
        self.bundle_cycle = "?"
        self.pid = None
        self.build_root = None
        self.publish_dir = None
        self.diagnostic_error_count = None
        self.final_result = None
        self.final_details = ""
        self.final_at = None
        self.tasks = {}
        self.completion_order = []
        self.last_timestamp = ""

    def active_tasks(self) -> list[TaskState]:
        return sorted(
            (task for task in self.tasks.values() if task.status == "active"),
            key=lambda task: (task.launched_at, task.task),
        )

    def recent_completed(self, limit: int) -> list[TaskState]:
        keys = self.completion_order[-limit:]
        return [self.tasks[key] for key in reversed(keys)]

    def pending_count(self) -> int:
        return max(self.total_tasks - self.launched, 0)


def read_state(log_path: Path) -> BuildState:
    state = BuildState()
    if not log_path.exists():
        return state
    with log_path.open("r", encoding="utf-8", errors="replace") as handle:
        for raw_line in handle:
            state.apply_line(raw_line.rstrip("\n"))
    return state


def parse_pid(header_rest: str) -> int | None:
    for token in header_rest.split():
        if token.startswith("pid="):
            try:
                return int(token.split("=", 1)[1])
            except ValueError:
                return None
    return None


def parse_build_root(header_rest: str) -> Path | None:
    for token in header_rest.split():
        if token.startswith("build_root="):
            value = token.split("=", 1)[1]
            return Path(value) if value else None
    return None


def parse_publish_dir(header_rest: str) -> Path | None:
    for token in header_rest.split():
        if token.startswith("publish_dir="):
            value = token.split("=", 1)[1]
            return Path(value) if value else None
    return None


def parse_publish_label(header_rest: str) -> str:
    for token in header_rest.split():
        if token.startswith("publish_label="):
            return token.split("=", 1)[1]
    return ""


def infer_publish_label_from_publish_dir(publish_dir: Path | None) -> str:
    if publish_dir is None:
        return ""
    if publish_dir.parent.name:
        return publish_dir.parent.name
    return publish_dir.name


def parse_diagnostic_error_count(details: str) -> int | None:
    for token in details.split():
        if token.startswith("diagnostic_errors="):
            try:
                return int(token.split("=", 1)[1])
            except ValueError:
                return None
    return None


def parse_current_artifacts_path(final_details: str) -> Path | None:
    for token in final_details.split():
        if token.startswith("current_artifacts=") or token.startswith("product_artifacts="):
            value = token.split("=", 1)[1]
            return Path(value) if value else None
    return None


def pid_is_alive(pid: int | None) -> bool | None:
    if pid is None:
        return None
    return Path(f"/proc/{pid}").exists()


def format_runtime(now_wall: datetime, launched_wall: str) -> str:
    if not launched_wall:
        return "?"
    try:
        start = datetime.fromisoformat(launched_wall)
        delta = (now_wall - start).total_seconds()
    except ValueError:
        return "?"
    total = max(int(delta), 0)
    hours, rem = divmod(total, 3600)
    minutes, seconds = divmod(rem, 60)
    if hours:
        return f"{hours}:{minutes:02d}:{seconds:02d}"
    return f"{minutes}:{seconds:02d}"


def runtime_seconds(now_wall: datetime, launched_wall: str | None) -> int | None:
    if not launched_wall:
        return None
    try:
        start = datetime.fromisoformat(launched_wall)
        delta = (now_wall - start).total_seconds()
    except ValueError:
        return None
    return max(int(delta), 0)


def task_snapshot(task: TaskState, now_wall: datetime) -> dict:
    runtime = runtime_seconds(now_wall, task.launched_wall)
    return {
        "task": task.task,
        "launched_at": task.launched_at,
        "launched_wall": task.launched_wall,
        "weight": task.weight,
        "status": task.status,
        "details": task.details,
        "completed_at": task.completed_at,
        "completed_wall": task.completed_wall,
        "runtime_seconds": runtime,
        "runtime": format_runtime(now_wall, task.launched_wall),
    }


def state_snapshot(
    state: BuildState,
    log_path: Path,
    completed_limit: int | None = 20,
    now_wall: datetime | None = None,
) -> dict:
    now_wall = now_wall or datetime.now(timezone.utc)
    diagnostics = read_diagnostics_state(state)
    pid_alive = pid_is_alive(state.pid)
    current_artifacts_path = parse_current_artifacts_path(state.final_details)
    if (
        current_artifacts_path is None
        and state.diagnostic_error_count is not None
        and state.publish_dir is not None
    ):
        current_artifacts_path = product_artifacts_path(state.publish_dir)
    if (
        current_artifacts_path is None
        and state.diagnostic_error_count is not None
        and state.build_root is not None
    ):
        current_artifacts_path = latest_current_artifacts_path(state.build_root)

    result = "in_progress"
    if state.final_result == "PASS":
        result = "pass"
    elif state.final_result == "FAIL":
        result = "fail"

    return {
        "schema_version": 1,
        "generated_at_utc": now_wall.isoformat().replace("+00:00", "Z"),
        "log": {
            "path": str(log_path),
            "exists": log_path.exists(),
            "last_line": state.last_line,
            "last_timestamp": state.last_timestamp,
        },
        "build": {
            "publish_label": state.publish_label or None,
            "header": state.header,
            "cycle_summary": state.cycle_summary,
            "bundle_cycle": state.bundle_cycle,
            "build_root": str(state.build_root) if state.build_root is not None else None,
            "publish_dir": str(state.publish_dir) if state.publish_dir is not None else None,
            "current_artifacts": (
                str(current_artifacts_path) if current_artifacts_path is not None else None
            ),
        },
        "progress": {
            "total_tasks": state.total_tasks,
            "launched": state.launched,
            "completed": state.completed,
            "pending": state.pending_count(),
            "active": len(state.active_tasks()),
            "running_units": state.running_units,
            "work_unit_budget": state.work_unit_budget,
            "completion_fraction": (
                state.completed / state.total_tasks if state.total_tasks > 0 else None
            ),
            "scheduled_fraction": (
                state.launched / state.total_tasks if state.total_tasks > 0 else None
            ),
        },
        "result": {
            "status": result,
            "raw": state.final_result,
            "at": state.final_at,
            "details": state.final_details,
        },
        "diagnostics": {
            "status": diagnostics.status,
            "text": diagnostics.text,
            "color": diagnostics.color,
        },
        "process": {
            "pid": state.pid,
            "alive": pid_alive,
        },
        "tasks": {
            "active": [task_snapshot(task, now_wall) for task in state.active_tasks()],
            "completed": [
                task_snapshot(task, now_wall)
                for task in (
                    state.recent_completed(completed_limit)
                    if completed_limit is not None
                    else [state.tasks[key] for key in reversed(state.completion_order)]
                )
            ],
        },
    }


def read_diagnostics_state(state: BuildState) -> DiagnosticsState:
    current_artifacts_path = parse_current_artifacts_path(state.final_details)
    if (
        current_artifacts_path is None
        and state.diagnostic_error_count is not None
        and state.publish_dir is not None
    ):
        current_artifacts_path = product_artifacts_path(state.publish_dir)
    if (
        current_artifacts_path is None
        and state.diagnostic_error_count is not None
        and state.build_root is not None
    ):
        current_artifacts_path = latest_current_artifacts_path(state.build_root)
    if current_artifacts_path is None:
        if state.diagnostic_error_count is not None:
            return diagnostics_from_count(state.diagnostic_error_count, "from log")
        return DiagnosticsState(
            status="pending",
            text="diagnostics: waiting for current_artifacts",
            color="yellow",
        )

    try:
        current = json.loads(current_artifacts_path.read_text(encoding="utf-8"))
    except OSError as exc:
        if state.diagnostic_error_count is not None:
            return diagnostics_from_count(
                state.diagnostic_error_count,
                f"from log; current_artifacts unreadable: {exc}",
            )
        return DiagnosticsState(
            status="unreadable",
            text=f"diagnostics: unable to read {current_artifacts_path}: {exc}",
            color="yellow",
        )
    except json.JSONDecodeError as exc:
        return DiagnosticsState(
            status="unreadable",
            text=f"diagnostics: invalid JSON in {current_artifacts_path}: {exc}",
            color="yellow",
        )

    current = select_diagnostics_manifest(current)
    if current is None:
        if state.diagnostic_error_count is not None:
            return diagnostics_from_count(state.diagnostic_error_count, "from log")
        return DiagnosticsState(
            status="missing",
            text=f"diagnostics: no manifests in {current_artifacts_path.name}",
            color="yellow",
        )

    diagnostics = current.get("diagnostics")
    if not isinstance(diagnostics, dict):
        if state.diagnostic_error_count is not None:
            return diagnostics_from_count(state.diagnostic_error_count, "from log")
        return DiagnosticsState(
            status="missing",
            text=f"diagnostics: no diagnostics entry in {current_artifacts_path.name}",
            color="yellow",
        )

    filename = diagnostics.get("filename")
    manifest_count = diagnostics.get("error_count")
    if not isinstance(filename, str) or not filename:
        return DiagnosticsState(
            status="invalid",
            text=f"diagnostics: invalid diagnostics filename in {current_artifacts_path.name}",
            color="yellow",
        )

    diagnostics_path = diagnostics_manifest_path(current_artifacts_path, current, filename)
    if diagnostics_path is None:
        return DiagnosticsState(
            status="invalid",
            text=(
                "diagnostics: invalid packaged artifact root in "
                f"{current_artifacts_path.name}"
            ),
            color="yellow",
        )
    try:
        payload = json.loads(diagnostics_path.read_text(encoding="utf-8"))
    except OSError as exc:
        count = manifest_count if isinstance(manifest_count, int) else None
        if count is not None:
            return diagnostics_from_count(count, f"{filename}; unreadable: {exc}")
        return DiagnosticsState(
            status="unreadable",
            text=f"diagnostics: unable to read {filename}: {exc}",
            color="yellow",
        )
    except json.JSONDecodeError as exc:
        return DiagnosticsState(
            status="unreadable",
            text=f"diagnostics: invalid JSON in {filename}: {exc}",
            color="yellow",
        )

    count = payload.get("error_count")
    if not isinstance(count, int):
        count = manifest_count if isinstance(manifest_count, int) else 0
    return diagnostics_from_count(count, filename)


def latest_current_artifacts_path(build_root: Path) -> Path | None:
    latest_alias = publication_root_for_build_root(build_root) / "current_artifacts.json"
    if latest_alias.is_file():
        return latest_alias
    return None


def product_artifacts_path(publish_dir: Path) -> Path | None:
    path = publish_dir / "product_artifacts.json"
    return path if path.is_file() else None


def select_diagnostics_manifest(current: object) -> dict | None:
    if isinstance(current, dict):
        return current
    if not isinstance(current, list):
        return None
    manifests = [manifest for manifest in current if isinstance(manifest, dict)]
    if not manifests:
        return None
    for manifest in manifests:
        if isinstance(manifest.get("diagnostics"), dict):
            return manifest
    return manifests[0]


def publication_root_for_build_root(build_root: Path) -> Path:
    return build_root / "published"


def publication_root_for_manifest(manifest_path: Path) -> Path:
    if manifest_path.name == "product_artifacts.json":
        return manifest_path.parent.parent.parent
    return manifest_path.parent


def diagnostics_manifest_path(
    current_artifacts_path: Path, current: dict, filename: str
) -> Path | None:
    roots = current.get("artifact_roots")
    if not isinstance(roots, dict):
        return None
    packaged = roots.get("packaged")
    if not isinstance(packaged, str) or not packaged:
        return None
    packaged_path = Path(packaged)
    filename_path = Path(filename)
    if (
        packaged_path.is_absolute()
        or filename_path.is_absolute()
        or ".." in packaged_path.parts
        or ".." in filename_path.parts
    ):
        return None
    return publication_root_for_manifest(current_artifacts_path) / packaged_path / filename_path


def diagnostics_from_count(error_count: int, source: str) -> DiagnosticsState:
    if error_count > 0:
        return DiagnosticsState(
            status="errors",
            text=f"diagnostics: ERROR count={error_count} source={source}",
            color="yellow",
        )
    return DiagnosticsState(
        status="ok",
        text=f"diagnostics: OK count=0 source={source}",
        color="green",
    )


def draw_line(stdscr, row: int, col: int, text: str, attr: int, max_x: int) -> None:
    if row < 0:
        return
    clipped = text[: max(0, max_x - col - 1)]
    stdscr.addstr(row, col, clipped, attr)


def run_ui(stdscr, log_path: Path, refresh_seconds: float) -> None:
    curses.curs_set(0)
    curses.use_default_colors()
    curses.init_pair(1, curses.COLOR_GREEN, -1)
    curses.init_pair(2, curses.COLOR_CYAN, -1)
    curses.init_pair(3, curses.COLOR_WHITE, -1)
    curses.init_pair(4, curses.COLOR_YELLOW, -1)
    curses.init_pair(5, curses.COLOR_RED, -1)

    while True:
        state = read_state(log_path)
        max_y, max_x = stdscr.getmaxyx()
        stdscr.erase()

        draw_line(
            stdscr,
            0,
            0,
            f"log: {log_path}",
            curses.color_pair(2) | curses.A_BOLD,
            max_x,
        )
        draw_line(
            stdscr,
            1,
            0,
            (
                f"publish={state.publish_label or '?'} {state.header}"
                if state.header
                else "waiting for build header..."
            ),
            curses.A_DIM,
            max_x,
        )

        summary = (
            f"publish={state.publish_label or '?'} "
            f"bundle_cycle={state.bundle_cycle} "
            f"active={len(state.active_tasks())} "
            f"completed={state.completed}/{state.total_tasks or '?'} "
            f"scheduled={state.launched}/{state.total_tasks or '?'} "
            f"running_units={state.running_units}/{state.work_unit_budget or '?'} "
            f"pending={state.pending_count()}"
        )
        draw_line(stdscr, 3, 0, summary, curses.A_BOLD, max_x)
        if state.cycle_summary:
            draw_line(stdscr, 4, 0, state.cycle_summary, curses.A_DIM, max_x)
            status_row = 5
            diagnostics_row = 6
            liveness_row = 7
            row = 9
        else:
            status_row = 4
            diagnostics_row = 5
            liveness_row = 6
            row = 8
        if state.final_result == "PASS":
            status_text = f"result=PASS {state.final_at or ''} {state.final_details}".strip()
            status_attr = curses.color_pair(1) | curses.A_BOLD
        elif state.final_result == "FAIL":
            status_text = f"result=FAIL {state.final_at or ''} {state.final_details}".strip()
            status_attr = curses.color_pair(5) | curses.A_BOLD
        else:
            status_text = "result=in_progress"
            status_attr = curses.color_pair(4) | curses.A_BOLD
        draw_line(stdscr, status_row, 0, status_text, status_attr, max_x)
        diagnostics = read_diagnostics_state(state)
        diagnostics_attr = curses.color_pair(1 if diagnostics.color == "green" else 4)
        draw_line(
            stdscr,
            diagnostics_row,
            0,
            diagnostics.text,
            diagnostics_attr | curses.A_BOLD,
            max_x,
        )
        pid_alive = pid_is_alive(state.pid)
        if pid_alive is None:
            liveness = "pid=unknown"
            liveness_attr = curses.A_DIM
        elif pid_alive:
            liveness = f"pid={state.pid} alive"
            liveness_attr = curses.color_pair(1) | curses.A_BOLD
        else:
            liveness = f"pid={state.pid} dead"
            liveness_attr = curses.color_pair(4) | curses.A_BOLD
        draw_line(stdscr, liveness_row, 0, liveness, liveness_attr, max_x)

        draw_line(
            stdscr,
            row,
            0,
            "Active Tasks",
            curses.color_pair(1) | curses.A_BOLD,
            max_x,
        )
        row += 1

        active = state.active_tasks()
        now_wall = datetime.now(timezone.utc)
        if not active:
            draw_line(stdscr, row, 2, "(none)", curses.A_DIM, max_x)
            row += 1
        else:
            for task in active[: max(0, (max_y - row - 8) // 2)]:
                draw_line(
                    stdscr,
                    row,
                    2,
                    f"{task.launched_at}  {task.task}  weight={task.weight}  runtime={format_runtime(now_wall, task.launched_wall)}",
                    curses.color_pair(1),
                    max_x,
                )
                row += 1

        row += 1
        draw_line(
            stdscr,
            row,
            0,
            "Recent Completed",
            curses.color_pair(3) | curses.A_BOLD,
            max_x,
        )
        row += 1

        remaining_rows = max_y - row - 3
        for task in state.recent_completed(max(0, remaining_rows)):
            details = f" {task.details}" if task.details else ""
            draw_line(
                stdscr,
                row,
                2,
                f"{task.completed_at or '?'}  {task.task}{details}",
                curses.A_DIM,
                max_x,
            )
            row += 1

        footer = (
            f"last: {state.last_line[: max(0, max_x - 25)]}"
            if state.last_line
            else "last: (none yet)"
        )
        draw_line(
            stdscr,
            max_y - 2,
            0,
            footer,
            curses.color_pair(4),
            max_x,
        )
        draw_line(
            stdscr,
            max_y - 1,
            0,
            "q to quit",
            curses.A_DIM,
            max_x,
        )

        stdscr.timeout(int(refresh_seconds * 1000))
        stdscr.refresh()
        key = stdscr.getch()
        if key in (ord("q"), ord("Q")):
            break


def print_json_snapshot(log_path: Path, completed_limit: int | None) -> None:
    snapshot = state_snapshot(read_state(log_path), log_path, completed_limit)
    print(json.dumps(snapshot, indent=2, sort_keys=True))


def run_json_watch(log_path: Path, refresh_seconds: float, completed_limit: int | None) -> None:
    while True:
        snapshot = state_snapshot(read_state(log_path), log_path, completed_limit)
        try:
            print(json.dumps(snapshot, sort_keys=True), flush=True)
        except BrokenPipeError:
            sys.stdout = open(os.devnull, "w")
            return
        time.sleep(refresh_seconds)


def parse_listen_address(value: str) -> tuple[str, int]:
    if ":" not in value:
        return value, 8097
    host, port = value.rsplit(":", 1)
    return host or "127.0.0.1", int(port)


def run_web_server(log_path: Path, listen: str, refresh_seconds: float) -> None:
    host, port = parse_listen_address(listen)

    class Handler(BaseHTTPRequestHandler):
        server_version = "AerobagBuildWatch/1"

        def log_message(self, format: str, *args) -> None:
            return

        def do_HEAD(self) -> None:
            self._handle_get(send_body=False)

        def do_GET(self) -> None:
            self._handle_get(send_body=True)

        def _handle_get(self, send_body: bool) -> None:
            path = urlparse(self.path).path
            if path == "/" or path == "/index.html":
                self._send_bytes(
                    200,
                    build_dashboard_html(refresh_seconds).encode("utf-8"),
                    "text/html; charset=utf-8",
                    send_body,
                )
                return
            if path == "/api/state":
                payload = state_snapshot(read_state(log_path), log_path, completed_limit=None)
                self._send_bytes(
                    200,
                    json.dumps(payload, sort_keys=True).encode("utf-8"),
                    "application/json",
                    send_body,
                )
                return
            if path == "/health.json":
                self._send_bytes(200, b'{"ok":true}\n', "application/json", send_body)
                return
            self._send_bytes(404, b"not found\n", "text/plain; charset=utf-8", send_body)

        def _send_bytes(
            self, status: int, body: bytes, content_type: str, send_body: bool = True
        ) -> None:
            self.send_response(status)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            if send_body:
                self.wfile.write(body)

    server = ThreadingHTTPServer((host, port), Handler)
    print(f"watch_build_log serving {log_path} on http://{host}:{port}/", flush=True)
    server.serve_forever()


def build_dashboard_html(refresh_seconds: float) -> str:
    refresh_ms = max(int(refresh_seconds * 1000), 250)
    return f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Aerobag Build Watch</title>
  <style>
    :root {{
      color-scheme: dark;
      --bg: #101312;
      --panel: #171b19;
      --panel-strong: #1f2522;
      --line: #303833;
      --text: #edf3ee;
      --muted: #a9b5ad;
      --green: #50d890;
      --yellow: #f0c85a;
      --red: #ff6b6b;
      --cyan: #66d9e8;
    }}
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0;
      min-height: 100vh;
      background: var(--bg);
      color: var(--text);
      font: 14px/1.45 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      letter-spacing: 0;
    }}
    main {{
      max-width: 1440px;
      margin: 0 auto;
      padding: 24px;
    }}
    header {{
      display: grid;
      grid-template-columns: 1fr auto;
      gap: 16px;
      align-items: start;
      margin-bottom: 18px;
    }}
    h1 {{
      margin: 0;
      font-size: 24px;
      font-weight: 700;
    }}
    .subtle {{ color: var(--muted); }}
    .mono {{ font-family: ui-monospace, "SFMono-Regular", Consolas, monospace; }}
    .pill {{
      display: inline-flex;
      align-items: center;
      min-height: 28px;
      padding: 4px 10px;
      border: 1px solid var(--line);
      background: var(--panel-strong);
      color: var(--text);
      border-radius: 6px;
      font-weight: 650;
      white-space: nowrap;
    }}
    .ok {{ color: var(--green); }}
    .warn {{ color: var(--yellow); }}
    .bad {{ color: var(--red); }}
    .info {{ color: var(--cyan); }}
    .grid {{
      display: grid;
      grid-template-columns: repeat(4, minmax(0, 1fr));
      gap: 12px;
      margin-bottom: 12px;
    }}
    .panel {{
      border: 1px solid var(--line);
      background: var(--panel);
      border-radius: 8px;
      padding: 14px;
      min-width: 0;
    }}
    .metric-label {{
      color: var(--muted);
      font-size: 12px;
      text-transform: uppercase;
      letter-spacing: .08em;
    }}
    .metric-value {{
      margin-top: 6px;
      font-size: 26px;
      line-height: 1.1;
      font-weight: 760;
    }}
    .bar {{
      height: 10px;
      background: #0b0d0c;
      border: 1px solid var(--line);
      border-radius: 6px;
      overflow: hidden;
      margin-top: 10px;
    }}
    .bar > div {{
      height: 100%;
      width: 0%;
      background: var(--green);
      transition: width 180ms linear;
    }}
    .wide {{
      display: grid;
      grid-template-columns: minmax(0, 1.2fr) minmax(0, .8fr);
      gap: 12px;
    }}
    table {{
      width: 100%;
      border-collapse: collapse;
      table-layout: fixed;
    }}
    th, td {{
      text-align: left;
      border-bottom: 1px solid var(--line);
      padding: 8px 6px;
      vertical-align: top;
      overflow-wrap: anywhere;
    }}
    th {{
      color: var(--muted);
      font-size: 12px;
      font-weight: 650;
      text-transform: uppercase;
      letter-spacing: .06em;
    }}
    tr:last-child td {{ border-bottom: 0; }}
    .status-line {{
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
      margin-top: 8px;
    }}
    .header-meta {{
      margin-top: 8px;
      max-width: 100%;
    }}
    .header-meta summary {{
      cursor: pointer;
      color: var(--muted);
      user-select: none;
      width: fit-content;
    }}
    .header-meta pre {{
      margin: 8px 0 0;
      padding: 10px;
      max-height: 160px;
      overflow: auto;
      background: #0b0d0c;
      border: 1px solid var(--line);
      border-radius: 6px;
      color: var(--muted);
      white-space: pre-wrap;
      overflow-wrap: anywhere;
    }}
    .last-line {{
      margin-top: 12px;
      padding: 10px;
      background: #0b0d0c;
      border: 1px solid var(--line);
      border-radius: 6px;
      color: var(--muted);
      min-height: 42px;
      overflow-wrap: anywhere;
    }}
    @media (max-width: 980px) {{
      main {{ padding: 16px; }}
      header, .wide {{ grid-template-columns: 1fr; }}
      .grid {{ grid-template-columns: repeat(2, minmax(0, 1fr)); }}
    }}
    @media (max-width: 560px) {{
      .grid {{ grid-template-columns: 1fr; }}
      .metric-value {{ font-size: 22px; }}
    }}
  </style>
</head>
<body>
  <main>
    <header>
      <div>
        <h1>Aerobag Build Watch</h1>
        <div class="status-line">
          <span id="publishLabel" class="pill info">publish=?</span>
          <span id="cycleLabel" class="pill">cycle=?</span>
        </div>
        <details class="header-meta">
          <summary>Build metadata</summary>
          <pre id="headerLine"></pre>
        </details>
      </div>
      <div id="resultPill" class="pill">connecting</div>
    </header>

    <section class="grid">
      <div class="panel">
        <div class="metric-label">Completed</div>
        <div id="completedMetric" class="metric-value">-</div>
        <div class="bar"><div id="completedBar"></div></div>
      </div>
      <div class="panel">
        <div class="metric-label">Scheduled</div>
        <div id="scheduledMetric" class="metric-value">-</div>
        <div class="bar"><div id="scheduledBar"></div></div>
      </div>
      <div class="panel">
        <div class="metric-label">Running Units</div>
        <div id="unitsMetric" class="metric-value">-</div>
      </div>
      <div class="panel">
        <div class="metric-label">Active Tasks</div>
        <div id="activeMetric" class="metric-value">-</div>
      </div>
    </section>

    <section class="panel">
      <div class="status-line" id="statusLine"></div>
      <div class="last-line mono" id="lastLine"></div>
    </section>

    <section class="wide" style="margin-top:12px">
      <div class="panel">
        <h2 style="margin:0 0 8px;font-size:16px">Active Tasks</h2>
        <table>
          <thead><tr><th style="width:88px">Start</th><th>Task</th><th style="width:88px">Weight</th><th style="width:96px">Runtime</th></tr></thead>
          <tbody id="activeTasks"></tbody>
        </table>
      </div>
      <div class="panel">
        <h2 style="margin:0 0 8px;font-size:16px">Completed</h2>
        <table>
          <thead><tr><th style="width:88px">Done</th><th>Task</th></tr></thead>
          <tbody id="completedTasks"></tbody>
        </table>
      </div>
    </section>
  </main>
  <script>
    const refreshMs = {refresh_ms};
    const cls = (status) => status === "pass" || status === true ? "ok" : status === "fail" || status === false ? "bad" : "warn";
    const text = (value) => value == null || value === "" ? "-" : String(value);
    const pct = (fraction) => fraction == null ? 0 : Math.max(0, Math.min(100, Math.round(fraction * 1000) / 10));
    function set(id, value) {{ document.getElementById(id).textContent = value; }}
    function pill(value, className) {{ return `<span class="pill ${{className || ""}}">${{escapeHtml(value)}}</span>`; }}
    function escapeHtml(value) {{
      return String(value).replace(/[&<>"']/g, (ch) => ({{ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }}[ch]));
    }}
    function taskRows(tasks, active) {{
      if (!tasks.length) {{
        return `<tr><td colspan="${{active ? 4 : 2}}" class="subtle">(none)</td></tr>`;
      }}
      return tasks.map((task) => active
        ? `<tr><td class="mono">${{escapeHtml(text(task.launched_at))}}</td><td class="mono">${{escapeHtml(task.task)}}</td><td>${{task.weight}}</td><td class="mono">${{escapeHtml(text(task.runtime))}}</td></tr>`
        : `<tr><td class="mono">${{escapeHtml(text(task.completed_at))}}</td><td><span class="mono">${{escapeHtml(task.task)}}</span><br><span class="subtle">${{escapeHtml(task.details || "")}}</span></td></tr>`
      ).join("");
    }}
    function metadataLines(state) {{
      const lines = [];
      const seen = new Set();
      function add(line) {{
        if (!line || seen.has(line)) return;
        seen.add(line);
        lines.push(line);
      }}
      function addTokens(source) {{
        if (!source) return;
        for (const token of String(source).split(/\\s+/)) {{
          if (!token) continue;
          if (token.startsWith("+")) {{
            add(`elapsed=${{token}}`);
          }} else if (token.includes("=")) {{
            add(token);
          }}
        }}
      }}
      add(`log_path=${{state.log.path}}${{state.log.exists ? "" : " (missing)"}}`);
      addTokens(state.build.header);
      addTokens(state.build.cycle_summary);
      return lines.join("\\n") || "(none)";
    }}
    function render(state) {{
      const progress = state.progress;
      set("publishLabel", `publish=${{state.build.publish_label || "?"}}`);
      const cycleEl = document.getElementById("cycleLabel");
      if (state.build.bundle_cycle && state.build.bundle_cycle !== "?") {{
        cycleEl.style.display = "";
        cycleEl.textContent = `cycle=${{state.build.bundle_cycle}}`;
      }} else {{
        cycleEl.style.display = "none";
      }}
      set("headerLine", metadataLines(state));
      const result = state.result.status;
      const resultEl = document.getElementById("resultPill");
      resultEl.className = `pill ${{cls(result)}}`;
      resultEl.textContent = result === "in_progress" ? "in progress" : result;
      set("completedMetric", `${{progress.completed}} / ${{progress.total_tasks || "?"}}`);
      set("scheduledMetric", `${{progress.launched}} / ${{progress.total_tasks || "?"}}`);
      set("unitsMetric", `${{progress.running_units}} / ${{progress.work_unit_budget || "?"}}`);
      set("activeMetric", String(progress.active));
      document.getElementById("completedBar").style.width = pct(progress.completion_fraction) + "%";
      document.getElementById("scheduledBar").style.width = pct(progress.scheduled_fraction) + "%";
      const statusParts = [
        pill(`pid=${{state.process.pid || "unknown"}} ${{state.process.alive === true ? "alive" : state.process.alive === false ? "dead" : "unknown"}}`, cls(state.process.alive)),
        pill(`pending=${{progress.pending}}`, "info"),
        pill(`updated=${{state.generated_at_utc}}`, "")
      ];
      if (!(result === "in_progress" && state.diagnostics.status === "pending")) {{
        statusParts.splice(1, 0, pill(state.diagnostics.text, state.diagnostics.color === "green" ? "ok" : "warn"));
      }}
      document.getElementById("statusLine").innerHTML = statusParts.join("");
      set("lastLine", state.log.last_line || "last: (none yet)");
      document.getElementById("activeTasks").innerHTML = taskRows(state.tasks.active, true);
      document.getElementById("completedTasks").innerHTML = taskRows(state.tasks.completed, false);
    }}
    async function refresh() {{
      try {{
        const response = await fetch(new URL("api/state", window.location.href), {{ cache: "no-store" }});
        render(await response.json());
      }} catch (error) {{
        document.getElementById("resultPill").className = "pill bad";
        document.getElementById("resultPill").textContent = String(error);
      }} finally {{
        setTimeout(refresh, refreshMs);
      }}
    }}
    refresh();
  </script>
</body>
</html>
"""


def main() -> int:
    parser = argparse.ArgumentParser(description="Watch an aerobag orchestrator log.")
    parser.add_argument(
        "log_path",
        nargs="?",
        default="/root/aerobag-artifacts/private-work/orchestrator-logs/published/master.log",
        help="Path to master.log",
    )
    parser.add_argument(
        "--refresh-seconds",
        type=float,
        default=2.0,
        help="Refresh interval in seconds",
    )
    parser.add_argument(
        "--recent-limit",
        type=int,
        default=20,
        help="Number of completed tasks to include in JSON snapshots; negative means all",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Print one machine-readable JSON snapshot and exit",
    )
    parser.add_argument(
        "--json-watch",
        action="store_true",
        help="Stream one compact JSON snapshot per line",
    )
    parser.add_argument(
        "--serve",
        nargs="?",
        const="127.0.0.1:8097",
        help="Serve a web dashboard and /api/state JSON endpoint, optionally on host:port",
    )
    args = parser.parse_args()

    log_path = Path(os.path.expanduser(args.log_path))
    completed_limit = None if args.recent_limit < 0 else args.recent_limit
    if args.json:
        print_json_snapshot(log_path, completed_limit)
        return 0
    if args.json_watch:
        run_json_watch(log_path, args.refresh_seconds, completed_limit)
        return 0
    if args.serve:
        run_web_server(log_path, args.serve, args.refresh_seconds)
        return 0
    curses.wrapper(run_ui, log_path, args.refresh_seconds)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
